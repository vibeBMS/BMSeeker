//! Downloads as data. A [`Job`] says what it needs done next (an
//! [`Effect`]: a request, a file downloaded, a wait) and is fed what came
//! of it ([`Fed`]), until it is done; it does no I/O itself. A platform's
//! driver does the effects: natively [`run_job`] with the native transport
//! (`hosts::Native`: a thread per download, stall and cancel); in a web
//! page, through its fetch and proxy, asynchronously; in tests, canned.
//! A [`Task`] (requests only) is a job too ([`TaskJob`]).
//!
//! [`Acquire`]: one link to its file (the link resolved through the hosts,
//! a page reached through redirects opened from where it ended, a page's
//! other links tried when its file does not come, a server's passing
//! failure tried again, Google Drive's quota told apart from a page).

use crate::hosts::{Context, Fetch, Link, Resolver, Target, Want};
use crate::net::{Answer, NetError, Next, Request, Task};
use crate::unpack::Blob;
use std::path::PathBuf;
use std::time::Duration;

/// Something a job needs done.
#[derive(Clone, Debug, PartialEq)]
pub enum Effect {
    /// A request (a page, an API): fed back as [`Fed::Http`].
    Http(Request),
    /// The file `target` downloaded (a MEGA file decrypted; natively into
    /// `slot`): fed back as [`Fed::Downloaded`]. `any`: a file that is not
    /// an archive is wanted too (if it is not a page).
    Download {
        target: Target,
        slot: PathBuf,
        any: bool,
    },
    /// A wait (before a request is tried again): fed back as [`Fed::Woke`].
    Sleep(Duration),
}

/// What an effect came to.
#[derive(Debug)]
pub enum Fed {
    Http(Answer),
    Downloaded(Result<Served, Unserved>),
    Woke,
}

/// A file downloaded.
#[derive(Debug)]
pub struct Served {
    pub blob: Blob,
    /// The name the server gave it (`Content-Disposition`).
    pub name: Option<String>,
    /// Where the download's redirects ended.
    pub at: Option<String>,
    /// Its first bytes (4 KiB at most): what it is.
    pub head: Vec<u8>,
}

impl Served {
    /// `bytes` downloaded (`head` taken from them).
    pub fn bytes(bytes: Vec<u8>, name: Option<String>, at: Option<String>) -> Served {
        Served {
            head: bytes[..bytes.len().min(4096)].to_vec(),
            blob: Blob::Bytes(bytes),
            name,
            at,
        }
    }
}

/// A download that brought no file: why, and where its redirects ended
/// (when the answer said).
#[derive(Clone, Debug, PartialEq)]
pub struct Unserved {
    pub error: NetError,
    pub at: Option<String>,
}

impl From<NetError> for Unserved {
    fn from(error: NetError) -> Unserved {
        Unserved { error, at: None }
    }
}

/// What a job needs next.
#[derive(Debug, PartialEq)]
pub enum Step<T> {
    Do(Effect),
    Done(T),
}

/// A computation that needs effects done.
pub trait Job: Send {
    type Output;
    /// The first step.
    fn start(&mut self) -> Step<Self::Output>;
    /// What the last [`Step::Do`] came to: the next step.
    fn feed(&mut self, fed: Fed) -> Step<Self::Output>;
}

/// What does a job's effects, one at a time.
pub trait Driver {
    fn perform(&self, effect: Effect) -> Fed;
}

/// Run `job` to its end, `driver` doing its effects.
pub fn run_job<J: Job + ?Sized>(job: &mut J, driver: &dyn Driver) -> J::Output {
    let mut step = job.start();
    loop {
        match step {
            Step::Do(e) => step = job.feed(driver.perform(e)),
            Step::Done(out) => return out,
        }
    }
}

/// A [`Task`] as a job: its requests are [`Effect::Http`].
pub struct TaskJob<T>(pub T);

fn asked<T>(next: Next<T>) -> Step<T> {
    match next {
        Next::Ask(r) => Step::Do(Effect::Http(r)),
        Next::Done(out) => Step::Done(out),
    }
}

