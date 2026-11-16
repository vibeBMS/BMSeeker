//! A difficulty table's song from its links into the library: the song's
//! archive (through [`hosts`](crate::hosts)) is unpacked there, its separate
//! chart (差分) goes into the song's folder ([`placement`](crate::placement)).
//! A link that leads to a page instead is [`Done::Page`], for a browser.

use crate::fs::Fs;
use crate::library::Library;
use std::path::{Path, PathBuf};

/// What became of one link.
#[derive(Debug)]
pub enum Done {
    /// Unpacked into this folder (files written).
    Added(PathBuf, usize),
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
    /// What a link found by name brought does not hold the chart wanted
    /// ([`NOT_THE_CHART`]).
    NotTheChart,
    /// The song is in the library and none of its chart's links works
    /// ([`NO_CHART_LINK`]).
    NoChartLink,
    /// No link to try.
    NoLink,
    /// Anything else (unpacking, the disk), as said.
    Other(String),
}

/// A download found by name that does not hold the chart wanted.
pub const NOT_THE_CHART: &str = "what it brought does not hold the chart";

/// What a chart's download comes to when its song is in the library
/// already and none of the chart's links brought it (nor led to a page).
pub const NO_CHART_LINK: &str = "the song is in the library; no link for this chart works";

impl Failure {
    /// The download was cancelled (nothing to tell: the person did it).
    pub fn cancelled(&self) -> bool {
        matches!(self, Failure::Net(crate::net::NetError::Cancelled))
    }
}

