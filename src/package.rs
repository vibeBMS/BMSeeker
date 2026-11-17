//! A download unpacked into a folder of its own (staged) as the packages it
//! holds, and what each is to the library. A package is a song's folder
//! (its charts with their files) or a chart on its own; one archive may
//! hold several ([`split`], by the BeMusicSeeker fork's rules). Each is
//! the library's already ([`PackageKind::Owned`]), a song new to it
//! ([`PackageKind::Song`]), or charts for a song's folder there ([`PackageKind::Diff`]):
//! [`classify`], the link it came from counting most. Where the rules are
//! not sure, the whole download is one package.

use crate::chart::ChartInfo;
use crate::fs::Fs;
use crate::install::plan::Role;
use crate::library::Library;
use crate::placement::{class_of, covers, is_chart, key, rel};
use crate::placement::{Class, Rules};
use crate::titles::Song;
use crate::unpack::Member;
use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::path::{Path, PathBuf};

/// A chart of a staged download.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct StagedChart {
    /// Its file (in [`Staged::members`]).
    pub(crate) member: usize,
    /// Its MD5 and SHA-256, lower case hex.
    pub(crate) md5: String,
    pub(crate) sha256: String,
    /// What it says (None: it could not be read).
    pub(crate) info: Option<ChartInfo>,
}

/// A download unpacked into the folder `root`: its files (paths in it) and
/// its charts, read.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Staged {
    pub(crate) root: PathBuf,
    pub(crate) members: Vec<Member>,
    pub(crate) charts: Vec<StagedChart>,
    /// A file on its own (a chart's link), not an archive.
    pub(crate) loose: bool,
}

impl Staged {
    /// The download unpacked into `root` as `members`, its charts read.
    pub(crate) fn read(fs: &dyn Fs, root: &Path, members: Vec<Member>, loose: bool) -> Staged {
        let charts = members
            .iter()
            .enumerate()
            .filter(|(_, m)| is_chart(&m.rel))
            .map(|(i, m)| {
                let bytes = fs.read(&root.join(&m.rel)).unwrap_or_default();
                StagedChart {
                    member: i,
                    md5: crate::install::md5_hex(&bytes),
                    sha256: crate::library::sha256_hex(&bytes),
                    info: crate::chart::info(&m.rel, &bytes),
                }
            })
            .collect();
        Staged {
            root: root.to_path_buf(),
            members,
            charts,
            loose,
        }
    }

    /// The chart `c`'s path in the download.
    pub(crate) fn chart_rel(&self, c: usize) -> &Path {
        &self.members[self.charts[c].member].rel
    }

    /// The folder of the chart `c` in the download.
    pub(crate) fn chart_dir(&self, c: usize) -> &Path {
        self.chart_rel(c).parent().unwrap_or(Path::new(""))
    }

    /// Whether the library has the chart `c`.
    pub(crate) fn owned(&self, c: usize, lib: &Library) -> bool {
        let c = &self.charts[c];
        lib.owns(&c.md5) || lib.owns(&c.sha256)
    }
}

/// Part of a download that goes somewhere as one.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Package {
    /// Its folder in the download ("": the whole of it).
    pub(crate) root: PathBuf,
    /// Its charts (in [`Staged::charts`]).
    pub(crate) charts: Vec<usize>,
    /// Its files, charts too (in [`Staged::members`]).
    pub(crate) files: Vec<usize>,
}

impl Package {
    /// The whole download as one.
    pub(crate) fn whole(s: &Staged) -> Package {
        Package {
            root: PathBuf::new(),
            charts: (0..s.charts.len()).collect(),
            files: (0..s.members.len()).collect(),
        }
    }

    /// Whether it holds a sound file.
    pub(crate) fn has_audio(&self, s: &Staged) -> bool {
        self.files
            .iter()
            .any(|&f| class_of(&rel(&s.members[f].rel, Path::new(""))) == Some(Class::Audio))
    }

