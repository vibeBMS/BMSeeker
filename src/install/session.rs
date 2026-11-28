//! What a run of downloads shares ([`Session`]): the library (what came in
//! added to it) and the batch (each file once), for jobs that run one
//! after another or at once.

use super::entry::{holding, untried, Batch, Entry, EntryJob};
use super::job::Installed;
use super::{Done, Failure};
use crate::fs::Store;
use crate::hosts::Context;
use crate::library::Library;
use crate::net::job::{Fed, Job, Step};
use crate::sources::Sources;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

/// A run of downloads into one library: its entries' jobs
/// ([`Session::job`]), one after another or several at once (natively on
/// threads of their own, in a web page interleaved), each seeing what the
/// others brought in. A file two jobs want is downloaded once: the second
/// waits for it, then finds its chart in the library ([`Done::Owned`]) or
/// takes what is left of the file's. A copy is the same session.
///
/// What a file brought is known for the session's life; what failed (a
/// download, a page, a file that brought nothing) answers the jobs of its
/// round: [`Session::again`] starts the next.
///
/// The library and the batch are lent to a job for each of its steps,
/// under one lock, never while its effect (a request, a download, a wait)
/// is done. The longest hold is one step: an archive that came unpacked
/// and applied (seconds for a large one); what waits for it is the other
/// jobs' next steps, [`Session::with_library`], and the drop of a job that
/// has started. Making a job, [`SessionJob::is_empty`], [`Session::want`]
/// and [`Session::again`] do not take it (a screen's thread does not wait
/// for an unpack). A job that panics while it has them loses the library:
/// every later step of the session's jobs ends at once (a failure,
/// [`LOST`]), and [`Session::with_library`] has none.
#[derive(Clone)]
pub struct Session(Arc<Shared>);

struct Shared {
    store: Arc<dyn Store + Send + Sync>,
    downloads: PathBuf,
    sources: Sources,
    state: Mutex<State>,
    /// The charts wanted since the last step (MD5s): into the batch at the
    /// next one. (A lock of its own, held for a copy: `want` does not wait
    /// for a step.)
    wanted: Mutex<Vec<String>>,
    /// The last job's number (the batch's claims name a job by it).
    jobs: AtomicU64,
    /// The round the jobs made now are of ([`Session::again`]).
    round: AtomicU64,
}

struct State {
    lib: Library,
    batch: Batch,
}

/// What a session's job ends with once a job panicked with its library.
pub(crate) const LOST: &str = "the run's library was lost";

impl Session {
    /// Downloads into `lib` (natively through `downloads`), each entry
    /// from its links and then `sources`.
    pub fn new(
        store: Arc<dyn Store + Send + Sync>,
        downloads: PathBuf,
        lib: Library,
        sources: Sources,
    ) -> Session {
        Session(Arc::new(Shared {
            store,
            downloads,
            sources,
            state: Mutex::new(State {
                lib,
                batch: Batch::default(),
            }),
            wanted: Mutex::default(),
            jobs: AtomicU64::new(0),
            round: AtomicU64::new(0),
        }))
    }

    /// The charts the run wants too (MD5s): a separate charts' archive
    /// keeps those of them it holds waiting for their songs. Without
    /// waiting for a job's step: the jobs' steps from the next one on see
    /// them.
    pub fn want(&self, md5s: impl IntoIterator<Item = String>) {
        let md5s: Vec<String> = md5s.into_iter().collect();
        let mut w = self.0.wanted.lock().unwrap_or_else(PoisonError::into_inner);
        w.extend(md5s);
    }

    /// A new round (a retry of what failed): the jobs made from now on try
    /// again what failed for the jobs before (a download, a file that came
    /// but brought nothing, a page: a page's download may come now); what
    /// came in stays known, and a file another job downloads is still
    /// waited for. Without waiting for a job's step.
    pub fn again(&self) {
        self.0.round.fetch_add(1, Ordering::Relaxed);
    }

    /// The job of `e` in this session, its progress and cancel `cx`'s.
    /// Nothing is read of the library until it starts: its steps
    /// ([`EntryJob`]'s) are made then, from the library as it is (the
    /// chart first when another job brought its song; a chart another job
    /// brought: [`Done::Owned`] at once).
    pub fn job(&self, e: Entry, cx: &Context) -> SessionJob {
        SessionJob {
            id: self.0.jobs.fetch_add(1, Ordering::Relaxed) + 1,
            round: self.0.round.load(Ordering::Relaxed),
            session: self.clone(),
            job: Lent::Made(Box::new((e, Vec::new(), cx.clone()))),
        }
    }

