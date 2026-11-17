//! Where a chart file added on its own belongs (a 差分 chart picked or
//! downloaded without its song's folder), as BeMusicSeeker estimates an
//! install destination. A song's folder is a folder holding charts; the
//! files below it count by their path in it (a chart's `sound/kick.wav` is
//! the song's `sound/kick.ogg`, not a `kick.ogg` beside its charts) and by
//! their kind (an image never stands for a sound: [`Class`]). A folder is a
//! candidate when, with the files the chart came with, it holds the sounds
//! the chart uses ([`Rules`]: 80% to be listed, 99% to be chosen); the chart
//! goes there without asking only when its charts are the same song (the
//! same artist and a title alike, as the BeMusicSeeker fork compares them:
//! [`crate::titles`]; two such folders: asked): numbered sound names
//! (`01.wav`, `02.wav`, ...) are common, so the file names alone prove
//! nothing. Else the user picks among the best candidates (songs alike
//! first).

use crate::fs::Fs;
use crate::titles;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

pub use crate::chart::{class_of, ChartInfo, Class};

/// A folder of the library: its files and the charts in it.
#[derive(Clone, Debug, Default)]
pub struct FolderInfo {
    pub dir: PathBuf,
    /// Files below the folder, by their path in it (`sound/kick.ogg`).
    pub files: Vec<String>,
    pub charts: Vec<ChartInfo>,
}

#[derive(Debug, PartialEq)]
pub enum Placement {
    /// Where it is (its sounds are there, or no folder has them all).
    Stay,
    /// Into this folder.
    Move(PathBuf),
    /// One of these folders, or none: the user decides.
    Ask(Vec<PathBuf>),
}

/// A name as a chart writes it (`sound/Kick.wav`), or a file's path in its
/// song's folder, as it is matched: its folders and name without the
/// extension, lower case, `/` between (`sound/kick`). None: no name; Err: a
/// file outside the chart's folder (`../`, a drive), never found.
pub(crate) fn key(name: &str) -> Option<Result<String, ()>> {
    let parts: Vec<&str> = name
        .trim()
        .split(['/', '\\'])
        .filter(|p| !p.is_empty() && *p != ".")
        .collect();
    if parts.iter().any(|p| *p == ".." || p.contains(':')) {
        return Some(Err(()));
    }
    let (last, dirs) = parts.split_last()?;
    let stem = match last.rfind('.') {
        Some(i) if i > 0 => &last[..i],
        _ => last,
    };
    let mut k = String::new();
    for d in dirs {
        k += d;
        k.push('/');
    }
    k += stem;
    Some(Ok(k.to_lowercase()))
}

/// The files of a folder (paths in it) a chart can use, by class.
#[derive(Default)]
struct Keys([HashSet<String>; 3]);

fn slot(c: Class) -> usize {
    match c {
        Class::Audio => 0,
        Class::Image => 1,
        Class::Movie => 2,
    }
}

impl Keys {
    fn of<'a>(files: impl IntoIterator<Item = &'a String>) -> Keys {
        let mut out = Keys::default();
        for f in files {
            out.add(f);
        }
        out
    }

    fn add(&mut self, file: &str) {
        if let (Some(c), Some(Ok(k))) = (class_of(file), key(file)) {
            self.0[slot(c)].insert(k);
        }
    }

    fn has(&self, c: Class, k: &str) -> bool {
        self.0[slot(c)].contains(k)
    }
}

/// The files of one class a chart names: their keys, and how many of them
/// are outside its folder.
#[derive(Default)]
struct Need {
    keys: HashSet<String>,
    outside: usize,
}

impl Need {
    fn of<'a>(names: impl IntoIterator<Item = &'a str>) -> Need {
        let mut out = Need::default();
        for n in names {
            match key(n) {
                Some(Ok(k)) => {
                    out.keys.insert(k);
                }
                Some(Err(())) => out.outside += 1,
                None => {}
            }
        }
        out
    }

    fn total(&self) -> usize {
        self.keys.len() + self.outside
    }

    /// (held by `own`, held by `own` or `brought`), of `class`.
    fn held(&self, class: Class, own: &Keys, brought: &Keys) -> (usize, usize) {
        let mine = self.keys.iter().filter(|k| own.has(class, k)).count();
        let all = self
            .keys
            .iter()
            .filter(|k| own.has(class, k) || brought.has(class, k))
            .count();
        (mine, all)
    }

    /// The share of them held (1 when it names none).
    fn share(&self, class: Class, own: &Keys, brought: &Keys) -> f32 {
        match self.total() {
            0 => 1.0,
            n => self.held(class, own, brought).1 as f32 / n as f32,
        }
    }
}

/// What a chart asks of a folder: its sounds, BGA images and movies, and
/// its stage file, banner and back image.
struct Needs {
    audio: Need,
    images: Need,
    movies: Need,
    extras: Need,
}

impl Needs {
    fn of(c: &ChartInfo) -> Needs {
        let names = |v: &[String]| Need::of(v.iter().map(|s| s.as_str()).collect::<Vec<_>>());
        Needs {
            audio: names(&c.sounds),
            images: names(&c.images),
            movies: names(&c.movies),
            extras: Need::of(c.extra_images()),
        }
    }

    fn outside(&self) -> bool {
        self.audio.outside + self.images.outside + self.movies.outside + self.extras.outside > 0
    }
}