    /// Its files below the folder of its chart `c`, by their path there
    /// (what goes with the chart).
    pub(crate) fn files_for(&self, s: &Staged, c: usize) -> Vec<String> {
        let dir = s.chart_dir(c);
        self.files
            .iter()
            .map(|&f| &s.members[f].rel)
            .filter(|p| p.starts_with(dir))
            .map(|p| rel(p, dir))
            .collect()
    }

    /// Whether it holds the sounds of each of its charts (`rules`' share)
    /// and a sound file at all: a song's own folder.
    pub(crate) fn complete(&self, s: &Staged, rules: Rules) -> bool {
        self.has_audio(s)
            && self.charts.iter().all(|&c| {
                let Some(info) = &s.charts[c].info else {
                    return false;
                };
                let files = self.files_for(s, c);
                covers(&files, &info.sounds)
                    || crate::placement::coverage(&files, &info.sounds) >= rules.min_coverage
            })
    }
}

/// The download's folders: the files right in each, and the folders in it.
struct Tree {
    dirs: BTreeMap<PathBuf, (Vec<usize>, BTreeSet<PathBuf>)>,
}

impl Tree {
    fn of(s: &Staged) -> Tree {
        let mut dirs: BTreeMap<PathBuf, (Vec<usize>, BTreeSet<PathBuf>)> = BTreeMap::new();
        dirs.entry(PathBuf::new()).or_default();
        for (i, m) in s.members.iter().enumerate() {
            let dir = m.rel.parent().unwrap_or(Path::new("")).to_path_buf();
            dirs.entry(dir.clone()).or_default().0.push(i);
            // each folder above it, in the one above that
            let mut d = dir;
            while let Some(up) = d.parent().map(Path::to_path_buf) {
                if d.as_os_str().is_empty() {
                    break;
                }
                let e = dirs.entry(up.clone()).or_default();
                e.1.insert(d.clone());
                d = up;
            }
        }
        Tree { dirs }
    }
}

/// The files below `dir` (paths in it) by kind, as a chart names them.
fn keys_below(s: &Staged, dir: &Path) -> HashSet<(Class, String)> {
    s.members
        .iter()
        .filter(|m| m.rel.starts_with(dir))
        .filter_map(|m| {
            let r = rel(&m.rel, dir);
            Some((class_of(&r)?, key(&r)?.ok()?))
        })
        .collect()
}

/// Whether one of the files `info` uses (sounds, BGA, stage file, banner,
/// back image, preview) is among `have`.
fn uses_any(info: &ChartInfo, have: &HashSet<(Class, String)>) -> bool {
    let has =
        |kind: Class, n: &str| matches!(key(n), Some(Ok(k)) if have.contains(&(kind, k.clone())));
    info.sounds.iter().any(|n| has(Class::Audio, n))
        || info.images.iter().any(|n| has(Class::Image, n))
        || info.movies.iter().any(|n| has(Class::Movie, n))
        || info
            .extra_images()
            .into_iter()
            .any(|n| has(Class::Image, n))
        || has(Class::Audio, &info.preview)
}

/// A sound file right in `files` named `preview…` or `_preview…` (a song's
/// preview, which no chart need name).
fn preview_sound(s: &Staged, files: &[usize]) -> bool {
    files.iter().any(|&f| {
        let name = crate::fs::file_name(&s.members[f].rel).to_lowercase();
        (name.starts_with("preview") || name.starts_with("_preview"))
            && class_of(&name) == Some(Class::Audio)
    })
}

/// The charts are one work's: one chart, or every chart of one title and
/// artist as compared ([`Song`]: exactly, as the fork has it, so that two
/// songs alike never become one).
fn one_song(s: &Staged, charts: &[usize]) -> bool {
    if charts.len() == 1 {
        return true;
    }
    let mut songs = charts.iter().map(|&c| {
        let i = s.charts[c].info.as_ref()?;
        Some(Song::of(&i.title, &i.artist)).filter(|s| !s.title.is_empty())
    });
    let Some(Some(first)) = songs.next() else {
        return false;
    };
    songs.all(|s| s.as_ref() == Some(&first))
}

