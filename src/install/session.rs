//! What a run of downloads shares ([`Session`]): the library (what came in
//! added to it) and the batch (each file once), for jobs that run one
//! after another or at once.

use super::entry::{Batch, Entry, EntryJob};
use super::job::Installed;
use super::{Done, Failure};
use crate::fs::Store;
use crate::hosts::Context;
use crate::library::Library;
use crate::net::job::{Fed, Job, Step};
use crate::sources::Sources;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, PoisonError};

/// A run of downloads into one library: its entries' jobs
/// ([`Session::job`]), one after another or several at once (natively on
/// threads of their own, in a web page interleaved), each seeing what the
/// others brought in. A file two jobs want is downloaded once: the second
/// waits for it, then finds its chart in the library ([`Done::Owned`]) or
/// takes what is left of the file's. A copy is the same session.
///
/// The library and the batch are lent to a job for each of its steps,
/// under one lock, never while its effect (a request, a download, a wait)
/// is done: a job waits for the lock only while another unpacks what came.
/// A job that panics while it has them loses the library: every later
/// step of the session's jobs ends at once (a failure, [`LOST`]).
#[derive(Clone)]
pub struct Session(Arc<Shared>);

struct Shared {
    store: Arc<dyn Store + Send + Sync>,
    downloads: PathBuf,
    sources: Sources,
    state: Mutex<State>,
}

struct State {
    lib: Library,
    batch: Batch,
    /// The last job's number (the batch's claims name a job by it).
    jobs: u64,
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
                jobs: 0,
            }),
        }))
    }

    /// The charts the run wants too (MD5s): a separate charts' archive
    /// keeps those of them it holds waiting for their songs.
    pub fn want(&self, md5s: impl IntoIterator<Item = String>) {
        if let Ok(mut s) = self.0.state.lock() {
            s.batch.want(md5s);
        }
    }

    /// The job of `e` ([`EntryJob`]) in this session, its progress and
    /// cancel `cx`'s: its steps as the library is now (the chart first
    /// when another job brought its song). It waits for the lock while
    /// another job's step holds it.
    pub fn job(&self, e: Entry, cx: &Context) -> SessionJob {
        let sh = &self.0;
        let mut state = sh.state.lock();
        let (lib, id) = match &mut state {
            Ok(s) => {
                s.jobs += 1;
                (std::mem::take(&mut s.lib), s.jobs)
            }
            Err(_) => (Library::default(), 0),
        };
        let store = sh.store.clone();
        let mut inner = EntryJob::new(store, sh.downloads.clone(), lib, e, sh.sources.clone(), cx);
        if let Ok(s) = &mut state {
            s.lib = std::mem::take(&mut inner.install.lib);
        }
        inner.install.id = id;
        SessionJob {
            inner,
            session: self.clone(),
        }
    }

    /// `f` of the library as it is now (what came in so far), read in
    /// place: a clone (`with_library(Library::clone)`) shares the charts'
    /// hashes and records until one changes, but copies the index of its
    /// songs' folders once that has been read. It waits for the lock while
    /// a job's step holds it.
    pub fn with_library<T>(&self, f: impl FnOnce(&Library) -> T) -> T {
        let s = self.0.state.lock().unwrap_or_else(PoisonError::into_inner);
        f(&s.lib)
    }
}

/// An entry's job in a [`Session`]: an [`EntryJob`] on the session's
/// library and batch; it ends with what became of its links (the library
/// and batch are the session's). Dropped before its end, the files it was
/// downloading are left to the session's other jobs.
pub struct SessionJob {
    inner: EntryJob,
    session: Session,
}

impl SessionJob {
    /// Then each of `links` on its own ([`EntryJob::addons`]).
    pub fn addons(mut self, links: impl IntoIterator<Item = String>) -> SessionJob {
        self.inner.add(links);
        self
    }

    /// Whether there is nothing to try ([`EntryJob::is_empty`]).
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    /// A step of the inner job (`f`), the session's library and batch lent
    /// to it.
    fn lent(&mut self, f: impl FnOnce(&mut EntryJob) -> Step<Installed>) -> Step<Vec<Done>> {
        let Ok(mut s) = self.session.0.state.lock() else {
            return Step::Done(vec![Done::Failed(Failure::Other(LOST.into()))]);
        };
        let s = &mut *s;
        let install = &mut self.inner.install;
        install.lib = std::mem::take(&mut s.lib);
        install.batch = std::mem::take(&mut s.batch);
        match f(&mut self.inner) {
            Step::Do(e) => {
                let install = &mut self.inner.install;
                s.lib = std::mem::take(&mut install.lib);
                s.batch = std::mem::take(&mut install.batch);
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
        let mut s = (self.session.0.state.lock()).unwrap_or_else(PoisonError::into_inner);
        s.batch.release(self.inner.install.id);
    }
}

#[cfg(test)]
mod tests;
