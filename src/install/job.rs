//! Downloads as data. A [`Job`] says what it needs done next (an
//! [`Effect`]: a request, a file downloaded, a wait) and is fed what came
//! of it ([`Fed`]), until it is done; it does no I/O itself. A platform's
//! driver does the effects: natively [`run_job`] with
//! `native::Native`; in a web page, through its fetch and
//! proxy, asynchronously; in tests, canned. A [`Task`] (requests only) is a
//! job too ([`TaskJob`]). A request or a download that meets a server's
//! passing failure (busy; a gateway's error) is tried again here after a
//! wait, so every driver retries alike.
//!
//! [`Acquire`]: one link to its file (the link resolved through the hosts,
//! a page reached through redirects opened from where it ended, a page's
//! other links tried when its file does not come, Google Drive's quota told
//! apart from a page).

use super::{Batch, Done, Failure, Tries};
use crate::fs::Store;
use crate::hosts::{Context, Fetch, Link, Resolver, Target, Want};
use crate::library::Library;
use crate::net::{Answer, BoxTask, NetError, Next, Request, Task};
use crate::sources::{Query, Sources};
use crate::unpack::{Blob, HEAD};
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Something a job needs done.
#[derive(Clone, Debug, PartialEq)]
pub enum Effect {
    /// A request (a page, an API): fed back as [`Fed::Http`].
    Http(Request),
    /// The file `target` downloaded (a MEGA file decrypted; natively into
    /// `slot`): fed back as [`Fed::Downloaded`].
    Download { target: Target, slot: PathBuf },
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
    /// Its first bytes ([`HEAD`] at most): what it is.
    pub head: Vec<u8>,
}