impl std::fmt::Display for Failure {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Failure::Net(e) => e.fmt(f),
            Failure::NotTheChart => f.write_str(NOT_THE_CHART),
            Failure::NoChartLink => f.write_str(NO_CHART_LINK),
            Failure::NoLink => f.write_str("no link"),
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
    md5::Md5::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// A link to a chart file of its own (`….bms` and the like).
pub fn chart_link(u: &str) -> bool {
    let path = u.split(['?', '#']).next().unwrap_or("");
    crate::placement::is_chart(Path::new(path))
}

/// The chart's file name in its link.
pub fn chart_name(u: &str) -> String {
    let path = u.split(['?', '#']).next().unwrap_or("");
    let name = path.rsplit('/').next().unwrap_or("chart.bms");
    safe_name(&crate::net::percent_decode(name))
}

/// `name` as one file name (no path, nothing a file system refuses).
pub fn safe_name(name: &str) -> String {
    crate::text::safe_file_name(name).unwrap_or_else(|| "download".into())
}

/// The charts in a separate chart's unpacked archive `dir` into their
/// songs' folders, each with the files beside it there (extra sounds, a
/// BGA; one the song has already stays unless this one is newer).
pub fn place_charts(fs: &dyn Fs, dir: &Path, library: &Path) -> Vec<Done> {
    // the library read once for all of them
    place_charts_in(fs, dir, &mut Library::new(library))
}

/// The MD5 of the chart `p` (None: it cannot be read).
fn md5_of(fs: &dyn Fs, p: &Path) -> Option<String> {
    fs.read(p).ok().map(|b| md5_hex(&b))
}

/// [`place_charts`] into `index` (what comes in added to it).
pub fn place_charts_in(fs: &dyn Fs, dir: &Path, index: &mut Library) -> Vec<Done> {
    use crate::placement::{locate_in, Rules};
    let mut out = Vec::new();
    let mut todo = vec![dir.to_path_buf()];
    while let Some(d) = todo.pop() {
        let entries = fs.read_dir(&d).unwrap_or_default();
        todo.extend(entries.iter().filter(|e| e.is_dir).map(|e| e.path.clone()));
        let files: Vec<&PathBuf> = entries
            .iter()
            .filter(|e| !e.is_dir)
            .map(|e| &e.path)
            .collect();
        let mut song_dir = None;
        for chart in files.iter().filter(|p| crate::placement::is_chart(p)) {
            use crate::placement::Located;
            let locate =
                |siblings| locate_in(fs, chart, index, siblings, Rules::default()).map(|(l, _)| l);
            let here = locate(true);
            // a song's whole archive (a chart of it found there): its folder
            // in the library, by what that holds itself; the song's own
            // files are there already
            let whole = here == Some(Located::Here);
            let at = if whole { locate(false) } else { here };
            match at {
                Some(Located::Move(to)) => {
                    if let Some(at) = crate::placement::bring(fs, chart, &to) {
                        index.add(&at, md5_of(fs, &at).as_deref());
                        out.push(Done::Placed(at));
                        if !whole {
                            song_dir = Some(to);
                        }
                    }
                }
                _ => out.push(Done::Waiting((*chart).clone())),
            }
        }
        // the files it came with (smart overwrite)
        if let Some(to) = song_dir {
            for f in files.iter().filter(|p| !crate::placement::is_chart(p)) {
                if let Some(at) = crate::placement::bring(fs, f, &to) {
                    index.add(&at, None);
                }
            }
        }
    }
    out
}

/// One of a song's links (`diff`: its separate chart's) on a disk: into
/// `downloads`, then into `library` (an archive unpacked, a chart into its
/// song's folder; a separate chart's archive into its song's folder too).
/// With `check` (a chart's MD5), only if what it brings holds that chart.
/// A separate chart whose song is not here yet waits in `downloads` (only
/// `md5`, the chart wanted, when it is known: a pack of many songs' charts
/// keeps no others).
#[cfg(not(target_arch = "wasm32"))]
#[allow(clippy::too_many_arguments)]
pub fn link(
    downloads: &Path,
    library: &Path,
    want: &crate::hosts::Want,
    link: &str,
    diff: bool,
    md5: &str,
    check: Option<&str>,
    cx: &crate::hosts::Context,
) -> Vec<Done> {
    let lib = &mut Library::new(library);
    link_in(downloads, lib, want, link, diff, md5, check, cx)
}

/// [`link`] into the library `lib` (what comes in added to it).
#[cfg(not(target_arch = "wasm32"))]
#[allow(clippy::too_many_arguments)]
pub fn link_in(
    downloads: &Path,
    lib: &mut Library,
    want: &crate::hosts::Want,
    link: &str,
    diff: bool,
    md5: &str,
    check: Option<&str>,
    cx: &crate::hosts::Context,
) -> Vec<Done> {
    // this download's own folder for what passes through (two songs of
    // one title, or two downloads at once, do not meet)
    let job = downloads.join(format!(".job-{}", unique()));
    if let Err(e) = std::fs::create_dir_all(&job) {
        return vec![Done::Failed(e.to_string().into())];
    }
    let out = link_job(&job, downloads, lib, want, link, diff, md5, check, cx);
    let _ = std::fs::remove_dir_all(&job);
    out
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

/// [`link_in`], what passes through in `job`.
#[cfg(not(target_arch = "wasm32"))]
#[allow(clippy::too_many_arguments)]
fn link_job(
    job: &Path,
    downloads: &Path,
    lib: &mut Library,
    want: &crate::hosts::Want,
    link: &str,
    diff: bool,
    md5: &str,
    check: Option<&str>,
    cx: &crate::hosts::Context,
) -> Vec<Done> {
    use crate::fs::Disk;
    use crate::import::{self, Imported};
    let title = want.title.as_str();
    let file = if chart_link(link) {
        cx.stage(crate::progress::Stage::Finding);
        let file = downloads.join(chart_name(link));
        if let Err(e) = crate::hosts::download(link, &file, cx) {
            let _ = std::fs::remove_file(&file);
            return vec![Done::Failed(e.into())];
        }
        // a page where a chart was expected
        let bytes = std::fs::read(&file).unwrap_or_default();
        if bytes.trim_ascii_start().starts_with(b"<") {
            let _ = std::fs::remove_file(&file);
            return vec![Done::Page(link.to_string())];
        }
        if check.is_some_and(|m| !md5_hex(&bytes).eq_ignore_ascii_case(m.trim())) {
            let _ = std::fs::remove_file(&file);
            return vec![Done::Failed(Failure::NotTheChart)];
        }
        file
    } else {
        let tmp = job.join("song");
        // the song's own link may be a pack of many songs: its archive
        let want = (!diff).then(|| want.clone());
        match crate::hosts::fetch(link, want, &tmp, cx) {
            Ok(()) => {
                let ext = crate::hosts::sniff(&tmp).unwrap_or("zip");
                // (the library folder is named after it)
                let file = job.join(format!("{}.{ext}", safe_name(title)));
                if std::fs::rename(&tmp, &file).is_err() {
                    return vec![Done::Failed("could not keep the download".into())];
                }
                file
            }
            Err(crate::hosts::Fetch::Page) => return vec![Done::Page(link.to_string())],
            Err(crate::hosts::Fetch::Failed(e)) => {
                let _ = std::fs::remove_file(&tmp);
                return vec![Done::Failed(e.into())];
            }
        }
    };
    cx.stage(crate::progress::Stage::Unpacking);
    if diff && !chart_link(link) {
        let dir = job.join("diff");
        let name = import::file_name(&file);
        let r = import::unpack_into(&file, &name, &dir);
        let _ = std::fs::remove_file(&file);
        let out = match r {
            Ok(_) => place_diff_in(&Disk, &dir, downloads, lib, md5, check),
            Err(e) => vec![Done::Failed(e.into())],
        };
        let _ = std::fs::remove_dir_all(&dir);
        return out;
    }
    // found by name: looked into before it goes into the library
    if let Some(m) = check.filter(|_| !chart_link(link)) {
        let dir = job.join("check");
        let name = import::file_name(&file);
        let holds = import::unpack_into(&file, &name, &dir).is_ok()
            && (holds_chart(&Disk, &dir, m) || holds_song(&Disk, &dir, want));
        let _ = std::fs::remove_dir_all(&dir);
        if !holds {
            let _ = std::fs::remove_file(&file);
            return vec![Done::Failed(Failure::NotTheChart)];
        }
    }
    let mut out = vec![match import::import_in(&Disk, &file, lib) {
        Ok(Imported::Unpacked(dir, n)) => Done::Added(dir, n),
        Ok(Imported::Placed(p)) => Done::Placed(p),
        Ok(Imported::Waiting(p)) | Ok(Imported::Ask(p, _)) => Done::Waiting(p),
        Ok(Imported::Other(p)) => Done::Failed(format!("{} is not a song", p.display()).into()),
        Err(e) => Done::Failed(e.into()),
    }];
    // charts that waited for this song
    if matches!(out[0], Done::Added(..)) {
        out.extend(
            place_waiting_in(&Disk, downloads, lib)
                .into_iter()
                .map(Done::Placed),
        );
    }
    out
}

/// A separate chart's archive, unpacked into `dir` (to be removed after):
/// with `check` (a chart's MD5, the archive found by name), nothing unless
/// it holds that chart; else its charts into their songs' folders
/// ([`place_charts`]), those whose songs are not here waiting in
/// `downloads` ([`keep_waiting`]).
pub fn place_diff(
    fs: &dyn Fs,
    dir: &Path,
    downloads: &Path,
    library: &Path,
    md5: &str,
    check: Option<&str>,
) -> Vec<Done> {
    place_diff_in(fs, dir, downloads, &mut Library::new(library), md5, check)
}

/// [`place_diff`] into `lib` (what comes in added to it).
pub fn place_diff_in(
    fs: &dyn Fs,
    dir: &Path,
    downloads: &Path,
    lib: &mut Library,
    md5: &str,
    check: Option<&str>,
) -> Vec<Done> {
    if check.is_some_and(|m| !holds_chart(fs, dir, m)) {
        return vec![Done::Failed(Failure::NotTheChart)];
    }
    keep_waiting(fs, place_charts_in(fs, dir, lib), downloads, md5)
}

/// The charts of `done` that wait for their songs, out of the folder about
/// to go and into `downloads` (where [`place_waiting`] finds them once a
/// song comes): the chart `md5` only, when it is known. Nothing placed
/// and nothing kept: [`Failure::NotTheChart`] (a pack of other songs'
/// charts).
pub fn keep_waiting(fs: &dyn Fs, done: Vec<Done>, downloads: &Path, md5: &str) -> Vec<Done> {
    let md5 = md5.trim().to_ascii_lowercase();
    let kept = done
        .into_iter()
        .filter_map(|d| match d {
            Done::Waiting(p) => {
                if md5.len() == 32 && fs.read(&p).map(|b| md5_hex(&b)).ok()? != md5 {
                    return None;
                }
                let to = downloads.join(p.file_name()?);
                // an older copy of it goes
                if to != p && fs.stamp(&to).is_some() {
                    fs.remove(&to);
                }
                (to == p || fs.rename(&p, &to)).then_some(Done::Waiting(to))
            }
            d => Some(d),
        })
        .collect::<Vec<_>>();
    if kept.is_empty() {
        return vec![Done::Failed(Failure::NotTheChart)];
    }
    kept
}

/// Charts waiting in `downloads` (their song came later): place those that
/// now have a folder.
pub fn place_waiting(fs: &dyn Fs, downloads: &Path, library: &Path) -> Vec<PathBuf> {
    // the library read once for all of them
    place_waiting_in(fs, downloads, &mut Library::new(library))
}

/// [`place_waiting`] into `index` (what comes in added to it).
pub fn place_waiting_in(fs: &dyn Fs, downloads: &Path, index: &mut Library) -> Vec<PathBuf> {
    use crate::placement::{bring, is_chart, locate_in, Located, Rules};
    let mut out = Vec::new();
    for e in fs.read_dir(downloads).unwrap_or_default() {
        if e.is_dir || !is_chart(&e.path) {
            continue;
        }
        let at = match locate_in(fs, &e.path, index, false, Rules::default()) {
            Some((Located::Move(dir), _)) => bring(fs, &e.path, &dir),
            _ => None,
        };
        if let Some(at) = at {
            index.add(&at, md5_of(fs, &at).as_deref());
            out.push(at);
        }
    }
    out
}

/// Whether the archive `bytes` (zip, 7z, rar) holds the chart `md5`: what
/// a source found by name brings, looked into in memory. With `song` (a
/// song's own archive, not a separate chart's): or a chart of that song
/// ([`holds_song`]).
pub fn archive_holds_chart(bytes: Vec<u8>, md5: &str, song: Option<&crate::hosts::Want>) -> bool {
    let md5 = md5.trim().to_ascii_lowercase();
    let mut found = false;
    let mut same = false;
    let _ = crate::unpack::unpack_bytes(bytes, |rel, data| {
        if found || !crate::placement::is_chart(&rel) {
            return;
        }
        found = md5_hex(&data) == md5;
        same = same
            || song
                .is_some_and(|w| crate::chart::info(&rel, &data).is_some_and(|c| is_song(&c, w)));
    });
    found || same
}

/// Whether the chart `c` is of the song `want`: its title but for the
/// chart's name, and its artist.
fn is_song(c: &crate::chart::ChartInfo, want: &crate::hosts::Want) -> bool {
    use crate::bmssearch::base_title;
    use crate::placement::{same_artist, same_name};
    let title = base_title(&want.title);
    title.chars().count() >= 2
        && !want.artist.trim().is_empty()
        && same_name(&base_title(&c.title), &title)
        && same_artist(&c.artist, &want.artist)
}

/// Whether the folder `dir` (a song just added) holds the chart `md5`
/// already: its separate chart's links need no trying then (a table may
/// give the song's page as one).
pub fn holds_chart(fs: &dyn Fs, dir: &Path, md5: &str) -> bool {
    let md5 = md5.trim().to_ascii_lowercase();
    if md5.len() != 32 {
        return false;
    }
    let mut todo = vec![dir.to_path_buf()];
    while let Some(d) = todo.pop() {
        for e in fs.read_dir(&d).unwrap_or_default() {
            if e.is_dir {
                todo.push(e.path);
            } else if crate::placement::is_chart(&e.path)
                && fs.read(&e.path).is_ok_and(|b| md5_hex(&b) == md5)
            {
                return true;
            }
        }
    }
    false
}

/// Whether the folder `dir` holds a chart of the song `want` (its title but
/// for the chart's name, and its artist): a song found by name whose chart
/// is a separate one (差分), to come after it.
pub fn holds_song(fs: &dyn Fs, dir: &Path, want: &crate::hosts::Want) -> bool {
    let mut todo = vec![dir.to_path_buf()];
    while let Some(d) = todo.pop() {
        for e in fs.read_dir(&d).unwrap_or_default() {
            if e.is_dir {
                todo.push(e.path);
            } else if crate::placement::is_chart(&e.path)
                && crate::placement::chart_info(fs, &e.path).is_some_and(|c| is_song(&c, want))
            {
                return true;
            }
        }
    }
    false
}

/// The folders a song's outcome added.
pub fn added(done: &[Done]) -> impl Iterator<Item = &Path> {
    done.iter().filter_map(|d| match d {
        Done::Added(dir, _) => Some(dir.as_path()),
        _ => None,
    })
}

/// Whether a link's outcome brought the song (or its chart) in.
pub fn got(done: &[Done]) -> bool {
    done.iter()
        .any(|d| matches!(d, Done::Added(..) | Done::Placed(_) | Done::Waiting(_)))
}

/// What trying one link after another came to: the first that brought it
/// in; else the first page among them (to open in a browser); else the last
/// failure.
#[derive(Default)]
pub struct Tries {
    page: Option<String>,
    failed: Option<Failure>,
}

impl Tries {
    /// A link's outcome: Some(it) once one brought the file in.
    pub fn record(&mut self, done: Vec<Done>) -> Option<Vec<Done>> {
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
    pub fn end(self) -> Vec<Done> {
        vec![match (self.page, self.failed) {
            (Some(p), _) => Done::Page(p),
            (None, Some(e)) => Done::Failed(e),
            (None, None) => Done::Failed(Failure::NoLink),
        }]
    }
}

/// [`link`] for each of `own` (the file's own links, in order) until one
/// brings it in; then the links each of `sources` knows for `q` (asked one
/// after another, only while nothing has come in).
#[cfg(not(target_arch = "wasm32"))]
pub fn links(
    downloads: &Path,
    library: &Path,
    q: &crate::sources::Query,
    own: &[String],
    sources: &crate::sources::Sources,
    cx: &crate::hosts::Context,
) -> Vec<Done> {
    links_in(downloads, &mut Library::new(library), q, own, sources, cx)
}

/// [`links`] into the library `lib` (read once for many songs; what comes
/// in added to it).
#[cfg(not(target_arch = "wasm32"))]
pub fn links_in(
    downloads: &Path,
    lib: &mut Library,
    q: &crate::sources::Query,
    own: &[String],
    sources: &crate::sources::Sources,
    cx: &crate::hosts::Context,
) -> Vec<Done> {
    let cancelled = || cx.cancelled();
    let mut tries = Tries::default();
    let mut tried: Vec<String> = Vec::new();
    let md5 = q.md5.trim();
    let check = (md5.len() == 32).then_some(md5);
    let mut try_all = |links: Vec<String>, checked: bool, tries: &mut Tries| -> Option<Vec<Done>> {
        for l in links {
            if cancelled() {
                return Some(vec![Done::Failed(crate::net::NetError::Cancelled.into())]);
            }
            if tried.contains(&l) {
                continue;
            }
            tried.push(l.clone());
            let check = check.filter(|_| checked);
            if let Some(done) =
                tries.record(link_in(downloads, lib, &q.want, &l, q.diff, md5, check, cx))
            {
                return Some(done);
            }
        }
        None
    };
    if let Some(done) = try_all(own.to_vec(), false, &mut tries) {
        return done;
    }
    let http = crate::net::Client::in_context(cx);
    for source in sources.iter() {
        if cancelled() {
            break;
        }
        let found = crate::net::run(&mut source.find(q), &http);
        if let Some(done) = try_all(found, source.checked(), &mut tries) {
            return done;
        }
    }
    tries.end()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::net::NetError;

    #[test]
    fn charts_in_an_archive() {
        use std::io::Write;
        let mut out = std::io::Cursor::new(Vec::new());
        {
            let mut z = zip::ZipWriter::new(&mut out);
            let o = zip::write::SimpleFileOptions::default();
            z.start_file("Song/a.bme", o).unwrap();
            z.write_all(b"#TITLE A").unwrap();
            z.start_file("Song/b.txt", o).unwrap();
            z.write_all(b"#TITLE B").unwrap();
            z.start_file("Song/c.bms", o).unwrap();
            z.write_all(b"#TITLE Caelumize [SPA]\n#ARTIST void\n")
                .unwrap();
            z.finish().unwrap();
        }
        let zip = out.into_inner();
        assert!(archive_holds_chart(
            zip.clone(),
            &md5_hex(b"#TITLE A").to_uppercase(),
            None
        ));
        assert!(!archive_holds_chart(
            zip.clone(),
            &md5_hex(b"#TITLE B"),
            None
        ));
        assert!(!archive_holds_chart(
            b"not an archive".to_vec(),
            &md5_hex(b"#TITLE A"),
            None
        ));
        // a song whose chart is a separate one: its own charts are the song's
        let want = |title: &str, artist: &str| crate::hosts::Want {
            title: title.into(),
            artist: artist.into(),
        };
        let other = md5_hex(b"#TITLE X");
        let song = want("Caelumize [SP Cloudy]", "void obj.Cloudy Boy");
        assert!(archive_holds_chart(zip.clone(), &other, Some(&song)));
        assert!(!archive_holds_chart(
            zip.clone(),
            &other,
            Some(&want("Caelum", "void"))
        ));
        assert!(!archive_holds_chart(
            zip,
            &other,
            Some(&want("Caelumize", "someone"))
        ));
    }

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
                let done = link(
                    &root.join("downloads"),
                    &root.join("bms"),
                    &want,
                    &l,
                    var == "BMSEEKER_DIFF",
                    "",
                    None,
                    &crate::hosts::Context::default(),
                );
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

    /// A separate chart found in its song's whole archive goes into the
    /// song's folder in the library (not left waiting: its sounds are
    /// beside it in the archive).
    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn a_chart_from_a_whole_song_archive() {
        use crate::fs::Disk;
        let root = std::env::temp_dir().join(format!("bmseeker-whole-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let (library, unpacked) = (root.join("bms"), root.join("dl").join("song"));
        let chart = |title: &str| {
            let mut c = format!("#TITLE {title}\n#ARTIST Yamajet\n");
            for i in 1..=20 {
                c += &format!("#WAV{i:02} s{i}.wav\n");
            }
            c
        };
        for dir in [library.join("Song"), unpacked.clone()] {
            std::fs::create_dir_all(&dir).unwrap();
            for i in 1..=20 {
                std::fs::write(dir.join(format!("s{i}.wav")), b"x").unwrap();
            }
            std::fs::write(dir.join("hard.bms"), chart("Song [HARD]")).unwrap();
        }
        std::fs::write(unpacked.join("kei.bms"), chart("Song (kei-another 7)")).unwrap();
        let done = place_charts(&Disk, &unpacked, &library);
        assert!(library.join("Song").join("kei.bms").exists(), "{done:?}");
        assert!(
            done.iter().all(|d| matches!(d, Done::Placed(_))),
            "{done:?}"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    /// A separate chart's archive: checked, the chart wanted kept waiting
    /// (no other), then placed once its song comes.
    #[test]
    fn a_separate_charts_archive() {
        use crate::fs::Disk;
        let root = std::env::temp_dir().join(format!("bmseeker-diff-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let (library, downloads) = (root.join("bms"), root.join("downloads"));
        let dir = downloads.join(".diff-x");
        let ours = "#TITLE Song [X]\n#ARTIST alpha / obj: b\n#WAV01 01.wav\n#WAV02 02.wav\n";
        let other = "#TITLE Other [X]\n#ARTIST gamma\n#WAV01 o.wav\n";
        std::fs::create_dir_all(dir.join("pack")).unwrap();
        std::fs::write(dir.join("pack/ours.bms"), ours).unwrap();
        std::fs::write(dir.join("pack/other.bms"), other).unwrap();
        let charts_in = |d: &Path| {
            let mut v: Vec<String> = std::fs::read_dir(d)
                .unwrap()
                .flatten()
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .filter(|n| n.ends_with(".bms"))
                .collect();
            v.sort();
            v
        };
        let not_the_chart = |d: &[Done]| matches!(d, [Done::Failed(Failure::NotTheChart)]);
        let place = |md5: &str, check: Option<&str>| {
            place_diff(&Disk, &dir, &downloads, &library, md5, check)
        };
        // found by name and not holding the chart
        let missing = md5_hex(b"#TITLE Missing");
        assert!(not_the_chart(&place(&missing, Some(&missing))));
        // a pack without the chart wanted: none of its charts kept
        assert!(not_the_chart(&place(&missing, None)));
        assert!(charts_in(&downloads).is_empty());
        // with it: that one waits in the downloads, alone
        let done = place(&md5_hex(ours.as_bytes()), None);
        assert!(
            matches!(&done[..], [Done::Waiting(p)] if *p == downloads.join("ours.bms")),
            "{done:?}"
        );
        assert_eq!(charts_in(&downloads), ["ours.bms"]);
        // its song comes: it goes into the song's folder
        let song = library.join("Song");
        std::fs::create_dir_all(&song).unwrap();
        std::fs::write(
            song.join("song.bms"),
            "#TITLE Song\n#ARTIST alpha\n#WAV01 01.wav\n",
        )
        .unwrap();
        std::fs::write(song.join("01.ogg"), b"x").unwrap();
        std::fs::write(song.join("02.ogg"), b"x").unwrap();
        assert_eq!(
            place_waiting(&Disk, &downloads, &library),
            [song.join("ours.bms")]
        );
        assert!(charts_in(&downloads).is_empty());
        let _ = std::fs::remove_dir_all(&root);
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
        assert_eq!(Failure::NotTheChart.to_string(), NOT_THE_CHART);
        assert_eq!(Failure::from(NetError::Status(404)).to_string(), "HTTP 404");
        assert!(Failure::from(NetError::Cancelled).cancelled());
        assert!(!Failure::Other("cancelled".into()).cancelled());
    }
}
