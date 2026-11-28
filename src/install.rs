//! A difficulty table's song from its links into the library: the song's
//! archive (through [`hosts`](crate::hosts)) is unpacked there, its separate
//! chart (差分) goes into the song's folder ([`placement`](crate::placement)).
//! A link that leads to a page instead is [`Done::Page`], for a browser.

use crate::fs::{now, Store};
use crate::library::Library;
use std::path::{Path, PathBuf};

pub(crate) mod entry;
pub mod job;
pub(crate) mod plan;
pub(crate) mod session;

pub use crate::chart::md5_hex;
pub use crate::package::Role;
pub use entry::{Batch, Entry, EntryJob};
pub use plan::{take, Naming, Request};
pub use session::{Session, SessionJob};

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

impl Done {
    /// What it is, in a word, for a program (the CLI's records' `state`;
    /// its text is a front end's to write): `added`, `owned`, `placed`,
    /// `waiting`, `page`, `failed`. The first four are what [`got`] counts.
    pub fn state(&self) -> &'static str {
        match self {
            Done::Added(..) => "added",
            Done::Owned(_) => "owned",
            Done::Placed(_) => "placed",
            Done::Waiting(_) => "waiting",
            Done::Page(_) => "page",
            Done::Failed(_) => "failed",
        }
    }
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

/// The chart's file name in its link.
pub(crate) fn chart_name(u: &str) -> String {
    let path = u.split(['?', '#']).next().unwrap_or("");
    let name = path.rsplit('/').next().unwrap_or("chart.bms");
    safe_name(&crate::net::percent_decode(name))
}

/// `name` as one file name: no path, nothing a file system refuses,
/// Windows' too ([`safe_part`](crate::fs::safe_part)). None: nothing is
/// left.
pub(crate) fn file_name_of(name: &str) -> Option<String> {
    crate::text::safe_file_name(name).and_then(|n| crate::fs::safe_part(&n))
}

/// `name` as one file name ([`file_name_of`]; `download` when nothing is
/// left).
pub fn safe_name(name: &str) -> String {
    file_name_of(name).unwrap_or_else(|| "download".into())
}

/// A name no other download has, of this process or another, and none
/// could make ready for it (a folder of this name is this run's own):
/// `t`, the time (seconds, hex: what a run that died left is known by its
/// age, [`sweep`]), a count, and what cannot be guessed.
pub(crate) fn unique() -> String {
    use std::hash::{BuildHasher, Hasher};
    use std::sync::atomic::{AtomicU64, Ordering};
    static N: AtomicU64 = AtomicU64::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    // (a hasher's keys are the system's random numbers; a web page has
    // none, and no other user)
    let random = std::collections::hash_map::RandomState::new()
        .build_hasher()
        .finish();
    format!("t{:x}-{n}-{random:016x}", now())
}

/// When the folder `name` a download passes through (`.take-…`, `.job-…`,
/// `.in-…`) was made (seconds), by its name ([`unique`]).
fn staged_at(name: &str) -> Option<u64> {
    let rest = [".take-t", ".job-t", ".in-t"]
        .iter()
        .find_map(|p| name.strip_prefix(p))?;
    u64::from_str_radix(rest.split('-').next()?, 16).ok()
}

/// What a run that died left in `downloads` goes: the folders downloads
/// pass through that are a day old. Once a run for each folder, when it is
/// first used.
pub(crate) fn sweep(store: &dyn Store, downloads: &Path) {
    static SWEPT: std::sync::Mutex<std::collections::BTreeSet<PathBuf>> =
        std::sync::Mutex::new(std::collections::BTreeSet::new());
    if !SWEPT.lock().unwrap().insert(downloads.to_path_buf()) {
        return;
    }
    let day_ago = now().saturating_sub(24 * 60 * 60);
    for e in store.read_dir(downloads).unwrap_or_default() {
        let at = staged_at(&crate::fs::file_name(&e.path));
        if e.is_dir && at.is_some_and(|t| t < day_ago) {
            store.remove_dir(&e.path);
        }
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
        progress: None,
    }
}