impl<T: Task> Job for TaskJob<T> {
    type Output = T::Output;
    fn start(&mut self) -> Step<T::Output> {
        asked(self.0.start())
    }
    fn feed(&mut self, fed: Fed) -> Step<T::Output> {
        match fed {
            Fed::Http(a) => asked(self.0.feed(a)),
            // (fed what it did not ask for: as a failed request)
            _ => asked(self.0.feed(Err("not a request's answer".into()))),
        }
    }
}

/// What [`Acquire`] got: the file, its name (where the host told it, else
/// the server's or the URL's, else `download`) and its first bytes.
#[derive(Debug)]
pub struct Got {
    pub blob: Blob,
    pub name: String,
    pub head: Vec<u8>,
}

/// Times a download's redirects may lead it to another host.
pub const MOVES: u8 = 2;

/// Waits (seconds) before a download is tried again after a server's
/// passing failure (busy; a gateway's error).
const WAITS: [u64; 2] = [3, 10];

/// One link to its file ([`Got`]); a link that is a page for a browser:
/// [`Fetch::Page`].
pub struct Acquire {
    cx: Context,
    want: Option<Want>,
    any: bool,
    slot: PathBuf,
    /// A download's failure is a page (a web page's proxy passes archives
    /// only, refusing anything else).
    pages: bool,
    r: Resolver,
    target: Option<Target>,
    moves: u8,
    waits: usize,
    /// The link is the file's own ([`Acquire::plain`]).
    plain: bool,
}

impl Acquire {
    /// The file `link` leads to for the song `want` (a folder's link: its
    /// archive), natively into `slot`; `any`: a file that is not an
    /// archive too, if it is not a page (a chart, a movie).
    pub fn new(link: &str, want: Option<Want>, any: bool, slot: PathBuf, cx: &Context) -> Acquire {
        Acquire {
            r: Resolver::new(link, want.clone(), cx),
            cx: cx.clone(),
            want,
            any,
            slot,
            pages: false,
            target: None,
            moves: 0,
            waits: 0,
            plain: false,
        }
    }

    /// A download that fails is a page (a web page's proxy).
    pub fn failures_are_pages(mut self) -> Acquire {
        self.pages = true;
        self
    }

    /// The file `url` itself (a mirror's), not through the hosts: whatever
    /// it is (a server's passing failure tried again).
    pub fn plain(url: &str, slot: PathBuf, cx: &Context) -> Acquire {
        Acquire {
            plain: true,
            ..Acquire::new(url, None, true, slot, cx)
        }
    }

    fn resolved(&mut self, next: Next<Result<Target, Fetch>>) -> Step<Result<Got, Fetch>> {
        match next {
            Next::Ask(r) => Step::Do(Effect::Http(r)),
            Next::Done(Ok(t)) => {
                self.target = Some(t);
                self.waits = 0;
                self.download()
            }
            Next::Done(Err(e)) => self.failed(e, None),
        }
    }

    fn download(&self) -> Step<Result<Got, Fetch>> {
        let target = self.target.clone().expect("a target");
        Step::Do(Effect::Download {
            target,
            slot: self.slot.clone(),
            any: self.any,
        })
    }

    /// The file came: what it is.
    fn served(&mut self, s: Served) -> Step<Result<Got, Fetch>> {
        let t = self.target.take().expect("a target");
        let page = s.head.is_empty() || s.head.trim_ascii_start().starts_with(b"<");
        let kept = self.plain || crate::hosts::sniff_bytes(&s.head).is_some();
        if !kept && !(self.any && !page) {
            let e = match crate::hosts::drive_quota(&s.head) {
                true => Fetch::Failed(NetError::Other(crate::hosts::DRIVE_QUOTA.into())),
                false => Fetch::Page,
            };
            let moved = s.at.filter(|u| *u != t.url);
            return self.failed(e, moved);
        }
        let name = t
            .name
            .or(s.name)
            .or_else(|| crate::hosts::url_file_name(&t.url))
            .unwrap_or_else(|| "download".into());
        Step::Done(Ok(Got {
            blob: s.blob,
            name,
            head: s.head,
        }))
    }