/// `name` without bracketed parts (`[ANOTHER]`, `(obj: ...)`, `【...】`),
/// lower case, spaces collapsed.
fn strip_brackets(name: &str) -> String {
    let mut out = String::new();
    let mut depth = 0usize;
    for c in name.chars() {
        match c {
            '[' | '(' | '【' | '（' | '［' | '<' | '〔' => depth += 1,
            ']' | ')' | '】' | '）' | '］' | '>' | '〕' => depth = depth.saturating_sub(1),
            _ if depth == 0 => out.extend(c.to_lowercase()),
            _ => {}
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Titles match when one starts with the other once tags are gone
/// ("Arcadia -★1-" and "Arcadia"); the shorter needs three characters.
pub fn same_title(a: &str, b: &str) -> bool {
    let (a, b) = (strip_brackets(a), strip_brackets(b));
    let (short, long) = if a.chars().count() <= b.chars().count() {
        (a, b)
    } else {
        (b, a)
    };
    short.chars().count() >= 3 && long.starts_with(&short)
}

/// Titles of one song's charts: [`same_title`], or the same once the
/// chart's name is gone ("CG901B -7EX-[Short Ver.]" and "CG901B -5-").
pub fn same_song_title(a: &str, b: &str) -> bool {
    use crate::bmssearch::base_title;
    let (x, y) = (base_title(a), base_title(b));
    same_title(a, b) || (x.chars().count() >= 2 && same_name(&x, &y))
}

/// The same title, but for its brackets ("Song [ANOTHER]" is "Song").
pub fn same_name(a: &str, b: &str) -> bool {
    let a = strip_brackets(a);
    !a.is_empty() && a == strip_brackets(b)
}

/// Artists match without the chart maker's part ("artist / obj: someone").
pub fn same_artist(a: &str, b: &str) -> bool {
    let clean = |s: &str| {
        let s = s.to_lowercase();
        let cut = ["/", "obj", "差分", "譜面"]
            .iter()
            .filter_map(|m| s.find(m))
            .min()
            .unwrap_or(s.len());
        strip_brackets(&s[..cut])
    };
    let (a, b) = (clean(a), clean(b));
    if a.is_empty() || b.is_empty() {
        return a == b;
    }
    let (short, long) = if a.len() <= b.len() { (a, b) } else { (b, a) };
    long.starts_with(&short)
}

/// The share of `sounds` that `files` (paths in a song's folder) hold: by
/// path and name, any sound extension; 1 when there are none (a sound
/// outside the folder, `../`, is never held).
pub fn coverage(files: &[String], sounds: &[String]) -> f32 {
    let need = Need::of(sounds.iter().map(|s| s.as_str()));
    need.share(Class::Audio, &Keys::of(files), &Keys::default())
}

/// Whether `files` (paths in a song's folder) hold every one of `sounds`
/// that can be held (by path and name, any sound extension).
pub fn covers(files: &[String], sounds: &[String]) -> bool {
    let need = Need::of(sounds.iter().map(|s| s.as_str()));
    let have = Keys::of(files);
    need.keys.iter().all(|k| have.has(Class::Audio, k))
}

/// How a chart's folder is chosen.
#[derive(Clone, Copy, Debug)]
pub struct Rules {
    /// The share of a chart's sounds a folder must hold, with the files the
    /// chart came with, to be its song's folder.
    pub min_coverage: f32,
    /// The share it must hold to be offered (the user picks).
    pub min_listed: f32,
}

impl Default for Rules {
    fn default() -> Rules {
        Rules {
            min_coverage: 0.99,
            min_listed: 0.8,
        }
    }
}

/// A folder a chart may belong in.
#[derive(Clone, Debug, PartialEq)]
pub struct Candidate {
    pub dir: PathBuf,
    /// The share of the chart's sounds there, with the files it came with.
    pub coverage: f32,
    /// How many of them the folder holds itself.
    pub own: usize,
    /// The folder's song is the chart's ([`titles::Relation::Strong`]:
    /// the same artist, the titles alike).
    pub same_song: bool,
    /// Its title or artist is alike ([`titles::Relation::Related`]).
    pub related: bool,
    /// The shares of its BGA images, movies, and stage file / banner /
    /// back image there (1 when it names none).
    pub images: f32,
    pub movies: f32,
    pub extras: f32,
    /// The sound files in the folder.
    pub sound_files: usize,
    /// The chart names a file outside its folder (`../`): it is not placed
    /// without asking.
    pub outside: bool,
}

impl Candidate {
    /// The chart's sounds the folder holds, of its own and the chart's
    /// sounds together (fewer other sounds: closer).
    fn jaccard(&self, sounds: usize) -> f32 {
        // (`own` is of both)
        let all = sounds + self.sound_files - self.own;
        if all == 0 {
            0.0
        } else {
            self.own as f32 / all as f32
        }
    }
}

/// The folders among `folders` for `chart`, which came with the files
/// `bundled` (paths beside it): those holding at least one of its sounds,
/// the best first (the same song; then a song alike; then more of its
/// sounds, more of them in the folder itself; then more of its images, movies, stage file / banner
/// / back image; then fewer other sounds).
pub fn rank(chart: &ChartInfo, bundled: &[String], folders: &[FolderInfo]) -> Vec<Candidate> {
    let need = Needs::of(chart);
    if need.audio.keys.is_empty() {
        return Vec::new();
    }
    let brought = Keys::of(bundled);
    let sounds = need.audio.total();
    // the files it came with hold most of its sounds: they say little, so
    // a folder is related only by both its title and artist
    let rule = match need.audio.share(Class::Audio, &brought, &Keys::default()) >= 0.9 {
        true => titles::Rule::Both,
        false => titles::Rule::Either,
    };
    let target = titles::Profile::one(&chart.title, &chart.artist);
    let mut out: Vec<Candidate> = folders
        .iter()
        .filter_map(|f| {
            let have = Keys::of(&f.files);
            let (own, all) = need.audio.held(Class::Audio, &have, &brought);
            if own == 0 {
                return None;
            }
            let profile = titles::Profile::of(f.charts.iter().map(|c| (&*c.title, &*c.artist, "")));
            let relation = target.relation(&profile, rule);
            Some(Candidate {
                dir: f.dir.clone(),
                coverage: all as f32 / sounds as f32,
                own,
                same_song: relation == titles::Relation::Strong,
                related: relation >= titles::Relation::Related,
                images: need.images.share(Class::Image, &have, &brought),
                movies: need.movies.share(Class::Movie, &have, &brought),
                extras: need.extras.share(Class::Image, &have, &brought),
                sound_files: have.0[slot(Class::Audio)].len(),
                outside: need.outside(),
            })
        })
        .collect();
    let order = |c: &Candidate| {
        (
            c.same_song,
            c.related,
            c.coverage,
            c.own,
            c.images,
            c.movies,
            c.extras,
            c.jaccard(sounds),
        )
    };
    out.sort_by(|a, b| {
        order(b)
            .partial_cmp(&order(a))
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    out
}

/// What [`rank`]'s candidates mean: the one folder that is the same song
/// and holds the chart's sounds (`min_coverage`); else the folders that
/// hold most of them (`min_listed`: the user picks, at most 3); else none
/// yet.
pub fn decide(candidates: &[Candidate], rules: Rules) -> Placement {
    let sure: Vec<&Candidate> = candidates
        .iter()
        .filter(|c| c.coverage >= rules.min_coverage && c.same_song && !c.outside)
        .collect();
    let listed: Vec<&Candidate> = candidates
        .iter()
        .filter(|c| c.coverage >= rules.min_listed.min(rules.min_coverage))
        .collect();
    let dirs = |c: &[&Candidate]| c.iter().take(3).map(|c| c.dir.clone()).collect();
    match (sure.len(), listed.len()) {
        (1, _) => Placement::Move(sure[0].dir.clone()),
        (n, _) if n > 1 => Placement::Ask(dirs(&sure)),
        (_, 0) => Placement::Stay,
        _ => Placement::Ask(dirs(&listed)),
    }
}

/// Where `chart`, now in `own` (with the files it came with), belongs among
/// `folders` (by the default [`Rules`]).
pub fn place(chart: &ChartInfo, own: &FolderInfo, folders: &[FolderInfo]) -> Placement {
    if chart.sounds.is_empty() || covers(&own.files, &chart.sounds) {
        return Placement::Stay;
    }
    let others: Vec<FolderInfo> = folders
        .iter()
        .filter(|f| f.dir != own.dir)
        .cloned()
        .collect();
    decide(&rank(chart, &own.files, &others), Rules::default())
}

/// Where a chart file in the library belongs, with labels for asking.
#[derive(Debug, PartialEq)]
pub enum Located {
    /// Its sounds are beside it.
    Here,
    /// No folder has its sounds (yet).
    NoSong,
    Move(PathBuf),
    /// Candidate folders, each with a label naming its song.
    Ask(Vec<(PathBuf, String)>),
}

const CHART_EXTS: [&str; 5] = ["bms", "bme", "bml", "pms", "bmson"];

pub fn is_chart(p: &std::path::Path) -> bool {
    p.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| CHART_EXTS.contains(&e.to_ascii_lowercase().as_str()))
}

pub(crate) fn file_name(p: &std::path::Path) -> String {
    p.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// `p`'s path in `dir`, `/` between its parts.
pub(crate) fn rel(p: &Path, dir: &Path) -> String {
    p.strip_prefix(dir)
        .unwrap_or(p)
        .components()
        .map(|c| c.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

/// A folder the library's walks pass by (`.job-…`, a downloads folder
/// named `.bmseeker`: what passes through, not songs).
fn hidden(p: &Path) -> bool {
    p.file_name()
        .is_some_and(|n| n.to_string_lossy().starts_with('.'))
}

/// Every folder below `dir` with its file names.
pub fn folders(
    fs: &dyn Fs,
    dir: &std::path::Path,
) -> std::collections::BTreeMap<PathBuf, Vec<String>> {
    fn walk(
        fs: &dyn Fs,
        dir: &std::path::Path,
        depth: usize,
        out: &mut std::collections::BTreeMap<PathBuf, Vec<String>>,
    ) {
        for e in fs.read_dir(dir).unwrap_or_default() {
            if e.is_dir {
                if depth < 8 {
                    walk(fs, &e.path, depth + 1, out);
                }
            } else {
                out.entry(dir.to_path_buf())
                    .or_default()
                    .push(file_name(&e.path));
            }
        }
    }
    let mut out = std::collections::BTreeMap::new();
    walk(fs, dir, 0, &mut out);
    out
}

/// The files below `dir` (a few levels), by their path in it.
pub fn files_below(fs: &dyn Fs, dir: &Path) -> Vec<String> {
    let mut out = Vec::new();
    let mut todo = vec![(dir.to_path_buf(), 0)];
    while let Some((d, depth)) = todo.pop() {
        for e in fs.read_dir(&d).unwrap_or_default() {
            if !e.is_dir {
                out.push(rel(&e.path, dir));
            } else if depth < 8 {
                todo.push((e.path, depth + 1));
            }
        }
    }
    out
}

/// The songs' folders below `dir` (those holding a chart), each with the
/// files below it by their path in it (a folder below holding charts of
/// its own is a song of its own; folders whose names start with `.` are
/// passed by).
pub fn song_folders(fs: &dyn Fs, dir: &Path) -> std::collections::BTreeMap<PathBuf, Vec<String>> {
    song_folders_except(fs, dir, None)
}

/// [`song_folders`], `skip` (and what is below it) passed by too.
pub(crate) fn song_folders_except(
    fs: &dyn Fs,
    dir: &Path,
    skip: Option<&Path>,
) -> std::collections::BTreeMap<PathBuf, Vec<String>> {
    struct Walk<'a> {
        fs: &'a dyn Fs,
        skip: Option<&'a Path>,
        out: std::collections::BTreeMap<PathBuf, Vec<String>>,
    }
    fn walk(w: &mut Walk, dir: &Path, depth: usize, song: Option<&Path>) {
        let entries = w.fs.read_dir(dir).unwrap_or_default();
        let song = if entries.iter().any(|e| !e.is_dir && is_chart(&e.path)) {
            w.out.entry(dir.to_path_buf()).or_default();
            Some(dir)
        } else {
            song
        };
        for e in &entries {
            if e.is_dir {
                if depth < 16 && !hidden(&e.path) && w.skip != Some(e.path.as_path()) {
                    walk(w, &e.path, depth + 1, song);
                }
            } else if let Some(s) = song {
                let r = rel(&e.path, s);
                w.out.entry(s.to_path_buf()).or_default().push(r);
            }
        }
    }
    let mut w = Walk {
        fs,
        skip,
        out: Default::default(),
    };
    walk(&mut w, dir, 0, None);
    for files in w.out.values_mut() {
        files.sort();
    }
    w.out
}

/// The title, artist and files of a chart file.
pub fn chart_info(fs: &dyn Fs, path: &std::path::Path) -> Option<ChartInfo> {
    let bytes = fs.read(path).ok()?;
    crate::chart::info(path, &bytes)
}

/// Where `chart` belongs among the folders below `library` (see [`place`]),
/// the files beside it having come with it.
pub fn locate(fs: &dyn Fs, chart: &Path, library: &Path) -> Option<Located> {
    locate_with(fs, chart, library, true, Rules::default()).map(|(l, _)| l)
}

/// [`locate`] for a chart that came on its own, into a folder of downloads
/// (the files beside it are not its).
pub fn locate_loose(fs: &dyn Fs, chart: &Path, library: &Path) -> Option<Located> {
    locate_with(fs, chart, library, false, Rules::default()).map(|(l, _)| l)
}

/// The library's songs' folders, read once for many charts: a
/// [`Library`](crate::library::Library).
pub type LibraryIndex = crate::library::Library;

/// [`locate`] (`siblings`: the files beside the chart came with it) by
/// `rules`, with the candidates (for a person to look at).
pub fn locate_with(
    fs: &dyn Fs,
    chart: &Path,
    library: &Path,
    siblings: bool,
    rules: Rules,
) -> Option<(Located, Vec<(Candidate, String)>)> {
    locate_in(fs, chart, &LibraryIndex::new(library), siblings, rules)
}

/// [`locate_with`] among the folders of `index` (read once for many
/// charts).
pub fn locate_in(
    fs: &dyn Fs,
    chart: &Path,
    index: &LibraryIndex,
    siblings: bool,
    rules: Rules,
) -> Option<(Located, Vec<(Candidate, String)>)> {
    let info = chart_info(fs, chart)?;
    let here = chart.parent()?.to_path_buf();
    // what came with it: the files below its folder are its song's (its
    // sounds there: it is where it belongs); those beside it go with it
    let below = if siblings {
        files_below(fs, &here)
    } else {
        Vec::new()
    };
    if covers(&below, &info.sounds) {
        return Some((Located::Here, Vec::new()));
    }
    let bundled: Vec<String> = below.into_iter().filter(|f| !f.contains('/')).collect();
    Some(locate_by(fs, &info, &bundled, &here, index, rules))
}

/// Where the chart `info`, now in the folder `here` with the files
/// `bundled` (paths in that folder) that go with it, belongs among the
/// folders of `index` (`here` aside), with the candidates (for a person to
/// look at).
pub fn locate_by(
    fs: &dyn Fs,
    info: &ChartInfo,
    bundled: &[String],
    here: &Path,
    index: &LibraryIndex,
    rules: Rules,
) -> (Located, Vec<(Candidate, String)>) {
    // the folders with any of its sounds; their charts read for the best
    let sounds = Needs::of(info).audio.keys;
    let near = index.near(fs, &sounds, here);
    let rough = rank(info, bundled, &near);
    let keep: Vec<FolderInfo> = rough
        .iter()
        .enumerate()
        .filter(|(i, c)| c.coverage >= rules.min_coverage || *i < 3)
        .filter_map(|(_, c)| near.iter().find(|f| f.dir == c.dir))
        .map(|f| FolderInfo {
            charts: index.folder_charts(fs, f),
            ..f.clone()
        })
        .collect();
    let ranked = rank(info, bundled, &keep);
    let song = |d: &Path| {
        let s = keep
            .iter()
            .find(|f| f.dir == d)
            .and_then(|f| f.charts.first())
            .map(|c| format!("{} — {}", c.title, c.artist))
            .unwrap_or_default();
        format!("{s}  ({})", file_name(d))
    };
    let located = match decide(&ranked, rules) {
        Placement::Move(d) => Located::Move(d),
        Placement::Stay => Located::NoSong,
        Placement::Ask(dirs) => Located::Ask(
            dirs.into_iter()
                .map(|d| {
                    let label = song(&d);
                    (d, label)
                })
                .collect(),
        ),
    };
    let listed = ranked
        .into_iter()
        .map(|c| {
            let label = song(&c.dir);
            (c, label)
        })
        .collect();
    (located, listed)
}

/// The charts below `dir` (a few levels), read.
pub fn charts_below(fs: &dyn Fs, dir: &Path) -> Vec<ChartInfo> {
    let mut out = Vec::new();
    let mut todo = vec![(dir.to_path_buf(), 0)];
    while let Some((d, depth)) = todo.pop() {
        for e in fs.read_dir(&d).unwrap_or_default() {
            if e.is_dir && depth < 8 {
                todo.push((e.path, depth + 1));
            } else if !e.is_dir && is_chart(&e.path) {
                out.extend(chart_info(fs, &e.path));
            }
        }
    }
    out
}

/// The folder below `library` for a song's archive named `stem` whose
/// charts are `charts`: `stem` when it is free or holds the same song (a
/// chart of the same title and artist), else `stem (2)`, `stem (3)`...
/// (another song of that title).
pub fn song_folder(fs: &dyn Fs, library: &Path, stem: &str, charts: &[ChartInfo]) -> PathBuf {
    free_folder(fs, library, stem, charts, |d| charts_below(fs, d))
}

/// [`song_folder`] in `lib` (its charts' titles and artists as it knows
/// them).
pub fn song_folder_in(
    fs: &dyn Fs,
    lib: &LibraryIndex,
    stem: &str,
    charts: &[ChartInfo],
) -> PathBuf {
    free_folder(fs, lib.root(), stem, charts, |d| lib.charts_below(fs, d))
}

fn free_folder(
    fs: &dyn Fs,
    library: &Path,
    stem: &str,
    charts: &[ChartInfo],
    below: impl Fn(&Path) -> Vec<ChartInfo>,
) -> PathBuf {
    let same = |c: &ChartInfo| {
        charts
            .iter()
            .any(|n| same_song_title(&c.title, &n.title) && same_artist(&c.artist, &n.artist))
    };
    (1..)
        .map(|n| match n {
            1 => library.join(stem),
            n => library.join(format!("{stem} ({n})")),
        })
        .find(|d| fs.read_dir(d).is_err() || below(d).iter().any(same))
        .expect("a free name")
}

/// What to do with a file coming into a folder (BeMusicSeeker's smart
/// overwrite), by the (modification time, size) of it and of the file of
/// its name there.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Clash {
    /// None there: it goes in.
    Move,
    /// It replaces the one there.
    Overwrite,
    /// The one there stays; it is not needed.
    Keep,
    /// A chart of the name there: it comes in under another name.
    Rename,
}

/// [`Clash`] for a file (`new`: its time and size; `old`: the one there):
/// a chart never replaces another; the same size within 2 seconds is the
/// same file; else a newer one replaces the one there, an older one does
/// not. Without file times to go by (`times` false: a web page's storage),
/// the one there stays.
pub fn clash(new: (i64, u64), old: Option<(i64, u64)>, is_chart: bool, times: bool) -> Clash {
    let Some(old) = old else {
        return Clash::Move;
    };
    if is_chart {
        return Clash::Rename;
    }
    if !times || (new.1 == old.1 && (new.0 - old.0).abs() <= 2) {
        return Clash::Keep;
    }
    if new.0 > old.0 {
        Clash::Overwrite
    } else {
        Clash::Keep
    }
}

/// Move `file` into the folder `dir` (a chart, or a file it came with), by
/// [`clash`]: a chart of the same name there is not replaced (it comes in
/// as `name_.bms`; the same chart again just goes); another file of the
/// same name stays unless `file` is newer, and `file` goes. Returns where
/// it is now (None: it was not needed, or could not move).
pub fn bring(fs: &dyn Fs, file: &Path, dir: &Path) -> Option<PathBuf> {
    let name = file_name(file);
    let mut to = dir.join(&name);
    let chart = is_chart(file);
    if chart && fs.stamp(&to).is_some() {
        let same = |a: &Path, b: &Path| fs.read(a).ok().is_some_and(|x| fs.read(b).ok() == Some(x));
        if same(file, &to) {
            fs.remove(file);
            return Some(to);
        }
    }
    let new = fs.stamp(file).unwrap_or_default();
    match clash(new, fs.stamp(&to), chart, fs.keeps_times()) {
        Clash::Move => fs.rename(file, &to).then_some(to),
        Clash::Overwrite => (fs.remove(&to) && fs.rename(file, &to)).then_some(to),
        Clash::Keep => {
            fs.remove(file);
            None
        }
        Clash::Rename => {
            let (stem, ext) = match name.rsplit_once('.') {
                Some((s, e)) => (s.to_string(), format!(".{e}")),
                None => (name.clone(), String::new()),
            };
            let mut n = 1;
            while fs.stamp(&to).is_some() {
                to = dir.join(format!("{stem}{}{ext}", "_".repeat(n)));
                n += 1;
            }
            fs.rename(file, &to).then_some(to)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chart(title: &str, artist: &str, sounds: &[&str]) -> ChartInfo {
        ChartInfo {
            title: title.into(),
            artist: artist.into(),
            sounds: sounds.iter().map(|s| s.to_string()).collect(),
            ..Default::default()
        }
    }

    fn folder(dir: &str, files: &[&str], charts: Vec<ChartInfo>) -> FolderInfo {
        FolderInfo {
            dir: dir.into(),
            files: files.iter().map(|s| s.to_string()).collect(),
            charts,
        }
    }

    #[test]
    fn titles_and_artists() {
        assert!(same_title("Arcadia -★1-", "Arcadia"));
        assert!(same_title("Mirage Garden [ANOTHER]", "Mirage Garden"));
        assert!(!same_title("X (虎のANOTHER)", "X [7KEYS]")); // too short to tell
        assert!(!same_title("Love Story", "Lovely Day"));
        // one song's charts, each named after the title
        assert!(same_song_title(
            "CG901B -7EX-[Short Ver.]BGA+",
            "CG901B -7light-[Short Ver.] BGA+"
        ));
        assert!(!same_song_title("X (虎のANOTHER)", "X [7KEYS]"));
        assert!(!same_song_title("Love Story -A-", "Lovely Day -A-"));
        assert!(same_artist(
            "ねこみみ魔法使い / obj:秘密",
            "ねこみみ魔法使い"
        ));
        assert!(same_artist("Squar / 差分 赤い人", "Squar"));
        assert!(!same_artist("xi", "Grand Thaw"));
    }

    #[test]
    fn numbered_sounds_need_the_same_song() {
        let diff = chart("Song B [INSANE]", "beta / obj: x", &["01.wav", "02.wav"]);
        let picked = folder("picked", &["b_insane.bme"], vec![diff.clone()]);
        let a = folder(
            "A",
            &["01.ogg", "02.ogg", "a.bms"],
            vec![chart("Song A", "alpha", &[])],
        );
        let b = folder(
            "B",
            &["01.ogg", "02.ogg", "b.bms"],
            vec![chart("Song B", "beta", &[])],
        );
        // both folders have 01 / 02; only B is the same song
        assert_eq!(
            place(&diff, &picked, &[a.clone(), b.clone()]),
            Placement::Move("B".into())
        );
        // the same song nowhere: ask among the folders that have the sounds
        let other = chart("Song C", "gamma", &["01.wav", "02.wav"]);
        assert_eq!(
            place(&other, &picked, &[a.clone(), b]),
            Placement::Ask(vec!["A".into(), "B".into()])
        );
        // no folder has every sound: it stays
        let more = chart("Song B", "beta", &["01.wav", "03.wav"]);
        assert_eq!(place(&more, &picked, &[a]), Placement::Stay);
    }

    /// The fork's matching: the same artist and a title alike is the same
    /// song; two such folders are not picked between; a song alike comes
    /// before one that is not.
    #[test]
    fn same_song_by_the_forks_rules() {
        let diff = chart("Song (HYPER) [X]", "alpha / obj: x", &["01.wav", "02.wav"]);
        let picked = folder("picked", &["x.bme"], vec![diff.clone()]);
        let song = |dir: &str, title: &str, artist: &str| {
            folder(
                dir,
                &["01.ogg", "02.ogg", "n.bms"],
                vec![chart(title, artist, &[])],
            )
        };
        let a = song("A", "Song [NORMAL]", "alpha");
        assert_eq!(
            place(&diff, &picked, std::slice::from_ref(&a)),
            Placement::Move("A".into())
        );
        // another artist: asked, not moved
        let b = song("B", "Song [NORMAL]", "beta");
        assert_eq!(
            place(&diff, &picked, std::slice::from_ref(&b)),
            Placement::Ask(vec!["B".into()])
        );
        // two folders of the same song (a tie): asked
        let a2 = song("A2", "Song [HYPER]", "alpha feat. y");
        assert_eq!(
            place(&diff, &picked, &[a.clone(), a2]),
            Placement::Ask(vec!["A".into(), "A2".into()])
        );
        // a song alike first among those asked
        let r = rank(&diff, &[], &[song("Z", "Zzz", "q"), b]);
        assert_eq!((r[0].dir.as_path(), r[0].related), (Path::new("B"), true));
        assert!(!r[1].related && !r[1].same_song);
    }

    #[test]
    fn a_chart_with_its_sounds_stays() {
        let c = chart("Song", "artist", &["kick.wav"]);
        let own = folder("picked", &["kick.wav", "song.bme"], vec![c.clone()]);
        let other = folder("S", &["kick.wav"], vec![chart("Song", "artist", &[])]);
        assert_eq!(place(&c, &own, &[other]), Placement::Stay);
    }

    #[test]
    fn sounds_it_came_with_count() {
        // the 差分 brings its extra sound; the song folder has the rest
        let diff = chart(
            "Song B [EX]",
            "beta / obj: x",
            &["01.wav", "02.wav", "extra.wav"],
        );
        let own = folder("pkg", &["b_ex.bme", "extra.ogg"], vec![diff.clone()]);
        let b = folder(
            "B",
            &["01.ogg", "02.ogg", "b.bms"],
            vec![chart("Song B", "beta", &[])],
        );
        assert_eq!(
            place(&diff, &own, std::slice::from_ref(&b)),
            Placement::Move("B".into())
        );
        // without it, 2 of 3: not enough
        let bare = folder("pkg", &["b_ex.bme"], vec![diff.clone()]);
        assert_eq!(
            place(&diff, &bare, std::slice::from_ref(&b)),
            Placement::Stay
        );
        // a lower bar takes it
        let r = rank(&diff, &[], &[b]);
        assert_eq!(r[0].own, 2);
        assert!((r[0].coverage - 2.0 / 3.0).abs() < 1e-6);
        assert_eq!(
            decide(
                &r,
                Rules {
                    min_coverage: 0.6,
                    ..Default::default()
                }
            ),
            Placement::Move("B".into())
        );
    }

    /// An Fs that counts the folders read.
    struct Counting(std::sync::atomic::AtomicUsize);

    impl Fs for Counting {
        fn read(&self, p: &Path) -> std::io::Result<Vec<u8>> {
            crate::fs::Disk.read(p)
        }
        fn read_dir(&self, p: &Path) -> std::io::Result<Vec<crate::fs::Entry>> {
            self.0.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            crate::fs::Disk.read_dir(p)
        }
        fn rename(&self, from: &Path, to: &Path) -> bool {
            crate::fs::Disk.rename(from, to)
        }
        fn stamp(&self, p: &Path) -> Option<(i64, u64)> {
            crate::fs::Disk.stamp(p)
        }
        fn remove(&self, p: &Path) -> bool {
            crate::fs::Disk.remove(p)
        }
    }

    fn temp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("bmseeker-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    fn put(p: &Path, body: &str) {
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, body).unwrap();
    }

    /// Charts waiting for their songs are placed with the library read
    /// once, not once per chart.
    #[test]
    fn the_library_read_once() {
        let root = temp("index");
        let (library, downloads) = (root.join("bms"), root.join("downloads"));
        for i in 0..10 {
            let song = library.join(format!("Song{i}"));
            put(&song.join("a.bms"), &format!("#TITLE Song{i}\n#ARTIST x\n"));
            put(&song.join(format!("s{i}.ogg")), "x");
        }
        for i in 0..5 {
            put(
                &downloads.join(format!("c{i}.bms")),
                &format!("#TITLE Song{i} [X]\n#ARTIST x\n#WAV01 s{i}.wav\n"),
            );
        }
        let fs = Counting(Default::default());
        let placed = crate::install::place_waiting(&fs, &downloads, &library);
        assert_eq!(placed.len(), 5, "{placed:?}");
        assert!(library.join("Song4/c4.bms").is_file());
        // the downloads, the library and its 10 folders: once each
        let reads = fs.0.load(std::sync::atomic::Ordering::Relaxed);
        assert!(reads <= 12, "{reads} folders read");
        let _ = std::fs::remove_dir_all(&root);
    }

    /// A file moved into the library after the index was read counts.
    #[test]
    fn the_index_follows_what_comes_in() {
        let root = temp("index-add");
        let library = root.join("bms");
        let song = library.join("Song");
        put(
            &song.join("a.bms"),
            "#TITLE Song\n#ARTIST x\n#WAV01 01.wav\n",
        );
        put(&song.join("01.ogg"), "x");
        let chart = root.join("dl/c.bms");
        put(
            &chart,
            "#TITLE Song [X]\n#ARTIST x\n#WAV01 01.wav\n#WAV02 02.wav\n",
        );
        let mut index = LibraryIndex::new(&library);
        let at = |index: &LibraryIndex| {
            locate_in(&crate::fs::Disk, &chart, index, false, Rules::default()).map(|(l, _)| l)
        };
        assert_eq!(at(&index), Some(Located::NoSong));
        put(&song.join("02.ogg"), "x");
        index.add(&song.join("02.ogg"), None);
        assert_eq!(at(&index), Some(Located::Move(song.clone())));
        let _ = std::fs::remove_dir_all(&root);
    }

    /// The audit's defect 1: a song's sounds in a folder of its own
    /// (`Song/sound/`): a 差分 using `sound/kick.wav` goes into `Song`, not
    /// `Song/sound`.
    #[test]
    fn sounds_in_a_folder_of_the_song() {
        use crate::fs::Disk;
        let root = temp("subdir");
        let (lib, dl) = (root.join("lib"), root.join("dl"));
        let song = lib.join("Song");
        put(
            &song.join("song.bms"),
            "#TITLE Song\n#ARTIST a\n#WAV01 sound/kick.wav\n#WAV02 sound/snare.wav\n",
        );
        put(&song.join("sound/kick.ogg"), "x");
        put(&song.join("sound/snare.ogg"), "x");
        let diff = dl.join("song_ex.bms");
        put(
            &diff,
            "#TITLE Song [EX]\n#ARTIST a\n#WAV01 sound/kick.wav\n#WAV02 sound\\snare.wav\n",
        );
        let at = |c: &Path| locate_with(&Disk, c, &lib, false, Rules::default()).map(|r| r.0);
        assert_eq!(at(&diff), Some(Located::Move(song.clone())));
        // the same names beside its charts are not those in `sound/`
        let flat = dl.join("flat.bms");
        put(
            &flat,
            "#TITLE Song [EX]\n#ARTIST a\n#WAV01 kick.wav\n#WAV02 snare.wav\n",
        );
        assert_eq!(at(&flat), Some(Located::NoSong));
        // a song's archive with its sounds in `sound/`: they are beside it
        assert_eq!(
            locate(&Disk, &song.join("song.bms"), &lib),
            Some(Located::Here)
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    /// The audit's defect 2: images are no sounds.
    #[test]
    fn images_are_no_sounds() {
        use crate::fs::Disk;
        let root = temp("images");
        let (lib, dl) = (root.join("lib"), root.join("dl"));
        let other = lib.join("Other");
        put(&other.join("01.png"), "x");
        put(&other.join("02.bmp"), "x");
        put(&other.join("o.bms"), "#TITLE Song\n#ARTIST a\n");
        let diff = dl.join("d2.bms");
        put(
            &diff,
            "#TITLE Song [EX]\n#ARTIST a\n#WAV01 01.wav\n#WAV02 02.wav\n",
        );
        let r = locate_with(&Disk, &diff, &lib, false, Rules::default()).map(|r| r.0);
        assert_eq!(r, Some(Located::NoSong));
        // a folder of files that are no chart's is no song
        let loose = lib.join("Loose");
        put(&loose.join("01.ogg"), "x");
        put(&loose.join("02.ogg"), "x");
        let r = locate_with(&Disk, &diff, &lib, false, Rules::default()).map(|r| r.0);
        assert_eq!(r, Some(Located::NoSong));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn listed_from_80_percent_placed_from_99() {
        let sounds: Vec<String> = (1..=10).map(|i| format!("{i:02}.wav")).collect();
        let sounds: Vec<&str> = sounds.iter().map(|s| s.as_str()).collect();
        let diff = chart("Song [X]", "a", &sounds);
        let files = |n: usize| -> Vec<String> {
            let mut v: Vec<String> = (1..=n).map(|i| format!("{i:02}.ogg")).collect();
            v.push("song.bms".into());
            v
        };
        let mk = |dir: &str, n: usize| FolderInfo {
            dir: dir.into(),
            files: files(n),
            charts: vec![chart("Song", "a", &[])],
        };
        // 9 of 10: offered, not chosen
        let r = rank(&diff, &[], &[mk("Nine", 9), mk("Seven", 7)]);
        assert_eq!(
            decide(&r, Rules::default()),
            Placement::Ask(vec!["Nine".into()])
        );
        // all of them: chosen
        let r = rank(&diff, &[], &[mk("All", 10), mk("Nine", 9)]);
        assert_eq!(decide(&r, Rules::default()), Placement::Move("All".into()));
        // a file outside the chart's folder: asked
        let mut out = diff.clone();
        out.sounds = (1..=200).map(|i| format!("{i:03}.wav")).collect();
        out.sounds.push("../shared.wav".into());
        let big = FolderInfo {
            dir: "Big".into(),
            files: (1..=200).map(|i| format!("{i:03}.ogg")).collect(),
            charts: vec![chart("Song", "a", &[])],
        };
        let r = rank(&out, &[], &[big]);
        assert!(r[0].coverage > 0.99 && r[0].outside);
        assert_eq!(
            decide(&r, Rules::default()),
            Placement::Ask(vec!["Big".into()])
        );
    }

    #[test]
    fn images_and_movies_rank_after_sounds() {
        let mut diff = chart("Song [X]", "a", &["a.wav"]);
        diff.images = vec!["bga/01.bmp".into()];
        diff.movies = vec!["mv.mpg".into()];
        diff.stagefile = "stage.jpg".into();
        let mk = |dir: &str, extra: &[&str]| {
            let mut files = vec!["a.ogg".to_string(), "s.bms".to_string()];
            files.extend(extra.iter().map(|s| s.to_string()));
            FolderInfo {
                dir: dir.into(),
                files,
                charts: vec![chart("Song", "a", &[])],
            }
        };
        let r = rank(
            &diff,
            &[],
            &[
                mk("Bare", &[]),
                mk("Stage", &["stage.png"]),
                mk("Movie", &["mv.mp4", "bga/01.png"]),
                mk("Flat", &["01.png", "mv.png"]),
            ],
        );
        let order: Vec<&str> = r.iter().map(|c| c.dir.to_str().unwrap()).collect();
        assert_eq!(order, ["Movie", "Stage", "Bare", "Flat"]);
        assert_eq!((r[0].images, r[0].movies, r[0].extras), (1.0, 1.0, 0.0));
        // fewer other sounds first, all else equal
        let more = FolderInfo {
            files: vec!["a.ogg".into(), "b.ogg".into(), "s.bms".into()],
            ..mk("More", &[])
        };
        let r = rank(&chart("S", "a", &["a.wav"]), &[], &[more, mk("Bare", &[])]);
        assert_eq!(r[0].dir, PathBuf::from("Bare"));
    }

    #[test]
    fn keys_by_path_and_class() {
        assert_eq!(key("Sound\\Kick.WAV"), Some(Ok("sound/kick".into())));
        assert_eq!(key("./a/./b.ogg"), Some(Ok("a/b".into())));
        assert_eq!(key("../x.wav"), Some(Err(())));
        assert_eq!(key("C:\\x.wav"), Some(Err(())));
        assert_eq!(key(" "), None);
        let files: Vec<String> = ["sound/kick.ogg", "kick.png"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        assert!(covers(&files, &["sound/kick.wav".into()]));
        assert!(!covers(&files, &["kick.wav".into()]));
        assert_eq!(
            coverage(&files, &["kick.wav".into(), "sound/kick.flac".into()]),
            0.5
        );
    }

    /// The song's folder holds files in its folders; a chart moved into
    /// one that was not a song's makes it one.
    #[test]
    fn songs_folders_and_their_files() {
        use crate::fs::Disk;
        let root = temp("songs");
        let lib = root.join("lib");
        put(&lib.join("Pack/Song/a.bms"), "#TITLE A\n");
        put(&lib.join("Pack/Song/sound/x.ogg"), "x");
        put(&lib.join("Pack/Song/Other/b.bms"), "#TITLE B\n");
        put(&lib.join("Pack/Song/Other/y.ogg"), "x");
        put(&lib.join("Pack/readme.txt"), "x");
        put(&lib.join(".bmseeker/w.bms"), "#TITLE W\n");
        let s = song_folders(&Disk, &lib);
        assert_eq!(s[&lib.join("Pack/Song")], ["a.bms", "sound/x.ogg"]);
        assert_eq!(s[&lib.join("Pack/Song/Other")], ["b.bms", "y.ogg"]);
        assert_eq!(s.len(), 2, "{s:?}");
        let mut index = LibraryIndex::new(&lib);
        // (read before the files come)
        assert!(index.files(&Disk, &lib.join("Pack/Song")).is_some());
        index.add(&lib.join("Pack/Song/sound/z.ogg"), None);
        index.add(&lib.join("New/c.bms"), None);
        index.add(&lib.join("New/c.ogg"), None);
        let files = |d: &str| index.files(&Disk, &lib.join(d)).map(|f| f.to_vec());
        assert!(files("Pack/Song")
            .unwrap()
            .contains(&"sound/z.ogg".to_string()));
        assert_eq!(files("New").unwrap(), ["c.bms", "c.ogg"]);
        let sounds: HashSet<String> = ["sound/z".to_string()].into();
        let near = index.near(&Disk, &sounds, Path::new(""));
        assert_eq!(near[0].dir, lib.join("Pack/Song"));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn the_overwrite_rule() {
        let t = 1_000_000;
        assert_eq!(clash((t, 5), None, false, true), Clash::Move);
        assert_eq!(clash((t, 5), None, true, true), Clash::Move);
        assert_eq!(clash((t, 5), Some((t, 6)), true, true), Clash::Rename);
        // the same size within 2 seconds: the same file
        assert_eq!(clash((t + 2, 5), Some((t, 5)), false, true), Clash::Keep);
        assert_eq!(clash((t - 2, 5), Some((t, 5)), false, true), Clash::Keep);
        // newer: replaces; older or as old: stays
        assert_eq!(
            clash((t + 3, 5), Some((t, 5)), false, true),
            Clash::Overwrite
        );
        assert_eq!(
            clash((t + 1, 6), Some((t, 5)), false, true),
            Clash::Overwrite
        );
        assert_eq!(clash((t, 6), Some((t, 5)), false, true), Clash::Keep);
        assert_eq!(clash((t - 100, 6), Some((t, 5)), false, true), Clash::Keep);
        // no times to go by: what is there stays
        assert_eq!(clash((t + 100, 6), Some((t, 5)), false, false), Clash::Keep);
        assert_eq!(clash((t, 5), None, false, false), Clash::Move);
    }

    /// The audit's defect 4: a 差分 package's sounds, unpacked with their
    /// (older) archive times, do not replace the song's own.
    #[test]
    fn a_package_unpacked_with_its_times() {
        use crate::fs::Mem;
        use crate::unpack::{unpack, Blob, BUDGET};
        let mem = Mem::default();
        let now = 1_700_000_000;
        mem.put(Path::new("bms/Song/01.ogg"), b"song's", now);
        let mut out = std::io::Cursor::new(Vec::new());
        {
            let mut z = zip::ZipWriter::new(&mut out);
            let at = zip::DateTime::from_date_and_time(2015, 6, 1, 0, 0, 0).unwrap();
            let o = zip::write::SimpleFileOptions::default().last_modified_time(at);
            for (n, b) in [("01.ogg", b"pack's".as_slice()), ("02.ogg", b"new")] {
                z.start_file(n, o).unwrap();
                std::io::Write::write_all(&mut z, b).unwrap();
            }
            z.finish().unwrap();
        }
        let pkg = Path::new("dl/pkg");
        unpack(Blob::Bytes(out.into_inner()), &mem, pkg, BUDGET).unwrap();
        let song = Path::new("bms/Song");
        assert_eq!(bring(&mem, &pkg.join("01.ogg"), song), None);
        assert_eq!(
            bring(&mem, &pkg.join("02.ogg"), song),
            Some(song.join("02.ogg"))
        );
        assert_eq!(mem.get(&song.join("01.ogg")).unwrap().0, b"song's");
        assert!(mem.get(&pkg.join("01.ogg")).is_none());
    }

    /// Where files keep no times (a web page's storage), what is there
    /// stays.
    #[test]
    fn no_times_no_overwrite() {
        struct NoTimes(crate::fs::Mem);
        impl Fs for NoTimes {
            fn read(&self, p: &Path) -> std::io::Result<Vec<u8>> {
                self.0.read(p)
            }
            fn read_dir(&self, p: &Path) -> std::io::Result<Vec<crate::fs::Entry>> {
                self.0.read_dir(p)
            }
            fn rename(&self, from: &Path, to: &Path) -> bool {
                self.0.rename(from, to)
            }
            fn stamp(&self, p: &Path) -> Option<(i64, u64)> {
                self.0.stamp(p)
            }
            fn remove(&self, p: &Path) -> bool {
                self.0.remove(p)
            }
            fn keeps_times(&self) -> bool {
                false
            }
        }
        let fs = NoTimes(Default::default());
        fs.0.put(Path::new("s/a.ogg"), b"old", 0);
        fs.0.put(Path::new("p/a.ogg"), b"newer", 100);
        assert_eq!(bring(&fs, Path::new("p/a.ogg"), Path::new("s")), None);
        assert_eq!(fs.0.get(Path::new("s/a.ogg")).unwrap().0, b"old");
    }

    #[test]
    fn smart_overwrite() {
        use crate::fs::Disk;
        let d = std::env::temp_dir().join(format!("bmseeker-bring-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        let (pkg, song) = (d.join("pkg"), d.join("song"));
        std::fs::create_dir_all(&pkg).unwrap();
        std::fs::create_dir_all(&song).unwrap();
        let old = std::time::SystemTime::now() - std::time::Duration::from_secs(3600);
        let put = |p: &Path, body: &str, older: bool| {
            std::fs::write(p, body).unwrap();
            if older {
                std::fs::File::options()
                    .write(true)
                    .open(p)
                    .unwrap()
                    .set_modified(old)
                    .unwrap();
            }
        };
        // newer: overwrites; older: the song's stays
        put(&song.join("a.ogg"), "old", true);
        put(&pkg.join("a.ogg"), "new", false);
        put(&song.join("b.ogg"), "keep", false);
        put(&pkg.join("b.ogg"), "stale", true);
        assert_eq!(
            bring(&Disk, &pkg.join("a.ogg"), &song),
            Some(song.join("a.ogg"))
        );
        assert_eq!(std::fs::read_to_string(song.join("a.ogg")).unwrap(), "new");
        assert_eq!(bring(&Disk, &pkg.join("b.ogg"), &song), None);
        assert_eq!(std::fs::read_to_string(song.join("b.ogg")).unwrap(), "keep");
        assert!(!pkg.join("b.ogg").exists());
        // a chart: never over another, the same one just goes
        put(&song.join("x.bms"), "#TITLE A", false);
        put(&pkg.join("x.bms"), "#TITLE B", false);
        assert_eq!(
            bring(&Disk, &pkg.join("x.bms"), &song),
            Some(song.join("x_.bms"))
        );
        put(&pkg.join("x.bms"), "#TITLE A", false);
        assert_eq!(
            bring(&Disk, &pkg.join("x.bms"), &song),
            Some(song.join("x.bms"))
        );
        assert!(!pkg.join("x.bms").exists());
        let _ = std::fs::remove_dir_all(&d);
    }
}
