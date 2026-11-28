//! Downloads' jobs on the model of [`crate::net::job`] (its types are
//! named here too).
//!
//! [`Acquire`]: one link to its file (the link resolved through the hosts,
//! a page reached through redirects opened from where it ended, a page's
//! other links tried when its file does not come, Google Drive's quota told
//! apart from a page).

use super::entry::Claim;
use super::{Batch, Done, Failure, Tries};
use crate::fs::{Blob, Store};
use crate::hosts::{Context, Fetch, Link, Resolver, Want};
use crate::library::Library;
pub use crate::net::job::{run_job, Driver, Effect, Fed, Job, Served, Step, TaskJob, Unserved};
use crate::net::job::{Again, Target};
use crate::net::{BoxTask, NetError, Task};
use crate::sources::{Query, Sources};
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// What [`Acquire`] got: the file, its name (where the host told it, else
/// the server's or the URL's, else `download`; one file name, no path:
/// [`safe_name`](super::safe_name)) and its first bytes.
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
        // (a host's and a server's name: whatever they said)
        let safe = |n: Option<String>| n.as_deref().and_then(super::file_name_of);
        let name = safe(t.name)
            .or_else(|| safe(s.name))
            .or_else(|| safe(crate::hosts::url_file_name(&t.url)))
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
            let target = match Link::parse(self.r.task.link(), 0) {
                Some(l) => Ok(Target::url(l.url)),
                None => Err(Fetch::Failed(crate::hosts::NOT_WEB.into())),
            };
            return self.resolved(Step::Done(target));
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
    /// Another job of the batch downloading the next link's file
    /// ([`Claim::Busy`]): asked again when woken.
    Waiting,
}

/// The wait before a file another job downloads is asked for again.
pub(crate) const BUSY_WAIT: Duration = Duration::from_millis(500);

/// Times a file another job downloads is asked for, [`BUSY_WAIT`] apart
/// (ten minutes), before this job takes it to be gone (kept, never fed nor
/// dropped) and downloads the file itself.
pub(crate) const BUSY_ASKS: u32 = 1200;

/// A song (or its separate chart: `q.diff`) from its own links in order,
/// then those each source finds (asked one after another, only while
/// nothing has come in): each link's file ([`Acquire`]; a chart's own link
/// as it is) into the library ([`super::take`]), until one brings it in
/// (`Tries`). A file the batch has had is not downloaded again, nor one
/// another job of the batch downloads (it waits for it: [`Batch::claim`]).
/// Natively each download passes through a folder of its own in
/// `downloads` (`.job-…`).
pub struct Install {
    store: Arc<dyn Store + Send + Sync>,
    downloads: PathBuf,
    pub(super) lib: Library,
    pub(super) batch: Batch,
    /// The job it is of, as the batch's claims name it (0: a job alone).
    pub(super) id: u64,
    /// An entry's chart (its hashes): when another job brings it in, this
    /// one ends ([`Done::Owned`]).
    chart: Vec<String>,
    /// Times the next link's file was asked for while another job
    /// downloads it.
    asks: u32,
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
            id: 0,
            chart: Vec::new(),
            asks: 0,
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

    /// This install's library, batch and job, for the song `q` from `own`
    /// and then `sources`; ended once the library has the chart of one of
    /// `chart`'s hashes.
    pub(super) fn again(
        &mut self,
        q: Query,
        own: Vec<String>,
        sources: Sources,
        chart: Vec<String>,
    ) {
        let next = Install::new(
            self.store.clone(),
            std::mem::take(&mut self.downloads),
            std::mem::take(&mut self.lib),
            q,
            own,
            &self.cx,
        );
        *self = Install {
            id: self.id,
            chart,
            ..next.batch(std::mem::take(&mut self.batch)).sources(sources)
        };
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
            // another job brought it in (a pack of several entries' charts)
            if let Some(dir) = super::entry::holding(&self.lib, &self.chart) {
                return Step::Done(vec![Done::Owned(dir)]);
            }
            let Some(link) = self.links.pop_front() else {
                break;
            };
            let key = Batch::key(&link, &self.q);
            if self.tried.contains(&key) {
                continue;
            }
            match self.batch.claim(&key, self.id, self.asks >= BUSY_ASKS) {
                Claim::Busy => {
                    self.asks += 1;
                    self.links.push_front(link);
                    self.doing = Doing::Waiting;
                    return Step::Do(Effect::Sleep(BUSY_WAIT));
                }
                Claim::Had => {
                    self.asks = 0;
                    self.tried.insert(key);
                    self.tries.record(vec![Done::Failed(Failure::Downloaded)]);
                    continue;
                }
                Claim::Mine => {
                    self.asks = 0;
                    self.tried.insert(key.clone());
                }
            }
            let check = self.checked && crate::chart::md5_key(&self.q.md5).is_some();
            super::sweep(&*self.store, &self.downloads);
            let job = self.downloads.join(format!(".job-{}", super::unique()));
            if let Err(e) = self.store.create_new_dir(&job) {
                self.batch.release(self.id);
                let failed = Failure::Other(format!("{}: {e}", job.display()));
                self.tries.record(vec![Done::Failed(failed)]);
                continue;
            }
            let chart = crate::links::chart_link(&link).then(|| super::chart_name(&link));
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
                req.progress = self.cx.progress.clone();
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
            Doing::Waiting => self.next(),
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