    /// The file did not come: again after a wait, else as a page or a
    /// failure.
    fn unserved(&mut self, u: Unserved) -> Step<Result<Got, Fetch>> {
        if u.error.passing() && self.waits < WAITS.len() && !self.cx.cancelled() {
            self.waits += 1;
            return Step::Do(Effect::Sleep(Duration::from_secs(WAITS[self.waits - 1])));
        }
        let t = self.target.take().expect("a target");
        if self.pages && u.error != NetError::Cancelled {
            let moved = u.at.filter(|a| *a != t.url);
            return self.failed(Fetch::Page, moved);
        }
        self.failed(Fetch::Failed(u.error), None)
    }

    /// What failed (`moved`: a page reached through redirects ended
    /// there): opened from where it ended, if a host there knows its way;
    /// else the next link its page named; else the end.
    fn failed(&mut self, e: Fetch, moved: Option<String>) -> Step<Result<Got, Fetch>> {
        let cancelled = self.cx.cancelled();
        let hosts = &self.cx.hosts;
        let rehost = moved.filter(|u| {
            let host = Link::parse(u, 0).and_then(|l| hosts.find(&l).map(|h| h.name()));
            host.is_some_and(|h| h != "direct" && h != self.r.host())
        });
        match (e, rehost) {
            (Fetch::Page, Some(u)) if !cancelled && self.moves < MOVES => {
                self.moves += 1;
                self.r = Resolver::new(&u, self.want.clone(), &self.cx);
                let next = self.r.start();
                self.resolved(next)
            }
            // a mirror the page named, when this file did not come
            (_, _) if !cancelled && self.r.try_next() => {
                self.cx.stage(crate::progress::Stage::Finding);
                let next = self.r.start();
                self.resolved(next)
            }
            (e, _) => Step::Done(Err(e)),
        }
    }
}

impl Job for Acquire {
    type Output = Result<Got, Fetch>;
    fn start(&mut self) -> Step<Self::Output> {
        self.cx.stage(crate::progress::Stage::Finding);
        if self.plain {
            let url = self.r.link().to_string();
            return self.resolved(Next::Done(Ok(Target::url(url))));
        }
        let next = self.r.start();
        self.resolved(next)
    }
    fn feed(&mut self, fed: Fed) -> Step<Self::Output> {
        match fed {
            Fed::Http(a) => {
                let next = self.r.feed(a);
                self.resolved(next)
            }
            Fed::Downloaded(Ok(s)) => self.served(s),
            Fed::Downloaded(Err(u)) => self.unserved(u),
            Fed::Woke => self.download(),
        }
    }
}

/// One of [`Files`]' links.
#[derive(Clone, Debug, PartialEq)]
pub struct Source {
    pub url: String,
    /// The file's own link (a mirror's: [`Acquire::plain`]), else an
    /// author's (through the hosts).
    pub plain: bool,
}

/// Why [`Files`] ended without them all: which link, and what it came to.
#[derive(Clone, Debug, PartialEq)]
pub struct Missed {
    pub index: usize,
    pub url: String,
    pub fetch: Fetch,
}

/// Several links' files, one after another (natively into `<slots>-<i>`):
/// all of them, or where the first that did not come stopped it.
pub struct Files {
    cx: Context,
    sources: Vec<Source>,
    slots: PathBuf,
    pages: bool,
    got: Vec<Got>,
    current: Option<Acquire>,
}

impl Files {
    /// `sources` in order, natively into files named after `slots`.
    pub fn new(sources: Vec<Source>, slots: PathBuf, cx: &Context) -> Files {
        Files {
            cx: cx.clone(),
            sources,
            slots,
            pages: false,
            got: Vec::new(),
            current: None,
        }
    }

    /// A download that fails is a page ([`Acquire::failures_are_pages`]).
    pub fn failures_are_pages(mut self) -> Files {
        self.pages = true;
        self
    }

    /// Where the `i`th file goes (natively).
    pub fn slot(slots: &std::path::Path, i: usize) -> PathBuf {
        let mut name = slots.file_name().unwrap_or_default().to_os_string();
        name.push(format!("-{i}.part"));
        slots.with_file_name(name)
    }