    /// `f` of the library as it is now (what came in so far), read in
    /// place: a clone (`with_library(Library::clone)`) shares the charts'
    /// hashes and records until one changes, but copies the index of its
    /// songs' folders once that has been read. It waits for the lock while
    /// a job's step holds it, so `f` must not call it again nor drop a job
    /// of the session that has started. None: the library was lost (a job
    /// panicked with it).
    pub fn with_library<T>(&self, f: impl FnOnce(&Library) -> T) -> Option<T> {
        let s = self.0.state.lock().ok()?;
        Some(f(&s.lib))
    }
}

/// A session's job before and after it starts.
enum Lent {
    /// Its entry, add-ons and context.
    Made(Box<(Entry, Vec<String>, Context)>),
    Started(Box<EntryJob>),
}

/// An entry's job in a [`Session`]: an [`EntryJob`] on the session's
/// library and batch, made when it starts; it ends with what became of its
/// links (the library and batch are the session's). Dropped before its
/// end, the files it was downloading are left to the session's other
/// jobs.
pub struct SessionJob {
    id: u64,
    round: u64,
    session: Session,
    job: Lent,
}

impl SessionJob {
    /// Then each of `links` on its own ([`EntryJob::addons`]).
    pub fn addons(mut self, links: impl IntoIterator<Item = String>) -> SessionJob {
        match &mut self.job {
            Lent::Made(m) => m.1.extend(links),
            Lent::Started(j) => j.add(links),
        }
        self
    }

    /// Whether there is nothing to try, whatever the library has: no link
    /// a download could try, no MD5 for the sources. (An entry whose chart
    /// the library has when it starts ends [`Done::Owned`].)
    pub fn is_empty(&self) -> bool {
        match &self.job {
            Lent::Made(m) => untried(&m.0, &m.1),
            Lent::Started(j) => j.is_empty(),
        }
    }

    /// A step of the job (`f`), the session's library and batch lent to
    /// it; the first makes its steps from them.
    fn lent(&mut self, f: impl FnOnce(&mut EntryJob) -> Step<Installed>) -> Step<Vec<Done>> {
        let sh = &self.session.0;
        let Ok(mut s) = sh.state.lock() else {
            return Step::Done(vec![Done::Failed(Failure::Other(LOST.into()))]);
        };
        let s = &mut *s;
        let wanted = std::mem::take(&mut *sh.wanted.lock().unwrap_or_else(PoisonError::into_inner));
        s.batch.want(wanted);
        let lib = std::mem::take(&mut s.lib);
        let lib = match &mut self.job {
            Lent::Made(m) => {
                let (e, addons, cx) = &mut **m;
                if let Some(dir) = holding(&lib, &e.hashes()) {
                    s.lib = lib;
                    return Step::Done(vec![Done::Owned(dir)]);
                }
                let (store, downloads) = (sh.store.clone(), sh.downloads.clone());
                let e = std::mem::take(e);
                let mut j = EntryJob::new(store, downloads, lib, e, sh.sources.clone(), cx);
                j.add(std::mem::take(addons));
                (j.install.id, j.install.round) = (self.id, self.round);
                self.job = Lent::Started(Box::new(j));
                None
            }
            Lent::Started(_) => Some(lib),
        };
        let Lent::Started(job) = &mut self.job else {
            unreachable!("started above")
        };
        if let Some(lib) = lib {
            job.install.lib = lib;
        }
        job.install.batch = std::mem::take(&mut s.batch);
        match f(job) {
            Step::Do(e) => {
                s.lib = std::mem::take(&mut job.install.lib);
                s.batch = std::mem::take(&mut job.install.batch);
                Step::Do(e)
            }
            Step::Done(out) => {
                (s.lib, s.batch) = (out.lib, out.batch);
                Step::Done(out.done)
            }
        }
    }
}

impl Job for SessionJob {
    type Output = Vec<Done>;
    fn start(&mut self) -> Step<Vec<Done>> {
        self.lent(EntryJob::start)
    }
    fn feed(&mut self, fed: Fed) -> Step<Vec<Done>> {
        self.lent(|j| j.feed(fed))
    }
}

impl Drop for SessionJob {
    fn drop(&mut self) {
        // (one not started has claimed nothing: no lock)
        if let Lent::Started(_) = self.job {
            let mut s = (self.session.0.state.lock()).unwrap_or_else(PoisonError::into_inner);
            s.batch.release(self.id);
        }
    }
}

#[cfg(test)]
mod tests;