/// Charts waiting in `downloads` (their song came later) into the folders
/// of `lib` they now belong in (what comes in added to it). A chart `lib`
/// has looked at before, its file as it was, stays where it is unless a
/// folder that holds one of its sounds took files since (only such a
/// folder can change where it belongs).
pub(crate) fn place_waiting(fs: &dyn Store, downloads: &Path, lib: &mut Library) -> Vec<PathBuf> {
    use crate::placement::{
        bring, chart_info, is_chart, locate_by, sounds_in, ChartInfo, Located, Rules,
    };
    let before = std::mem::take(&mut lib.waiting.changed);
    let mut seen = std::mem::take(&mut lib.waiting.charts);
    let mut out = Vec::new();
    for e in fs.read_dir(downloads).unwrap_or_default() {
        if e.is_dir || !is_chart(&e.path) {
            continue;
        }
        let stamp = fs.stamp(&e.path);
        let (info, again) = match seen.remove(&e.path).filter(|(s, _)| *s == stamp) {
            Some((_, info)) => (info, true),
            None => (chart_info(fs, &e.path), false),
        };
        let stays = |i: &ChartInfo| {
            again
                && ![&before, &lib.waiting.changed]
                    .iter()
                    .any(|d| sounds_in(fs, i, lib, d))
        };
        let at = match &info {
            Some(i) if !stays(i) => match locate_by(fs, i, &[], downloads, lib, Rules::default()).0
            {
                Located::Move(dir) => Some(bring(fs, &e.path, &dir).ok().flatten()),
                _ => None,
            },
            _ => None,
        };
        match at {
            Some(Some(at)) => {
                let md5 = crate::chart::read(fs, &at).map(|b| md5_hex(&b));
                lib.add(&at, md5.as_deref());
                out.push(at);
            }
            // (to be tried again)
            Some(None) => {}
            None => {
                lib.waiting.charts.insert(e.path, (stamp, info));
            }
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
    use crate::links::chart_link;
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

    /// A library of 5,000 songs' folders with numbered sounds (`01.wav`
    /// ...), 100 charts waiting for their songs (each two of one song),
    /// and 50 songs coming in one after another, the waiting charts placed
    /// after each: the time of the 50 placings (with a digest of what was
    /// placed, to compare). `cargo test place_waiting_after_installs --
    /// --ignored --nocapture`
    #[test]
    #[ignore = "a measurement"]
    fn place_waiting_after_installs() {
        use crate::fs::{Entry, Fs, Mem};
        use std::hash::{Hash, Hasher};
        use std::io::Result;
        /// `Mem` listing a folder by the range of paths below it.
        #[derive(Default)]
        struct Tree(Mem);
        impl Fs for Tree {
            fn read(&self, p: &Path) -> Result<Vec<u8>> {
                self.0.read(p)
            }
            fn read_dir(&self, p: &Path) -> Result<Vec<Entry>> {
                let files = self.0 .0.lock().unwrap();
                let mut out: Vec<Entry> = Vec::new();
                for k in files.range(p.to_path_buf()..).map(|(k, _)| k) {
                    let Ok(rest) = k.strip_prefix(p) else {
                        break;
                    };
                    let mut c = rest.components();
                    let Some(first) = c.next() else { continue };
                    let path = p.join(first);
                    if out.last().is_none_or(|e| e.path != path) {
                        let is_dir = c.next().is_some();
                        out.push(Entry { path, is_dir });
                    }
                }
                Ok(out)
            }
            fn rename(&self, from: &Path, to: &Path) -> Result<()> {
                self.0.rename(from, to)
            }
            fn stamp(&self, p: &Path) -> Option<(i64, u64)> {
                self.0.stamp(p)
            }
            fn remove(&self, p: &Path) -> Result<()> {
                self.0.remove(p)
            }
        }
        impl Store for Tree {
            fn create(&self, p: &Path, t: Option<i64>) -> Result<Box<dyn std::io::Write + '_>> {
                self.0.create(p, t)
            }
            fn remove_dir(&self, p: &Path) {
                self.0.remove_dir(p)
            }
        }
        let t = Tree::default();
        let put = |p: String, body: String| t.0.put(Path::new(&p), body.as_bytes(), 0);
        let wavs = |n: usize| -> String {
            (1..=n)
                .map(|j| format!("#WAV{j:02} {j:02}.wav\n"))
                .collect()
        };
        let song = |dir: &str, title: &str, artist: &str, sounds: usize| {
            put(
                format!("{dir}/song.bms"),
                format!("#TITLE {title}\n#ARTIST {artist}\n"),
            );
            for j in 1..=sounds {
                put(format!("{dir}/{j:02}.wav"), String::new());
            }
        };
        for i in 0..5000 {
            song(
                &format!("bms/Song {i}"),
                &format!("Song {i}"),
                &format!("Artist {}", i % 300),
                20 + i % 40,
            );
        }
        for k in 0..100 {
            let chart = format!(
                "#TITLE New {} [X{}]\n#ARTIST A{}\n{}",
                k / 2,
                k % 2,
                k / 2,
                wavs(30)
            );
            put(format!("dl/w{k}.bms"), chart);
        }
        let mut lib = Library::new(Path::new("bms"));
        let started = std::time::Instant::now();
        let mut digest = std::collections::hash_map::DefaultHasher::new();
        let mut placed = 0;
        for i in 0..50 {
            let dir = format!("bms/New {i}");
            song(&dir, &format!("New {i} [N]"), &format!("A{i}"), 30);
            lib.add_folder(&t, Path::new(&dir));
            let at = place_waiting(&t, Path::new("dl"), &mut lib);
            placed += at.len();
            at.hash(&mut digest);
        }
        println!(
            "50 installs, 100 waiting, 5,000 folders: {:?}, {placed} placed, digest {:016x}",
            started.elapsed(),
            digest.finish()
        );
        assert_eq!(placed, 100);
    }

    #[test]
    fn unique_names() {
        let (a, b) = (unique(), unique());
        assert_ne!(a, b);
        // the time, a count, and 64 bits that are not the other's
        let parts = |n: &str| n.split('-').map(String::from).collect::<Vec<_>>();
        let (a, b) = (parts(&a), parts(&b));
        assert!(a.len() == 3 && a[0].starts_with('t') && a[2].len() == 16);
        assert_ne!(a[2], b[2]);
        let at = staged_at(&format!(".job-{}", a.join("-")));
        assert!(at.is_some_and(|t| now() - t < 60));
    }

    /// The folders a run that died left go when a downloads folder is
    /// first used, a day old; nothing else there does.
    #[test]
    fn stale_staged_folders_go() {
        let m = crate::fs::Mem::default();
        let dl = Path::new("dl-stale");
        let old = now() - 25 * 60 * 60;
        let kept = [
            format!(".take-{}/a.bms", unique()),
            format!(".job-t{:x}-0-0/download", now() - 23 * 60 * 60),
            // (not named as this version names them: their age is not known)
            ".take-4242-0/a.bms".into(),
            format!("song-t{old:x}-0-0/a.bms"),
            "waiting.bms".into(),
            "scan.json".into(),
        ];
        let gone = [
            format!(".take-t{old:x}-0-00aa/Song/a.bms"),
            format!(".job-t{old:x}-1-00bb/download"),
            format!(".in-t{old:x}-2-00cc/0.old"),
        ];
        for f in kept.iter().chain(&gone) {
            m.put(&dl.join(f), b"x", 0);
        }
        // (a download that is no archive: nothing comes of it but the sweep)
        let lib = &mut Library::new(Path::new("bms-stale"));
        let blob = crate::fs::Blob::Bytes(b"<html>".to_vec());
        take(&m, blob, &Request::default(), dl, lib);
        for f in &kept {
            assert!(m.get(&dl.join(f)).is_some(), "{f}");
        }
        for f in &gone {
            assert!(m.get(&dl.join(f)).is_none(), "{f}");
        }
        // once a run: what looks old later is not this run's to judge
        m.put(&dl.join(&gone[0]), b"x", 0);
        sweep(&m, dl);
        assert!(m.get(&dl.join(&gone[0])).is_some());
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
        // a name Windows keeps for a device, or would cut the end of
        assert_eq!(safe_name("NUL.zip"), "_NUL.zip");
        assert_eq!(safe_name("song.bms ."), "song.bms");
        assert_eq!(safe_name(" . "), "download");
        assert_eq!(chart_name("https://x.org/a/con.bms%20"), "_con.bms");
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

    /// Each outcome's word; those `got` counts are the first four.
    #[test]
    fn states() {
        let all = [
            Done::Added("a".into(), 1),
            Done::Owned("a".into()),
            Done::Placed("a/x.bms".into()),
            Done::Waiting("x.bms".into()),
            Done::Page("https://1drv.ms/x".into()),
            Done::Failed(Failure::NoLink),
        ];
        let states: Vec<_> = all.iter().map(Done::state).collect();
        assert_eq!(
            states,
            ["added", "owned", "placed", "waiting", "page", "failed"]
        );
        for (i, d) in all.iter().enumerate() {
            assert_eq!(got(std::slice::from_ref(d)), i < 4, "{d:?}");
        }
    }
}
