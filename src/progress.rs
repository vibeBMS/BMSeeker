//! How far a song's download has got, for whoever shows it: a [`Progress`]
//! shared between the download (on its thread, or the web's callbacks) and
//! the screen. The functions that download take it in their
//! [`Context`](crate::hosts::Context) ([`Context::with_progress`]).
//!
//! [`Context::with_progress`]: crate::hosts::Context::with_progress

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
    /// Services that refused BMSeeker and are skipped
    /// ([`crate::refusal`]).
    refused: Mutex<Vec<String>>,
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

    /// Where it is looking or downloading from ("": not said), as
    /// [`Progress::set_at`] was last told.
    pub fn at(&self) -> String {
        self.at.lock().unwrap().clone()
    }

    /// `host` refused BMSeeker (or refused it earlier): it is skipped.
    pub fn refused(&self, host: &str) {
        let mut all = self.refused.lock().unwrap();
        if !all.iter().any(|h| h == host) {
            all.push(host.to_string());
        }
    }

    /// The services that refused and were skipped, for the person waiting.
    pub fn refusals(&self) -> Vec<String> {
        self.refused.lock().unwrap().clone()
    }

    /// A short line for a person, in English: "12.3 / 40.0 MB ·
    /// pixeldrain.net", "Finding the file… (ginger)", "Unpacking…"; while
    /// finding, the services that refused: "· api.bmssearch.net refused,
    /// skipped". A front end in another language writes its own from
    /// [`Progress::stage`], [`Progress::bytes`], [`Progress::at`] and
    /// [`Progress::refusals`].
    pub fn label(&self) -> String {
        let mb = |b: u64| b as f64 / (1024.0 * 1024.0);
        let at = self.at.lock().unwrap();
        let refused = self.refused.lock().unwrap();
        let skipped = match refused.is_empty() {
            true => String::new(),
            false => format!(" · {} refused, skipped", refused.join(", ")),
        };
        match self.stage() {
            Stage::Waiting => "Waiting…".into(),
            Stage::Finding if at.is_empty() => format!("Finding the file…{skipped}"),
            Stage::Finding => format!("Finding the file… ({at}){skipped}"),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels() {
        let p = Progress::new();
        assert_eq!(p.stage(), Stage::Waiting);
        p.start_file(Some(4 << 20));
        p.add(1 << 20);
        assert_eq!(p.label(), "1.0 / 4.0 MB");
        assert_eq!(p.fraction(), Some(0.25));
        // where from
        p.set_at("pixeldrain.net");
        assert_eq!(p.label(), "1.0 / 4.0 MB · pixeldrain.net");
        p.set_stage(Stage::Finding);
        p.set_at("ginger");
        assert_eq!(p.label(), "Finding the file… (ginger)");
        p.refused("bmssearch.net");
        p.refused("bmssearch.net");
        assert_eq!(p.refusals(), ["bmssearch.net"]);
        assert_eq!(
            p.label(),
            "Finding the file… (ginger) · bmssearch.net refused, skipped"
        );
    }

    /// The line from the parts a front end reads, as one would write it
    /// in its own words.
    fn line_of(p: &Progress) -> String {
        let mb = |b: u64| b as f64 / (1024.0 * 1024.0);
        let (at, refused) = (p.at(), p.refusals());
        let skipped = match refused.is_empty() {
            true => String::new(),
            false => format!(" · {} refused, skipped", refused.join(", ")),
        };
        match p.stage() {
            Stage::Waiting => "Waiting…".into(),
            Stage::Finding if at.is_empty() => format!("Finding the file…{skipped}"),
            Stage::Finding => format!("Finding the file… ({at}){skipped}"),
            Stage::Downloading => {
                let size = match p.bytes() {
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

    /// The parts are all the line is made of: in every stage, with a
    /// place or none, refusals or none, a size known or not.
    #[test]
    fn the_line_from_its_parts() {
        let mut seen = std::collections::HashSet::new();
        for at in ["", "pixeldrain.net"] {
            for refused in [&[][..], &["api.bmssearch.net", "mega.nz"]] {
                for total in [None, Some(40 << 20)] {
                    for stage in STAGES {
                        let p = Progress::new();
                        p.start_file(total);
                        p.add(12_900_000);
                        p.set_stage(stage);
                        p.set_at(at);
                        for h in refused {
                            p.refused(h);
                        }
                        assert_eq!(p.at(), at);
                        assert_eq!(line_of(&p), p.label());
                        seen.insert(p.label());
                    }
                }
            }
        }
        // (each kind of line was made)
        assert_eq!(seen.len(), 11);
    }

    #[test]
    fn a_contexts_progress() {
        use crate::hosts::Context;
        let mine = Progress::new();
        let cx = Context::default().with_progress(mine.clone());
        assert!(Arc::ptr_eq(cx.progress.as_ref().unwrap(), &mine));
        cx.stage(Stage::Unpacking);
        assert_eq!(mine.stage(), Stage::Unpacking);
        assert!(!cx.cancelled());
        mine.cancel();
        assert!(cx.cancelled());
    }
}
