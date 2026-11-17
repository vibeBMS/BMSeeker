//! A difficulty table's song from its links into the library: the song's
//! archive (through [`hosts`](crate::hosts)) is unpacked there, its separate
//! chart (差分) goes into the song's folder ([`placement`](crate::placement)).
//! A link that leads to a page instead is [`Done::Page`], for a browser.

use crate::fs::Fs;
use crate::library::Library;
use std::path::{Path, PathBuf};

pub(crate) mod entry;
pub mod job;
pub(crate) mod plan;

pub use entry::{Batch, Entry, EntryJob};
pub use plan::{take, Naming, Request, Role};

/// What became of one link.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub enum Done {
    /// Unpacked into this folder (files written).
    Added(PathBuf, usize),
    /// Its charts are in the library already, in this song's folder (the
    /// files it lacked brought in).
    Owned(PathBuf),
    /// A chart put into its song's folder.
    Placed(PathBuf),
    /// A chart whose song is not in the library yet.
    Waiting(PathBuf),
    /// A page to open in a browser.
    Page(String),
    Failed(Failure),
}

/// Why a link (or every link) brought nothing. Its text (`Display`) is
/// what a person reads.
#[derive(Clone, Debug, PartialEq)]
pub enum Failure {
    /// The download (or a request on the way) failed; `Cancelled` too.
    Net(crate::net::NetError),
    /// What a link found by name brought does not hold the chart wanted.
    NotTheChart,
    /// The song is in the library and none of its chart's links works (nor
    /// led to a page).
    NoChartLink,
    /// No link to try.
    NoLink,
    /// The link's file this batch of downloads has had already (and what
    /// it brought is in the library, or waits).
    Downloaded,
    /// Anything else (unpacking, the disk), as said.
    Other(String),
}

impl Failure {
    /// The download was cancelled (nothing to tell: the person did it).
    pub fn cancelled(&self) -> bool {
        matches!(self, Failure::Net(crate::net::NetError::Cancelled))
    }

    /// What it is, for a program to read (its text is a person's): the
    /// download's ([`NetError::code`](crate::net::NetError::code)),
    /// `not_the_chart`, `no_chart_link`, `no_link`, `downloaded`, `other`.
    pub fn code(&self) -> &'static str {
        match self {
            Failure::Net(e) => e.code(),
            Failure::NotTheChart => "not_the_chart",
            Failure::NoChartLink => "no_chart_link",
            Failure::NoLink => "no_link",
            Failure::Downloaded => "downloaded",
            Failure::Other(_) => "other",
        }
    }
}

impl std::fmt::Display for Failure {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Failure::Net(e) => e.fmt(f),
            Failure::NotTheChart => f.write_str("what it brought does not hold the chart"),
            Failure::NoChartLink => {
                f.write_str("the song is in the library; no link for this chart works")
            }
            Failure::NoLink => f.write_str("no link"),
            Failure::Downloaded => f.write_str("downloaded already in this run"),
            Failure::Other(e) => f.write_str(e),
        }
    }
}

impl From<crate::net::NetError> for Failure {
    fn from(e: crate::net::NetError) -> Failure {
        Failure::Net(e)
    }
}

impl From<String> for Failure {
    fn from(e: String) -> Failure {
        Failure::Other(e)
    }
}

impl From<&str> for Failure {
    fn from(e: &str) -> Failure {
        Failure::Other(e.into())
    }
}

/// The MD5 of `bytes`, in hex.
pub fn md5_hex(bytes: &[u8]) -> String {
    use md5::Digest;
    crate::text::hex(&md5::Md5::digest(bytes))
}

/// A link to a chart file of its own (`….bms` and the like).
pub(crate) fn chart_link(u: &str) -> bool {
    let path = u.split(['?', '#']).next().unwrap_or("");
    crate::placement::is_chart(Path::new(path))
}

/// The chart's file name in its link.
pub(crate) fn chart_name(u: &str) -> String {
    let path = u.split(['?', '#']).next().unwrap_or("");
    let name = path.rsplit('/').next().unwrap_or("chart.bms");
    safe_name(&crate::net::percent_decode(name))
}

/// `name` as one file name (no path, nothing a file system refuses).
pub fn safe_name(name: &str) -> String {
    crate::text::safe_file_name(name).unwrap_or_else(|| "download".into())
}

/// A name no other download of this process or another has (a web
/// page: of this page).
pub(crate) fn unique() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static N: AtomicU64 = AtomicU64::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    if cfg!(target_arch = "wasm32") {
        // (no process ids there)
        format!("w-{n}")
    } else {
        format!("{}-{n}", std::process::id())
    }
}

/// What a link for `q` brings is for (`check`: found by name, it must
/// hold the chart), as [`take`] needs it; `file`: its file name.
pub fn request(
    q: &crate::sources::Query,
    check: bool,
    file: Option<String>,
    config: &crate::config::Config,
) -> Request {
    Request {
        want: Some(q.want.clone()),
        md5: crate::chart::md5_key(&q.md5),
        org_md5: q.org_md5.clone(),
        role: if q.diff { Role::Diff } else { Role::Song },
        check,
        rules: crate::placement::Rules::default(),
        // (the library folder is named after the song, or its charts)
        naming: Naming::of(config, Naming::Named(safe_name(&q.want.title))),
        file,
        reinstall: config.reinstall,
        also: Vec::new(),
    }
}