/// The download's packages, by the fork's rules for a folder holding
/// charts (the first that applies): (1) a file its charts use is there (or
/// a `preview` sound): the whole folder, folders below with their charts
/// too; (2) folders below hold charts: each chart here on its own; (3) one
/// chart, or (4) one song's charts: the whole folder; (5) else each chart
/// on its own. Folders without charts are looked into. Not sure (a chart
/// that cannot be read, a sound, image or movie left out of every package):
/// the whole download, as one.
pub(crate) fn split(s: &Staged) -> Vec<Package> {
    if s.charts.is_empty() {
        return Vec::new();
    }
    let tree = Tree::of(s);
    let mut out = Vec::new();
    walk(s, &tree, Path::new(""), &mut out);
    let mut packed: HashSet<usize> = HashSet::new();
    for p in &out {
        packed.extend(p.files.iter().copied());
    }
    let unread = s.charts.iter().any(|c| c.info.is_none());
    let left_out = s
        .members
        .iter()
        .enumerate()
        .any(|(i, m)| !packed.contains(&i) && class_of(&rel(&m.rel, Path::new(""))).is_some());
    if unread || left_out || out.is_empty() {
        return vec![Package::whole(s)];
    }
    out
}

/// The packages at and below the folder `dir`; whether it holds charts.
fn walk(s: &Staged, tree: &Tree, dir: &Path, out: &mut Vec<Package>) -> bool {
    let Some((files, subdirs)) = tree.dirs.get(dir) else {
        return false;
    };
    let charts: Vec<usize> = (0..s.charts.len())
        .filter(|&c| s.chart_dir(c) == dir)
        .collect();
    let below = |d: &Path| Package {
        root: d.to_path_buf(),
        charts: (0..s.charts.len())
            .filter(|&c| s.chart_rel(c).starts_with(d))
            .collect(),
        files: (0..s.members.len())
            .filter(|&f| s.members[f].rel.starts_with(d))
            .collect(),
    };
    if !charts.is_empty() {
        // (1) what its charts use is here: all of it is one song's
        let have = keys_below(s, dir);
        let used = preview_sound(s, files)
            || charts.iter().any(|&c| {
                s.charts[c]
                    .info
                    .as_ref()
                    .is_some_and(|i| uses_any(i, &have))
            });
        if used {
            out.push(below(dir));
            return true;
        }
    }
    let mut child_charts = false;
    for d in subdirs {
        child_charts |= walk(s, tree, d, out);
    }
    if charts.is_empty() {
        return child_charts;
    }
    // (3), (4): one song's charts, alone in it
    if !child_charts && one_song(s, &charts) {
        out.push(below(dir));
        return true;
    }
    // (2), (5): each chart on its own
    for c in charts {
        out.push(Package {
            root: dir.to_path_buf(),
            charts: vec![c],
            files: vec![s.charts[c].member],
        });
    }
    true
}

/// What a package is to the library.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PackageKind {
    /// Each of its charts is in the library.
    Owned,
    /// A song new to the library: a folder of its own.
    Song,
    /// Charts for a song's folder in the library (a 差分, or a song the
    /// library has some charts of).
    Diff,
}