impl Served {
    /// `bytes` downloaded (`head` taken from them).
    pub fn bytes(bytes: Vec<u8>, name: Option<String>, at: Option<String>) -> Served {
        Served {
            head: bytes[..bytes.len().min(HEAD)].to_vec(),
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

impl<T> Step<T> {
    /// The output, once there, as `f` makes it.
    pub(crate) fn map<U>(self, f: impl FnOnce(T) -> U) -> Step<U> {
        match self {
            Step::Do(e) => Step::Do(e),
            Step::Done(out) => Step::Done(f(out)),
        }
    }
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

/// Waits (seconds) before a request or a download is tried again after a
/// server's passing failure ([`NetError::passing`]).
const WAITS: [u64; 2] = [3, 10];

/// The waits of one request or download.
#[derive(Default)]
struct Again(usize);

impl Again {
    /// The wait before `e`'s request is tried again, if it is.
    fn after(&mut self, e: &NetError) -> Option<Effect> {
        let s = WAITS.get(self.0).filter(|_| e.passing())?;
        self.0 += 1;
        Some(Effect::Sleep(Duration::from_secs(*s)))
    }
}

/// A [`Task`] as a job: its requests are [`Effect::Http`] (each tried
/// again after a passing failure).
pub struct TaskJob<T> {
    task: T,
    asked: Option<Request>,
    again: Again,
}

impl<T: Task> TaskJob<T> {
    pub fn new(task: T) -> TaskJob<T> {
        TaskJob {
            task,
            asked: None,
            again: Again::default(),
        }
    }

    fn asked(&mut self, next: Next<T::Output>) -> Step<T::Output> {
        match next {
            Next::Ask(r) => {
                self.again = Again::default();
                self.asked = Some(r.clone());
                Step::Do(Effect::Http(r))
            }
            Next::Done(out) => Step::Done(out),
        }
    }
}

impl<T: Task> Job for TaskJob<T> {
    type Output = T::Output;
    fn start(&mut self) -> Step<T::Output> {
        let next = self.task.start();
        self.asked(next)
    }
    fn feed(&mut self, fed: Fed) -> Step<T::Output> {
        let answer = match (fed, &self.asked) {
            (Fed::Http(Err(e)), _) => match self.again.after(&e) {
                Some(wait) => return Step::Do(wait),
                None => Err(e),
            },
            (Fed::Http(a), _) => a,
            (Fed::Woke, Some(r)) => return Step::Do(Effect::Http(r.clone())),
            (fed, _) => {
                debug_assert!(false, "{fed:?}: no request's answer");
                Err("not a request's answer".into())
            }
        };
        let next = self.task.feed(answer);
        self.asked(next)
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
pub(crate) const MOVES: u8 = 2;

/// The files whose download failed, and why: another link that leads to
/// one (a venue's page naming the links its event's index named, a
/// source's link to the work a table links) fails at once, not after the
/// host's timeout again.
pub(crate) type Dead = Arc<Mutex<HashMap<crate::hosts::DownloadKey, NetError>>>;

/// One link to its file ([`Got`]); a link that is a page for a browser:
/// [`Fetch::Page`].
pub struct Acquire {
    cx: Context,
    want: Option<Want>,
    any: bool,
    slot: PathBuf,
    /// The link resolved through the hosts (its requests asked again
    /// after a wait).
    r: TaskJob<Resolver>,
    /// The file being downloaded, once resolved.
    target: Option<Target>,
    moves: u8,
    /// The download's waits.
    again: Again,
    /// The link is the file's own ([`Acquire::plain`]).
    plain: bool,
    dead: Dead,
}

impl Acquire {
    /// The file `link` leads to for the song `want` (a folder's link: its
    /// archive), natively into `slot`; `any`: a file that is not an
    /// archive too, if it is not a page (a chart, a movie).
    pub(crate) fn new(
        link: &str,
        want: Option<Want>,
        any: bool,
        slot: PathBuf,
        cx: &Context,
    ) -> Acquire {
        Acquire {
            r: TaskJob::new(Resolver::new(link, want.clone(), cx)),
            cx: cx.clone(),
            want,
            any,
            slot,
            target: None,
            moves: 0,
            again: Again::default(),
            plain: false,
            dead: Default::default(),
        }
    }

    /// A file in `dead` failing at once; one whose download fails put
    /// there.
    pub(crate) fn dead(self, dead: Dead) -> Acquire {
        Acquire { dead, ..self }
    }

    /// The file `url` itself (a mirror's), not through the hosts: whatever
    /// it is (a server's passing failure tried again).
    pub(crate) fn plain(url: &str, slot: PathBuf, cx: &Context) -> Acquire {
        Acquire {
            plain: true,
            ..Acquire::new(url, None, true, slot, cx)
        }
    }

    fn resolved(&mut self, step: Step<Result<Target, Fetch>>) -> Step<Result<Got, Fetch>> {
        self.cx.at(self.r.task.link());
        match step {
            Step::Do(e) => Step::Do(e),
            Step::Done(Ok(t)) => {
                let failed = self.dead.lock().unwrap().get(&dead_key(&t)).cloned();
                if let Some(e) = failed {
                    return self.failed(Fetch::Failed(e), None);
                }
                self.again = Again::default();
                self.download(t)
            }
            Step::Done(Err(e)) => self.failed(e, None),
        }
    }

    fn download(&mut self, target: Target) -> Step<Result<Got, Fetch>> {
        let slot = self.slot.clone();
        self.target = Some(target.clone());
        Step::Do(Effect::Download { target, slot })
    }

    /// The file `t` came: what it is.
    fn served(&mut self, s: Served, t: Target) -> Step<Result<Got, Fetch>> {
        let empty = s.head.is_empty();
        let page = empty || is_page(&s.head);
        let kept = self.plain || crate::unpack::sniff(&s.head).is_some();
        if !kept && !(self.any && !page) {
            // (a file wanted as it is that came empty is no page either)
            let e = if self.any && empty {
                Fetch::Failed(NetError::Other("the file is empty".into()))
            } else if crate::hosts::drive_quota(&s.head) {
                Fetch::Failed(NetError::Other(crate::hosts::DRIVE_QUOTA.into()))
            } else {
                Fetch::Page
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

    /// The file `t` did not come: again after a wait, else as a page or a
    /// failure.
    fn unserved(&mut self, u: Unserved, t: Target) -> Step<Result<Got, Fetch>> {
        if let Some(wait) = self.again.after(&u.error).filter(|_| !self.cx.cancelled()) {
            self.target = Some(t);
            return Step::Do(wait);
        }
        if u.error == NetError::Cancelled {
            return self.failed(Fetch::Failed(u.error), None);
        }
        // (a refusal is no page: the service is skipped)
        if self.cx.failures_are_pages && !matches!(u.error, NetError::Refused(_)) {
            let moved = u.at.filter(|a| *a != t.url);
            return self.failed(Fetch::Page, moved);
        }
        let e = u.error.clone();
        self.dead.lock().unwrap().insert(dead_key(&t), e);
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
            host.is_some_and(|h| h != "direct" && h != self.r.task.host())
        });
        match (e, rehost) {
            (Fetch::Page, Some(u)) if !cancelled && self.moves < MOVES => {
                self.moves += 1;
                self.r = TaskJob::new(Resolver::new(&u, self.want.clone(), &self.cx));
                let step = self.r.start();
                self.resolved(step)
            }
            // a mirror the page named, when this file did not come
            (_, _) if !cancelled && self.r.task.try_next() => {
                self.cx.stage(crate::progress::Stage::Finding);
                let step = self.r.start();
                self.resolved(step)
            }
            (e, _) => Step::Done(Err(e)),
        }
    }
}

fn dead_key(t: &Target) -> crate::hosts::DownloadKey {
    crate::hosts::DownloadKey::of(&t.url, None)
}

/// Whether a file's first bytes are a page's.
fn is_page(head: &[u8]) -> bool {
    head.trim_ascii_start().starts_with(b"<")
}

impl Job for Acquire {
    type Output = Result<Got, Fetch>;
    fn start(&mut self) -> Step<Self::Output> {
        self.cx.stage(crate::progress::Stage::Finding);
        if self.plain {
            let url = self.r.task.link().to_string();
            return self.resolved(Step::Done(Ok(Target::url(url))));
        }
        let step = self.r.start();
        self.resolved(step)
    }
    fn feed(&mut self, fed: Fed) -> Step<Self::Output> {
        let step = match (fed, self.target.take()) {
            (Fed::Downloaded(Ok(s)), Some(t)) => return self.served(s, t),
            (Fed::Downloaded(Err(u)), Some(t)) => return self.unserved(u, t),
            (Fed::Woke, Some(t)) => return self.download(t),
            // (cancelled: not asked again after a wait)
            (Fed::Http(Err(e)), None) if self.cx.cancelled() => {
                let next = self.r.task.feed(Err(e));
                self.r.asked(next)
            }
            (fed, _) => self.r.feed(fed),
        };
        self.resolved(step)
    }
}

/// One of [`Files`]' links.
#[derive(Clone, Debug, PartialEq)]
pub struct FileLink {
    pub url: String,
    /// The file's own link (a mirror's: `Acquire::plain`), else an
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
    links: Vec<FileLink>,
    slots: PathBuf,
    got: Vec<Got>,
    current: Option<Acquire>,
}

impl Files {
    /// `links` in order, natively into files named after `slots`.
    pub fn new(links: Vec<FileLink>, slots: PathBuf, cx: &Context) -> Files {
        Files {
            cx: cx.clone(),
            links,
            slots,
            got: Vec::new(),
            current: None,
        }
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
        let Some(link) = self.links.get(i) else {
            return Step::Done(Ok(std::mem::take(&mut self.got)));
        };
        let slot = Files::slot(&self.slots, i);
        let mut a = match link.plain {
            true => Acquire::plain(&link.url, slot, &self.cx),
            false => Acquire::new(&link.url, None, false, slot, &self.cx),
        };
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
                    url: self.links[index].url.clone(),
                    fetch,
                }))
            }
        }
    }
}

impl Job for Files {
    type Output = Result<Vec<Got>, Missed>;
    fn start(&mut self) -> Step<Self::Output> {
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
    pub done: Vec<Done>,
    pub lib: Library,
    pub batch: Batch,
}

/// A link being tried.
struct Trying {
    acquire: Acquire,
    link: String,
    key: crate::hosts::DownloadKey,
    check: bool,
    /// A chart's own link: the chart's file name.
    chart: Option<String>,
    job: PathBuf,
}

/// What [`Install`] waits for.
enum Doing {
    Nothing,
    /// A source's links.
    Finding(TaskJob<BoxTask<Vec<String>>>),
    Trying(Box<Trying>),
}

/// A song (or its separate chart: `q.diff`) from its own links in order,
/// then those each source finds (asked one after another, only while
/// nothing has come in): each link's file ([`Acquire`]; a chart's own link
/// as it is) into the library ([`super::take`]), until one brings it in
/// (`Tries`). A file the batch has had is not downloaded again.
/// Natively each download passes through a folder of its own in
/// `downloads` (`.job-…`).
pub struct Install {
    store: Arc<dyn Store + Send + Sync>,
    downloads: PathBuf,
    pub(super) lib: Library,
    batch: Batch,
    q: Query,
    sources: Sources,
    pub(super) cx: Context,
    links: VecDeque<String>,
    /// The links in `links` were found by name (what they bring must hold
    /// the chart).
    checked: bool,
    next_source: usize,
    doing: Doing,
    tried: HashSet<crate::hosts::DownloadKey>,
    tries: Tries,
}

impl Install {
    /// The song `q` from its own links `own` into `lib` (natively through
    /// `downloads`); no source asked, nothing had.
    pub fn new(
        store: Arc<dyn Store + Send + Sync>,
        downloads: PathBuf,
        lib: Library,
        q: Query,
        own: Vec<String>,
        cx: &Context,
    ) -> Install {
        Install {
            store,
            downloads,
            lib,
            batch: Batch::default(),
            q,
            sources: Sources::default(),
            cx: cx.clone(),
            links: own.into(),
            checked: false,
            next_source: 0,
            doing: Doing::Nothing,
            tried: HashSet::new(),
            tries: Tries::default(),
        }
    }

    /// The sources asked, in order, when its own links bring nothing.
    pub fn sources(mut self, sources: Sources) -> Install {
        self.sources = sources;
        self
    }

    /// What the run has had (a table's missing charts).
    pub(crate) fn batch(mut self, batch: Batch) -> Install {
        self.batch = batch;
        self
    }

    /// This install's library and batch, for the song `q` from `own` and
    /// then `sources`.
    pub(super) fn again(&mut self, q: Query, own: Vec<String>, sources: Sources) {
        let next = Install::new(
            self.store.clone(),
            std::mem::take(&mut self.downloads),
            std::mem::take(&mut self.lib),
            q,
            own,
            &self.cx,
        );
        *self = next.batch(std::mem::take(&mut self.batch)).sources(sources);
    }

    /// `done`, the library and the batch handed back.
    pub(super) fn installed(&mut self, done: Vec<Done>) -> Installed {
        Installed {
            done,
            lib: std::mem::take(&mut self.lib),
            batch: std::mem::take(&mut self.batch),
        }
    }

    /// The next link, else the next source's, else the end.
    pub(super) fn next(&mut self) -> Step<Vec<Done>> {
        loop {
            if self.cx.cancelled() {
                return Step::Done(vec![Done::Failed(NetError::Cancelled.into())]);
            }
            let Some(link) = self.links.pop_front() else {
                break;
            };
            let key = Batch::key(&link, &self.q);
            if !self.tried.insert(key.clone()) {
                continue;
            }
            if self.batch.has(&key) {
                self.tries.record(vec![Done::Failed(Failure::Downloaded)]);
                continue;
            }
            let check = self.checked && crate::chart::md5_key(&self.q.md5).is_some();
            let job = self.downloads.join(format!(".job-{}", super::unique()));
            self.store.create_dir_all(&job);
            let chart = super::chart_link(&link).then(|| super::chart_name(&link));
            let acquire = match &chart {
                Some(name) => Acquire::plain(&link, job.join(name), &self.cx),
                None => {
                    // the song's own link may be a pack of many songs: its archive
                    let want = (!self.q.diff).then(|| self.q.want.clone());
                    Acquire::new(&link, want, false, job.join("download"), &self.cx)
                }
            };
            let mut acquire = acquire.dead(self.batch.dead.clone());
            let step = acquire.start();
            let t = Trying {
                acquire,
                link,
                key,
                check,
                chart,
                job,
            };
            return self.acquiring(Box::new(t), step);
        }
        // the sources' links, one source after another
        let source = self.sources.iter().nth(self.next_source);
        let Some(source) = source.filter(|_| !self.cx.cancelled()) else {
            return Step::Done(std::mem::take(&mut self.tries).end());
        };
        self.next_source += 1;
        self.checked = source.checked();
        self.cx.at(source.name());
        let mut find = TaskJob::new(source.find(&self.q));
        let step = find.start();
        self.finding(find, step)
    }

    fn finding(
        &mut self,
        find: TaskJob<BoxTask<Vec<String>>>,
        step: Step<Vec<String>>,
    ) -> Step<Vec<Done>> {
        match step {
            Step::Do(e) => {
                self.doing = Doing::Finding(find);
                Step::Do(e)
            }
            Step::Done(found) => {
                self.links = found.into();
                self.next()
            }
        }
    }

    fn acquiring(&mut self, t: Box<Trying>, step: Step<Result<Got, Fetch>>) -> Step<Vec<Done>> {
        let got = match step {
            Step::Do(e) => {
                self.doing = Doing::Trying(t);
                return Step::Do(e);
            }
            Step::Done(got) => got,
        };
        let done = match got {
            // a page where a chart was expected
            Ok(g) if t.chart.is_some() && is_page(&g.head) => vec![Done::Page(t.link)],
            Ok(g) => {
                self.cx.stage(crate::progress::Stage::Unpacking);
                let file = t.chart.unwrap_or_else(|| "download".into());
                let mut req = super::request(&self.q, t.check, Some(file), &self.cx.config);
                req.also = self.batch.also(&self.q);
                super::take(&*self.store, g.blob, &req, &self.downloads, &mut self.lib)
            }
            Err(Fetch::Page) => vec![Done::Page(t.link)],
            Err(Fetch::Failed(e)) => vec![Done::Failed(e.into())],
        };
        self.store.remove_dir(&t.job);
        self.batch.tried(t.key, &done);
        match self.tries.record(done) {
            Some(done) => Step::Done(done),
            None => self.next(),
        }
    }

    /// What the effect asked for came to.
    pub(super) fn fed(&mut self, fed: Fed) -> Step<Vec<Done>> {
        match std::mem::replace(&mut self.doing, Doing::Nothing) {
            Doing::Finding(mut f) => {
                let step = f.feed(fed);
                self.finding(f, step)
            }
            Doing::Trying(mut t) => {
                let step = t.acquire.feed(fed);
                self.acquiring(t, step)
            }
            Doing::Nothing => {
                debug_assert!(false, "fed with nothing asked");
                self.next()
            }
        }
    }
}

impl Job for Install {
    type Output = Installed;
    fn start(&mut self) -> Step<Installed> {
        self.next().map(|done| self.installed(done))
    }
    fn feed(&mut self, fed: Fed) -> Step<Installed> {
        self.fed(fed).map(|done| self.installed(done))
    }
}

#[cfg(test)]
pub(crate) mod tests;
