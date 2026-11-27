//! Downloads as data. A [`Job`] says what it needs done next (an
//! [`Effect`]: a request, a file downloaded, a wait) and is fed what came
//! of it ([`Fed`]), until it is done; it does no I/O itself. A platform's
//! driver does the effects: natively [`run_job`] with
//! `native::Native`; in a web page, through its fetch and
//! proxy, asynchronously; in tests, canned. A [`Task`] (requests only) is a
//! job too ([`TaskJob`]). A request or a download that meets a server's
//! passing failure (busy; a gateway's error) is tried again here after a
//! wait, so every driver retries alike. The jobs of a download (a link to
//! its file, a song into the library) are in `install::job`.

use super::{Answer, NetError, Next, Request, Task};
use crate::fs::Blob;
use std::path::PathBuf;
use std::time::Duration;

/// The file a link leads to.
#[derive(Clone, Debug, PartialEq)]
pub struct Target {
    pub url: String,
    /// A MEGA file's 32-byte key: the bytes come encrypted
    /// ([`MegaCipher`](crate::hosts::MegaCipher)).
    pub mega_key: Option<Vec<u8>>,
    /// Its name where the host tells it (a folder's listing, MEGA, the
    /// mirror), made one file name; else the server's or the URL's.
    pub name: Option<String>,
}

impl Target {
    pub fn url(url: impl Into<String>) -> Target {
        Target {
            url: url.into(),
            mega_key: None,
            name: None,
        }
    }

    /// With the name a host told (whatever text it is): one file name of
    /// it, no path.
    pub(crate) fn named(url: impl Into<String>, name: Option<String>) -> Target {
        Target {
            name: name.as_deref().and_then(crate::text::safe_file_name),
            ..Target::url(url)
        }
    }
}

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

/// How many first bytes of a download are kept to tell what it is
/// ([`Served::head`]): what [`sniff`](crate::unpack::sniff) is given (a
/// tar's first header is in them; a gzip's too, compressed).
pub const HEAD: usize = 4096;

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
pub(crate) struct Again(usize);

impl Again {
    /// The wait before `e`'s request is tried again, if it is.
    pub(crate) fn after(&mut self, e: &NetError) -> Option<Effect> {
        let s = WAITS.get(self.0).filter(|_| e.passing())?;
        self.0 += 1;
        Some(Effect::Sleep(Duration::from_secs(*s)))
    }
}

/// A [`Task`] as a job: its requests are [`Effect::Http`] (each tried
/// again after a passing failure).
pub struct TaskJob<T> {
    pub(crate) task: T,
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

    pub(crate) fn asked(&mut self, next: Next<T::Output>) -> Step<T::Output> {
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