    /// The next file's job started, or the end.
    fn next(&mut self) -> Step<Result<Vec<Got>, Missed>> {
        let i = self.got.len();
        let Some(src) = self.sources.get(i) else {
            return Step::Done(Ok(std::mem::take(&mut self.got)));
        };
        let slot = Files::slot(&self.slots, i);
        let mut a = match src.plain {
            true => Acquire::plain(&src.url, slot, &self.cx),
            false => Acquire::new(&src.url, None, false, slot, &self.cx),
        };
        if self.pages {
            a = a.failures_are_pages();
        }
        let step = a.start();
        self.current = Some(a);
        self.went(step)
    }

    fn went(&mut self, step: Step<Result<Got, Fetch>>) -> Step<Result<Vec<Got>, Missed>> {
        match step {
            Step::Do(e) => Step::Do(e),
            Step::Done(Ok(g)) => {
                self.got.push(g);
                self.current = None;
                self.next()
            }
            Step::Done(Err(fetch)) => {
                let index = self.got.len();
                Step::Done(Err(Missed {
                    index,
                    url: self.sources[index].url.clone(),
                    fetch,
                }))
            }
        }
    }
}

impl Job for Files {
    type Output = Result<Vec<Got>, Missed>;
    fn start(&mut self) -> Step<Self::Output> {
        self.got.clear();
        self.next()
    }
    fn feed(&mut self, fed: Fed) -> Step<Self::Output> {
        let step = match self.current.as_mut() {
            Some(a) => a.feed(fed),
            None => return Step::Done(Ok(std::mem::take(&mut self.got))),
        };
        self.went(step)
    }
}

/// What [`Install`] came to, and the library and batch it was given back
/// (what came in added).
pub struct Installed {
    pub done: Vec<super::Done>,
    pub lib: crate::library::Library,
    pub batch: super::Batch,
}

/// A link being tried.
struct Trying {
    acquire: Acquire,
    link: String,
    key: crate::hosts::DownloadKey,
    check: bool,
    chart: bool,
    job: PathBuf,
}

/// A song (or its separate chart: `q.diff`) from its own links in order,
/// then those each source finds (asked one after another, only while
/// nothing has come in): each link's file ([`Acquire`]; a chart's own link
/// as it is) into the library ([`super::take`]), until one brings it in
/// ([`super::Tries`]). A file the batch has had is not downloaded again.
/// Natively each download passes through a folder of its own in
/// `downloads` (`.job-…`).
pub struct Install {
    store: std::sync::Arc<dyn crate::fs::Store + Send + Sync>,
    downloads: PathBuf,
    lib: Option<crate::library::Library>,
    batch: Option<super::Batch>,
    q: crate::sources::Query,
    sources: crate::sources::Sources,
    cx: Context,
    pages: bool,
    links: std::collections::VecDeque<String>,
    /// The links in `links` were found by name (what they bring must hold
    /// the chart).
    checked: bool,
    next_source: usize,
    finding: Option<TaskJob<crate::net::BoxTask<Vec<String>>>>,
    trying: Option<Trying>,
    tried: std::collections::HashSet<crate::hosts::DownloadKey>,
    tries: super::Tries,
}