/// Charts waiting in `downloads` (their song came later) into the folders
/// of `lib` they now belong in (what comes in added to it).
pub fn place_waiting(fs: &dyn Fs, downloads: &Path, lib: &mut Library) -> Vec<PathBuf> {
    use crate::placement::{bring, is_chart, locate, Located, Rules};
    let mut out = Vec::new();
    for e in fs.read_dir(downloads).unwrap_or_default() {
        if e.is_dir || !is_chart(&e.path) {
            continue;
        }
        let at = match locate(fs, &e.path, lib, false, Rules::default()) {
            Some((Located::Move(dir), _)) => bring(fs, &e.path, &dir).ok().flatten(),
            _ => None,
        };
        if let Some(at) = at {
            let md5 = fs.read(&at).ok().map(|b| md5_hex(&b));
            lib.add(&at, md5.as_deref());
            out.push(at);
        }
    }
    out
}

/// Whether a link's outcome brought the song (or its chart) in, or found
/// it in the library already.
pub fn got(done: &[Done]) -> bool {
    done.iter().any(|d| {
        matches!(
            d,
            Done::Added(..) | Done::Owned(_) | Done::Placed(_) | Done::Waiting(_)
        )
    })
}

/// What trying one link after another came to: the first that brought it
/// in; else the first page among them (to open in a browser); else the last
/// failure.
#[derive(Default)]
pub(crate) struct Tries {
    page: Option<String>,
    failed: Option<Failure>,
}

impl Tries {
    /// A link's outcome: Some(it) once one brought the file in.
    pub(crate) fn record(&mut self, done: Vec<Done>) -> Option<Vec<Done>> {
        if got(&done) {
            return Some(done);
        }
        for d in done {
            match d {
                Done::Page(u) => {
                    self.page.get_or_insert(u);
                }
                Done::Failed(e) => self.failed = Some(e),
                _ => {}
            }
        }
        None
    }

    /// When no link brought it in.
    pub(crate) fn end(self) -> Vec<Done> {
        vec![match (self.page, self.failed) {
            (Some(p), _) => Done::Page(p),
            (None, Some(e)) => Done::Failed(e),
            (None, None) => Done::Failed(Failure::NoLink),
        }]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::net::NetError;

    /// `BMSEEKER_SONG=URL BMSEEKER_DIFF=URL [BMSEEKER_TITLE=… BMSEEKER_ARTIST=…]
    /// cargo test -p bmseeker song_links -- --ignored`
    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    #[ignore = "network"]
    fn song_links() {
        let root = std::env::temp_dir().join("bmseeker-install");
        let _ = std::fs::remove_dir_all(&root);
        let want = crate::hosts::Want {
            title: std::env::var("BMSEEKER_TITLE").unwrap_or("Test Song".into()),
            artist: std::env::var("BMSEEKER_ARTIST").unwrap_or_default(),
        };
        for var in ["BMSEEKER_SONG", "BMSEEKER_DIFF"] {
            if let Ok(l) = std::env::var(var) {
                let q = crate::sources::Query {
                    want: want.clone(),
                    diff: var == "BMSEEKER_DIFF",
                    ..Default::default()
                };
                let lib = Library::new(&root.join("bms"));
                let cx = crate::hosts::Context::default();
                let store = std::sync::Arc::new(crate::fs::Disk);
                let mut job =
                    job::Install::new(store, root.join("downloads"), lib, q, vec![l], &cx);
                let done = job::run_job(&mut job, &crate::native::Native::new(&cx)).done;
                println!("{var}: {done:?}");
            }
        }
    }

    #[test]
    fn unique_names() {
        assert_ne!(unique(), unique());
    }

    #[test]
    fn chart_links() {
        assert!(chart_link("http://x.org/up/song_7A.bme"));
        assert!(chart_link("https://x.org/a/b.bms?dl=1"));
        assert!(!chart_link("https://x.org/a/song.zip"));
        assert!(!chart_link("https://drive.google.com/file/d/X/view"));
        assert_eq!(
            chart_name("https://x.org/a/b%20c%28M%29.bms?dl=1"),
            "b c(M).bms"
        );
        assert_eq!(safe_name("a/b: c?"), "a_b_ c_");
    }

    #[test]
    fn tries_in_order() {
        let mut t = Tries::default();
        assert!(t
            .record(vec![Done::Failed(NetError::Status(404).into())])
            .is_none());
        assert!(t
            .record(vec![Done::Page("https://1drv.ms/x".into())])
            .is_none());
        assert!(t
            .record(vec![Done::Page("https://later/".into())])
            .is_none());
        assert!(matches!(&t.end()[..], [Done::Page(p)] if p == "https://1drv.ms/x"));
        let mut t = Tries::default();
        assert!(t
            .record(vec![Done::Failed(NetError::Status(404).into())])
            .is_none());
        assert!(t.record(vec![Done::Added("a".into(), 3)]).is_some());
        // nothing tried; the last failure; how each reads
        assert!(matches!(
            &Tries::default().end()[..],
            [Done::Failed(Failure::NoLink)]
        ));
        let mut t = Tries::default();
        t.record(vec![Done::Failed(NetError::Status(404).into())]);
        t.record(vec![Done::Failed(Failure::NotTheChart)]);
        assert!(matches!(&t.end()[..], [Done::Failed(Failure::NotTheChart)]));
        assert_eq!(
            Failure::NotTheChart.to_string(),
            "what it brought does not hold the chart"
        );
        assert_eq!(Failure::from(NetError::Status(404)).to_string(), "HTTP 404");
        assert!(Failure::from(NetError::Cancelled).cancelled());
        assert!(!Failure::Other("cancelled".into()).cancelled());
    }
}