/// What the package `p` of `s` is, the link it came from (`role`) counting
/// most: all its charts in `lib`: [`PackageKind::Owned`]; some: [`PackageKind::Diff`]
/// (what is new goes into the song's folder). A chart on its own, or what a
/// separate chart's link brought: a [`PackageKind::Diff`]. A song's link: a
/// [`PackageKind::Song`] when it holds any sound. Anything else: a song when it
/// holds its charts' sounds (`rules`), else a 差分.
pub(crate) fn classify(
    p: &Package,
    s: &Staged,
    lib: &Library,
    role: Role,
    rules: Rules,
) -> PackageKind {
    let owned = p.charts.iter().filter(|&&c| s.owned(c, lib)).count();
    if !p.charts.is_empty() && owned == p.charts.len() {
        return PackageKind::Owned;
    }
    if owned > 0 || s.loose {
        return PackageKind::Diff;
    }
    match role {
        Role::Diff => PackageKind::Diff,
        Role::Song if p.has_audio(s) => PackageKind::Song,
        Role::Any if p.complete(s, rules) => PackageKind::Song,
        _ => PackageKind::Diff,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A download of these files (path, text), read from memory.
    pub(crate) fn staged(files: &[(&str, &str)]) -> Staged {
        let mem = crate::fs::Mem::default();
        let root = Path::new("dl/.take");
        let members = files
            .iter()
            .map(|(p, body)| {
                mem.put(&root.join(p), body.as_bytes(), 0);
                Member {
                    rel: PathBuf::from(p),
                    size: body.len() as u64,
                    mtime: None,
                }
            })
            .collect();
        Staged::read(&mem, root, members, false)
    }

    fn roots(s: &Staged) -> Vec<(String, usize, usize)> {
        let mut v: Vec<(String, usize, usize)> = split(s)
            .into_iter()
            .map(|p| (rel(&p.root, Path::new("")), p.charts.len(), p.files.len()))
            .collect();
        v.sort();
        v
    }

    const SONG: &str = "#TITLE Song [A]\n#ARTIST x\n#WAV01 01.wav\n";

    #[test]
    fn one_chart_one_package() {
        let s = staged(&[
            ("Song/a.bms", "#TITLE Song\n#WAV01 01.wav\n"),
            ("Song/readme.txt", "hi"),
        ]);
        assert_eq!(roots(&s), [("Song".into(), 1, 2)]);
    }

    /// Charts whose files are there: the folder, with those below.
    #[test]
    fn shared_files_one_package() {
        let s = staged(&[
            ("Song/a.bms", SONG),
            ("Song/b.bms", "#TITLE Other\n#ARTIST y\n#WAV01 01.wav\n"),
            ("Song/01.ogg", "x"),
            ("Song/extra/c.bms", "#TITLE Third\n#WAV01 01.wav\n"),
        ]);
        assert_eq!(roots(&s), [("Song".into(), 3, 4)]);
        // a preview sound is enough
        let s = staged(&[
            ("Song/a.bms", "#TITLE A\n#WAV01 a.wav\n"),
            ("Song/b.bms", "#TITLE B\n#WAV01 b.wav\n"),
            ("Song/_preview.ogg", "x"),
        ]);
        assert_eq!(roots(&s), [("Song".into(), 2, 3)]);
    }

    /// A pack of songs: each folder its own package.
    #[test]
    fn folders_with_charts() {
        let s = staged(&[
            ("Pack/A/a.bms", "#TITLE A\n#WAV01 01.wav\n"),
            ("Pack/A/01.ogg", "x"),
            ("Pack/B/b.bms", "#TITLE B\n#WAV01 01.wav\n"),
            ("Pack/B/sound/01.ogg", "x"),
            ("Pack/readme.txt", "x"),
        ]);
        assert_eq!(
            roots(&s),
            [("Pack/A".into(), 1, 2), ("Pack/B".into(), 1, 2)]
        );
        // a chart beside folders of charts, nothing of its own there: alone
        let s = staged(&[
            ("Pack/top.bms", "#TITLE Top\n#WAV01 t.wav\n"),
            ("Pack/A/a.bms", "#TITLE A\n#WAV01 01.wav\n"),
            ("Pack/A/01.ogg", "x"),
        ]);
        assert_eq!(roots(&s), [("Pack".into(), 1, 1), ("Pack/A".into(), 1, 2)]);
    }

    /// Charts of other songs, without their files: each on its own; one
    /// song's charts: together.
    #[test]
    fn per_chart() {
        let s = staged(&[
            (
                "diffs/a.bms",
                "#TITLE Alpha [X]\n#ARTIST a\n#WAV01 01.wav\n",
            ),
            ("diffs/b.bms", "#TITLE Beta [Y]\n#ARTIST b\n#WAV01 01.wav\n"),
            ("diffs/readme.txt", "x"),
        ]);
        assert_eq!(roots(&s), [("diffs".into(), 1, 1), ("diffs".into(), 1, 1)]);
        let s = staged(&[
            (
                "diffs/a.bms",
                "#TITLE Alpha [X]\n#ARTIST a\n#WAV01 01.wav\n",
            ),
            (
                "diffs/b.bms",
                "#TITLE Alpha [Y]\n#ARTIST a / obj: q\n#WAV01 01.wav\n",
            ),
        ]);
        assert_eq!(roots(&s), [("diffs".into(), 2, 2)]);
    }

    /// Not sure: the whole download.
    #[test]
    fn unsure_is_one_package() {
        // sounds no chart names where they are (left out)
        let s = staged(&[
            ("diffs/a.bms", "#TITLE Alpha [X]\n#WAV01 01.wav\n"),
            ("diffs/b.bms", "#TITLE Beta [Y]\n#WAV01 02.wav\n"),
            ("sounds/01.ogg", "x"),
        ]);
        assert_eq!(roots(&s), [("".into(), 2, 3)]);
        // a chart that cannot be read
        let mut s = staged(&[("a/a.bms", SONG), ("b/b.bms", SONG), ("b/01.ogg", "x")]);
        s.charts[0].info = None;
        assert_eq!(roots(&s), [("".into(), 2, 3)]);
        // no chart: no package
        assert!(split(&staged(&[("x.ogg", "x")])).is_empty());
    }

    #[test]
    fn classified() {
        use crate::library::Owned;
        let s = staged(&[("Song/a.bms", SONG), ("Song/01.ogg", "x")]);
        let p = &split(&s)[0];
        let rules = Rules::default();
        let lib = Library::new(Path::new("bms"));
        assert_eq!(classify(p, &s, &lib, Role::Song, rules), PackageKind::Song);
        assert_eq!(classify(p, &s, &lib, Role::Any, rules), PackageKind::Song);
        // a separate chart's link: a 差分, whatever it holds
        assert_eq!(classify(p, &s, &lib, Role::Diff, rules), PackageKind::Diff);
        // charts only: a 差分 unless a song's link brought it
        let d = staged(&[("x.bms", SONG)]);
        let q = &split(&d)[0];
        assert_eq!(classify(q, &d, &lib, Role::Any, rules), PackageKind::Diff);
        let d2 = staged(&[("x.bms", SONG), ("y.ogg", "x")]);
        let q2 = &split(&d2)[0];
        assert_eq!(
            classify(q2, &d2, &lib, Role::Song, rules),
            PackageKind::Song
        );
        assert_eq!(classify(q2, &d2, &lib, Role::Any, rules), PackageKind::Diff);
        // in the library: owned
        let owned = |md5: &str| Owned {
            path: "bms/Song/a.bms".into(),
            md5: md5.into(),
            ..Default::default()
        };
        let lib = Library::from_catalog(Path::new("bms"), [owned(&s.charts[0].md5)]);
        assert_eq!(classify(p, &s, &lib, Role::Song, rules), PackageKind::Owned);
        // some of its charts: what is new goes into the song's folder
        let more = staged(&[
            ("Song/a.bms", SONG),
            ("Song/b.bms", "#TITLE Song [B]\n#WAV01 01.wav\n"),
            ("Song/01.ogg", "x"),
        ]);
        let m = &split(&more)[0];
        assert_eq!(
            classify(m, &more, &lib, Role::Song, rules),
            PackageKind::Diff
        );
    }
}