impl Install {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        store: std::sync::Arc<dyn crate::fs::Store + Send + Sync>,
        downloads: PathBuf,
        lib: crate::library::Library,
        batch: super::Batch,
        q: crate::sources::Query,
        own: Vec<String>,
        sources: crate::sources::Sources,
        cx: &Context,
    ) -> Install {
        Install {
            store,
            downloads,
            lib: Some(lib),
            batch: Some(batch),
            q,
            sources,
            cx: cx.clone(),
            pages: false,
            links: own.into(),
            checked: false,
            next_source: 0,
            finding: None,
            trying: None,
            tried: Default::default(),
            tries: Default::default(),
        }
    }

    /// A download that fails is a page ([`Acquire::failures_are_pages`]).
    pub fn failures_are_pages(mut self) -> Install {
        self.pages = true;
        self
    }

    /// What its own links bring must hold the chart `q.md5` too (they
    /// were found by name).
    pub fn own_checked(mut self) -> Install {
        self.checked = true;
        self
    }

    fn end(&mut self, done: Vec<super::Done>) -> Step<Installed> {
        Step::Done(Installed {
            done,
            lib: self.lib.take().expect("the library"),
            batch: self.batch.take().expect("the batch"),
        })
    }

    /// The next link, else the next source's, else the end.
    fn next(&mut self) -> Step<Installed> {
        use super::{chart_link, Batch, Done};
        loop {
            if self.cx.cancelled() {
                return self.end(vec![Done::Failed(NetError::Cancelled.into())]);
            }
            let Some(link) = self.links.pop_front() else {
                break;
            };
            let key = Batch::key(&link, &self.q);
            if !self.tried.insert(key.clone()) {
                continue;
            }
            if self.batch.as_ref().is_some_and(|b| b.has(&key)) {
                self.tries
                    .record(vec![Done::Failed(super::DOWNLOADED.into())]);
                continue;
            }
            let check = self.checked && self.q.md5.trim().len() == 32;
            let job = self.downloads.join(format!(".job-{}", super::unique()));
            self.store.create_dir_all(&job);
            let chart = chart_link(&link);
            let mut acquire = if chart {
                Acquire::plain(&link, job.join(super::chart_name(&link)), &self.cx)
            } else {
                // the song's own link may be a pack of many songs: its archive
                let want = (!self.q.diff).then(|| self.q.want.clone());
                Acquire::new(&link, want, false, job.join("download"), &self.cx)
            };
            if self.pages {
                acquire = acquire.failures_are_pages();
            }
            let step = acquire.start();
            self.trying = Some(Trying {
                acquire,
                link,
                key,
                check,
                chart,
                job,
            });
            return self.acquiring(step);
        }
        // the sources' links, one source after another
        if self.cx.cancelled() {
            return Step::Done(self.finish());
        }
        let Some(source) = self.sources.iter().nth(self.next_source) else {
            return Step::Done(self.finish());
        };
        self.next_source += 1;
        self.checked = source.checked();
        let mut find = TaskJob(source.find(&self.q));
        let step = find.start();
        self.finding = Some(find);
        self.finding(step)
    }

    fn finish(&mut self) -> Installed {
        let done = std::mem::take(&mut self.tries).end();
        match self.end(done) {
            Step::Done(i) => i,
            Step::Do(_) => unreachable!(),
        }
    }

    fn finding(&mut self, step: Step<Vec<String>>) -> Step<Installed> {
        match step {
            Step::Do(e) => Step::Do(e),
            Step::Done(found) => {
                self.finding = None;
                self.links = found.into();
                self.next()
            }
        }
    }

    fn acquiring(&mut self, step: Step<Result<Got, Fetch>>) -> Step<Installed> {
        use super::Done;
        let got = match step {
            Step::Do(e) => return Step::Do(e),
            Step::Done(got) => got,
        };
        let t = self.trying.take().expect("a link");
        let done = match got {
            // a page where a chart was expected
            Ok(g) if t.chart && g.head.trim_ascii_start().starts_with(b"<") => {
                vec![Done::Page(t.link.clone())]
            }
            Ok(g) => {
                self.cx.stage(crate::progress::Stage::Unpacking);
                let file = match t.chart {
                    true => super::chart_name(&t.link),
                    false => "download".into(),
                };
                let batch = self.batch.as_ref().expect("the batch");
                let mut req = super::request(&self.q, t.check, Some(file), &self.cx.config);
                req.also = batch.also(&self.q);
                let lib = self.lib.as_mut().expect("the library");
                super::take(&*self.store, g.blob, &req, &self.downloads, lib)
            }
            Err(Fetch::Page) => vec![Done::Page(t.link.clone())],
            Err(Fetch::Failed(e)) => vec![Done::Failed(e.into())],
        };
        self.store.remove_dir(&t.job);
        if let Some(b) = self.batch.as_mut() {
            b.tried(t.key, &done);
        }
        match self.tries.record(done) {
            Some(done) => self.end(done),
            None => self.next(),
        }
    }
}

impl Job for Install {
    type Output = Installed;
    fn start(&mut self) -> Step<Installed> {
        self.next()
    }
    fn feed(&mut self, fed: Fed) -> Step<Installed> {
        if let Some(f) = self.finding.as_mut() {
            let step = f.feed(fed);
            return self.finding(step);
        }
        match self.trying.as_mut() {
            Some(t) => {
                let step = t.acquire.feed(fed);
                self.acquiring(step)
            }
            None => self.next(),
        }
    }
}

#[cfg(test)]
pub(crate) mod tests;
