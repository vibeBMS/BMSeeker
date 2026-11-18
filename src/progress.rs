//! How far a song's download has got, for whoever shows it: a [`Progress`]
//! shared between the download (on its thread, or the web's callbacks) and
//! the screen. The functions that download take it in their
//! [`Context`](crate::hosts::Context) ([`Context::with_progress`]), else
//! the thread's ([`scope`]).
//!
//! [`Context::with_progress`]: crate::hosts::Context::with_progress

use std::cell::RefCell;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicU8, Ordering};
use std::sync::{Arc, Mutex};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stage {
    /// Not started (another song's download is first).
    Waiting,
    /// Reading pages / APIs for the file.
    Finding,
    /// The file coming in (see [`Progress::bytes`]).
    Downloading,
    /// Unpacking it, placing its charts.
    Unpacking,
    /// In the library; the player reading it (its scan).
    Adding,
}

const STAGES: [Stage; 5] = [
    Stage::Waiting,
    Stage::Finding,
    Stage::Downloading,
    Stage::Unpacking,
    Stage::Adding,
];

#[derive(Default, Debug)]
pub struct Progress {
    stage: AtomicU8,
    got: AtomicU64,
    /// 0: not known
    total: AtomicU64,
    cancelled: AtomicBool,
    /// Where it is looking or downloading from ("pixeldrain.net", a
    /// source's name), for the person waiting.
    at: Mutex<String>,
}

/// What a cancelled download fails with.
pub const CANCELLED: &str = "cancelled";

impl Progress {
    pub fn new() -> Arc<Progress> {
        Arc::default()
    }

    pub fn stage(&self) -> Stage {
        STAGES[self.stage.load(Ordering::Relaxed) as usize % STAGES.len()]
    }

    pub fn set_stage(&self, s: Stage) {
        self.stage.store(s as u8, Ordering::Relaxed);
    }

    /// A file starts coming in (`total` bytes, if known).
    pub fn start_file(&self, total: Option<u64>) {
        self.got.store(0, Ordering::Relaxed);
        self.total.store(total.unwrap_or(0), Ordering::Relaxed);
        self.set_stage(Stage::Downloading);
    }

    /// Ask the download to stop (it fails with [`CANCELLED`]).
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Relaxed);
    }

    pub fn cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Relaxed)
    }

    pub fn add(&self, n: u64) {
        self.got.fetch_add(n, Ordering::Relaxed);
    }

    /// (bytes in, of how many if known)
    pub fn bytes(&self) -> (u64, Option<u64>) {
        let t = self.total.load(Ordering::Relaxed);
        (self.got.load(Ordering::Relaxed), (t > 0).then_some(t))
    }

    /// The share in (0..=1), when the size is known.
    pub fn fraction(&self) -> Option<f32> {
        let (got, total) = self.bytes();
        total.map(|t| (got as f64 / t as f64).min(1.0) as f32)
    }

    /// It is looking or downloading at `place` (a host, a source's name).
    pub fn set_at(&self, place: &str) {
        let mut at = self.at.lock().unwrap();
        if *at != place {
            place.clone_into(&mut at);
        }
    }

    /// A short line for a person: "12.3 / 40.0 MB · pixeldrain.net",
    /// "Finding the file… (ginger)", "Unpacking…".
    pub fn label(&self) -> String {
        let mb = |b: u64| b as f64 / (1024.0 * 1024.0);
        let at = self.at.lock().unwrap();
        match self.stage() {
            Stage::Waiting => "Waiting…".into(),
            Stage::Finding if at.is_empty() => "Finding the file…".into(),
            Stage::Finding => format!("Finding the file… ({at})"),
            Stage::Downloading => {
                let size = match self.bytes() {
                    (got, Some(t)) => format!("{:.1} / {:.1} MB", mb(got), mb(t)),
                    (got, None) => format!("{:.1} MB", mb(got)),
                };
                match at.is_empty() {
                    true => size,
                    false => format!("{size} · {at}"),
                }
            }
            Stage::Unpacking => "Unpacking…".into(),
            Stage::Adding => "Adding to the library…".into(),
        }
    }
}

thread_local! {
    static CURRENT: RefCell<Option<Arc<Progress>>> = const { RefCell::new(None) };
}

/// Run `f` with `p` as this thread's progress (what downloads report to
/// when their [`Context`](crate::hosts::Context) names none). Prefer
/// [`Context::with_progress`](crate::hosts::Context::with_progress): work
/// moved to another thread loses this one.
pub fn scope<R>(p: Option<Arc<Progress>>, f: impl FnOnce() -> R) -> R {
    let old = CURRENT.with(|c| c.replace(p));
    let r = f();
    CURRENT.with(|c| *c.borrow_mut() = old);
    r
}

/// This thread's progress, if any.
pub(crate) fn current() -> Option<Arc<Progress>> {
    CURRENT.with(|c| c.borrow().clone())
}

/// Set this thread's progress' stage (if any).
pub fn stage(s: Stage) {
    if let Some(p) = current() {
        p.set_stage(s);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels() {
        let p = Progress::new();
        assert_eq!(p.stage(), Stage::Waiting);
        scope(Some(p.clone()), || {
            stage(Stage::Finding);
            current().unwrap().start_file(Some(4 << 20));
            current().unwrap().add(1 << 20);
        });
        assert!(current().is_none());
        assert_eq!(p.label(), "1.0 / 4.0 MB");
        assert_eq!(p.fraction(), Some(0.25));
        // where from
        p.set_at("pixeldrain.net");
        assert_eq!(p.label(), "1.0 / 4.0 MB · pixeldrain.net");
        p.set_stage(Stage::Finding);
        p.set_at("ginger");
        assert_eq!(p.label(), "Finding the file… (ginger)");
    }

    #[test]
    fn a_contexts_progress() {
        use crate::hosts::Context;
        let (mine, thread) = (Progress::new(), Progress::new());
        let cx = Context::default().with_progress(mine.clone());
        assert!(Context::default().progress().is_none());
        scope(Some(thread.clone()), || {
            // its own first; the thread's for a context without one
            assert!(Arc::ptr_eq(&cx.progress().unwrap(), &mine));
            assert!(Arc::ptr_eq(
                &Context::default().progress().unwrap(),
                &thread
            ));
        });
        cx.stage(Stage::Unpacking);
        assert_eq!(mine.stage(), Stage::Unpacking);
        assert!(!cx.cancelled());
        mine.cancel();
        assert!(cx.cancelled());
    }
}
