//! What a download comes to in the library, on any [`Store`]: unpacked into
//! a folder of its own (staged), its packages ([`crate::package`]) planned
//! without touching a file ([`plan`]: each new to the library, the
//! library's already, or for a song's folder there, or waiting for its
//! song), then the plan carried out ([`apply`]). [`take`] does it all, for
//! a download that has just arrived: natively and in a web page alike.

use super::{md5_hex, safe_name, unique, Done, Failure};
use crate::chart::{md5_key, ChartInfo};
use crate::fs::{file_name, move_file, Fs, Store};
use crate::hosts::Want;
use crate::library::Library;
use crate::package::{classify, split, Package, PackageKind, Staged};
use crate::placement::{self, covers, is_chart, Located, Rules};
use crate::unpack::{unpack, Blob, Member, BUDGET};
use std::collections::{HashMap, HashSet};
use std::io;
use std::path::{Path, PathBuf};

/// What the link a download came from was for: it counts most in what a
/// package is.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Role {
    /// A song's own link (a table's `url`).
    Song,
    /// A separate chart's (差分: a table's `url_diff`).
    Diff,
    /// Not known (a file brought in by hand).
    #[default]
    Any,
}

/// How a song new to the library names its folder.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum Naming {
    /// After the download (its file name but for the extension).
    #[default]
    Download,
    /// This name (the song's title, made a file name).
    Named(String),
    /// After its charts, as the BeMusicSeeker fork names one
    /// (`naming::folder_name`: `format` with `%ARTIST%` and
    /// `%TITLE%`; `sjis_only`: only characters Shift_JIS has); a folder the
    /// download wraps everything in left out; a folder of the library
    /// holding the song (by its charts' titles and artists, whatever its
    /// name) used.
    Charts { format: String, sjis_only: bool },
}

impl Naming {
    /// [`Naming::Charts`] when `config` names folders after their charts
    /// ([`Config::folder_naming`](crate::config::Config::folder_naming)),
    /// else `otherwise`.
    pub fn of(config: &crate::config::Config, otherwise: Naming) -> Naming {
        match config.folder_naming {
            true => Naming::Charts {
                format: config.folder_name_format.clone(),
                sjis_only: config.sjis_only,
            },
            false => otherwise,
        }
    }
}

/// What a download is for, and how it is brought in.
#[derive(Clone, Debug, Default)]
pub struct Request {
    /// The song looked for.
    pub want: Option<Want>,
    /// The chart looked for (its MD5).
    pub md5: Option<String>,
    /// The MD5s of the song's own charts (a table's `org_md5`): where a
    /// separate chart goes when its package says nothing.
    pub org_md5: Vec<String>,
    pub role: Role,
    /// Found by name: only if it holds the chart `md5` (a song's link: or
    /// a chart of the song `want`).
    pub check: bool,
    pub rules: Rules,
    pub naming: Naming,
    /// The download's file name (else the file's own): a chart's (`.bms`,
    /// ...) is a chart on its own, not an archive.
    pub file: Option<String>,
    /// A package whose charts the library has: its files come in again,
    /// replacing those of their names when newer (else only those the
    /// song's folder lacks).
    pub reinstall: bool,
    /// Other charts wanted (MD5s: a batch's, a table's missing charts):
    /// from a separate chart's archive they wait for their songs too.
    pub also: Vec<String>,
}

/// What a package comes to.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Outcome {
    /// The library's already, in this folder (files it lacked brought in).
    Owned(PathBuf),
    /// A song new to the library, in this folder.
    New(PathBuf),
    /// Into this song's folder in the library.
    Into(PathBuf),
    /// Its charts wait in the downloads for their song.
    Waiting,
    /// One of these folders (each with a label naming its song): the user
    /// picks; its charts wait in the downloads meanwhile.
    Ask(Vec<(PathBuf, String)>),
    /// Nothing comes in.
    Rejected(Failure),
}

/// A package's outcome and its files' moves (from the staged folder; to a
/// path, or a name in the downloads folder when it waits).
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Item {
    pub(crate) outcome: Outcome,
    pub(crate) moves: Vec<(PathBuf, PathBuf)>,
}

impl Item {
    fn rejected(f: Failure) -> Item {
        Item {
            outcome: Outcome::Rejected(f),
            moves: Vec::new(),
        }
    }
}

/// What folders a package's charts point to.
#[derive(Debug, PartialEq)]
enum Vote {
    None,
    One(PathBuf),
    /// Equally many: not for a machine to pick.
    Tie(Vec<PathBuf>),
}

/// Each voter's folders: the folder most of them name.
fn tally(voters: Vec<HashSet<PathBuf>>) -> Vote {
    let mut count: HashMap<PathBuf, usize> = HashMap::new();
    for v in voters {
        for d in v {
            *count.entry(d).or_default() += 1;
        }
    }
    let Some(top) = count.values().max().copied() else {
        return Vote::None;
    };
    let mut best: Vec<PathBuf> = count
        .into_iter()
        .filter(|(_, n)| *n == top)
        .map(|(d, _)| d)
        .collect();
    best.sort();
    match best.len() {
        1 => Vote::One(best.remove(0)),
        _ => Vote::Tie(best),
    }
}

/// `folder` without `sub` at its end (compared case-insensitively): the
/// package's folder in the library when a chart in its `sub` is there.
fn up(folder: &Path, sub: &Path) -> Option<PathBuf> {
    let mut f = folder.to_path_buf();
    for seg in sub.components().rev() {
        let name = f.file_name()?.to_string_lossy().to_lowercase();
        if name != seg.as_os_str().to_string_lossy().to_lowercase() {
            return None;
        }
        f = f.parent()?.to_path_buf();
    }
    Some(f)
}

/// What a chart's folder is in its package's.
fn sub(s: &Staged, p: &Package, c: usize) -> PathBuf {
    s.chart_dir(c)
        .strip_prefix(&p.root)
        .unwrap_or(Path::new(""))
        .to_path_buf()
}

/// The library's charts in the package vote: each (MD5) for the folders it
/// is in there (the package's folder, by where the chart is in it).
fn owned_vote(s: &Staged, p: &Package, lib: &Library) -> Vote {
    let mut by_md5: HashMap<&str, HashSet<PathBuf>> = HashMap::new();
    for &c in p.charts.iter().filter(|&&c| s.owned(c, lib)) {
        let chart = &s.charts[c];
        let roots = by_md5.entry(chart.md5.as_str()).or_default();
        let at = lib.folders_of(&chart.md5).iter();
        for f in at.chain(lib.folders_of(&chart.sha256)) {
            roots.extend(up(f, &sub(s, p, c)));
        }
    }
    tally(by_md5.into_values().filter(|v| !v.is_empty()).collect())
}

/// The song's own charts a table names (`org_md5`) vote for theirs.
fn hint_vote(lib: &Library, org: &[String]) -> Vote {
    let mut seen = HashSet::new();
    let voters = org
        .iter()
        .filter_map(|m| md5_key(m))
        .filter(|m| seen.insert(m.clone()))
        .map(|m| lib.folders_of(&m).iter().cloned().collect::<HashSet<_>>())
        .filter(|v| !v.is_empty())
        .collect();
    tally(voters)
}

/// Whether the package's charts `charts` have their sounds in the library
/// folder `dest` (with the files they come with), by `rules`.
fn fits(
    fs: &dyn Fs,
    s: &Staged,
    p: &Package,
    lib: &Library,
    charts: &[usize],
    dest: &Path,
    rules: Rules,
) -> bool {
    let song: Vec<String> = lib.files(fs, dest).map(|f| f.to_vec()).unwrap_or_default();
    // the library's files where the chart would be (a folder in the song's)
    let files_at = |sub: &Path| -> Vec<String> {
        if sub.as_os_str().is_empty() {
            return song.clone();
        }
        if let Some(f) = lib.files(fs, &dest.join(sub)) {
            return f.to_vec();
        }
        let at = format!("{}/", placement::rel(sub, Path::new("")));
        song.iter()
            .filter_map(|f| f.strip_prefix(&at).map(String::from))
            .collect()
    };
    charts.iter().all(|&c| {
        let Some(info) = &s.charts[c].info else {
            return false;
        };
        let mut files = files_at(&sub(s, p, c));
        files.extend(p.files_for(s, c));
        covers(&files, &info.sounds)
            || placement::coverage(&files, &info.sounds) >= rules.min_coverage
    })
}

/// A folder's label: its song's title and artist, and its name.
fn label(fs: &dyn Fs, lib: &Library, d: &Path) -> String {
    let song = lib
        .charts_below(fs, d)
        .first()
        .map(|c| format!("{} — {}", c.title, c.artist))
        .unwrap_or_default();
    format!("{song}  ({})", file_name(d))
}

/// The package's files into `dest` as they are laid out in it (the
/// library's own charts left out; with `fill`, only the files `dest`
/// lacks).
fn laid_out(
    fs: &dyn Fs,
    s: &Staged,
    p: &Package,
    lib: &Library,
    dest: &Path,
    fill: bool,
) -> Vec<(PathBuf, PathBuf)> {
    p.files
        .iter()
        .filter_map(|&f| {
            let m = &s.members[f];
            let chart = s.charts.iter().position(|c| c.member == f);
            if chart.is_some_and(|c| s.owned(c, lib)) {
                return None;
            }
            let to = dest.join(m.rel.strip_prefix(&p.root).ok()?);
            if chart.is_none() && fill && fs.stamp(&to).is_some() {
                return None;
            }
            Some((s.root.join(&m.rel), to))
        })
        .collect()
}

/// The charts `charts` waiting in the downloads (by their names).
fn waiting(s: &Staged, charts: &[usize], outcome: Outcome) -> Item {
    Item {
        outcome,
        moves: charts
            .iter()
            .map(|&c| {
                let rel = s.chart_rel(c);
                (s.root.join(rel), PathBuf::from(file_name(rel)))
            })
            .collect(),
    }
}

/// Where the package `p` (charts for songs' folders in the library) goes:
/// the folder its own charts the library has vote for (a tie: asked); else
/// the folder the song's own charts a table names are in; else where each
/// chart's sounds are ([`placement`]): all in one folder, the package goes
/// in as it is laid out; else each chart on its own, with the files beside
/// it.
fn diff_items(fs: &dyn Fs, s: &Staged, p: &Package, lib: &Library, req: &Request) -> Vec<Item> {
    let new: Vec<usize> = p
        .charts
        .iter()
        .copied()
        .filter(|&c| !s.owned(c, lib))
        .collect();
    // a song's whole folder: only the files the library's lacks (unless
    // brought in again)
    let fill = p.complete(s, req.rules) && !req.reinstall;
    let into = |d: PathBuf| Item {
        moves: laid_out(fs, s, p, lib, &d, fill),
        outcome: Outcome::Into(d),
    };
    let ask = |dirs: Vec<PathBuf>| {
        let labels = dirs
            .into_iter()
            .map(|d| {
                let l = label(fs, lib, &d);
                (d, l)
            })
            .collect();
        waiting(s, &new, Outcome::Ask(labels))
    };
    match owned_vote(s, p, lib) {
        Vote::One(d) if fits(fs, s, p, lib, &new, &d, req.rules) => return vec![into(d)],
        Vote::One(d) => return vec![ask(vec![d])],
        Vote::Tie(dirs) => return vec![ask(dirs)],
        Vote::None => {}
    }
    match hint_vote(lib, &req.org_md5) {
        Vote::One(d) => {
            let d = new
                .first()
                .and_then(|&c| up(&d, &sub(s, p, c)))
                .unwrap_or(d);
            if fits(fs, s, p, lib, &new, &d, req.rules) {
                return vec![into(d)];
            }
        }
        Vote::Tie(dirs) => return vec![ask(dirs)],
        Vote::None => {}
    }
    // each chart by its sounds (a song's whole folder: the library's must
    // hold them itself)
    let located: Vec<(usize, Located, bool)> = new
        .iter()
        .map(|&c| {
            let Some(info) = &s.charts[c].info else {
                return (c, Located::NoSong, false);
            };
            let bundled = p.files_for(s, c);
            let alone = covers(&bundled, &info.sounds);
            let here = s.root.join(s.chart_dir(c));
            let with: &[String] = if alone { &[] } else { &bundled };
            let (l, _) = placement::locate_by(fs, info, with, &here, lib, req.rules);
            (c, l, alone)
        })
        .collect();
    let roots: Vec<Option<PathBuf>> = located
        .iter()
        .map(|(c, l, _)| match l {
            Located::Move(x) => up(x, &sub(s, p, *c)),
            _ => None,
        })
        .collect();
    if let Some(Some(first)) = roots.first() {
        if roots.iter().all(|r| r.as_ref() == Some(first)) {
            return vec![into(first.clone())];
        }
    }
    // each chart on its own: the files right beside it with it
    let mut taken: HashSet<usize> = HashSet::new();
    located
        .into_iter()
        .map(|(c, l, alone)| match l {
            Located::Move(to) => {
                let rel = s.chart_rel(c);
                let mut moves = vec![(s.root.join(rel), to.join(file_name(rel)))];
                if !alone {
                    let dir = s.chart_dir(c);
                    for &f in &p.files {
                        let m = &s.members[f].rel;
                        if m.parent() == Some(dir) && !is_chart(m) && taken.insert(f) {
                            moves.push((s.root.join(m), to.join(file_name(m))));
                        }
                    }
                }
                Item {
                    outcome: Outcome::Into(to),
                    moves,
                }
            }
            Located::Ask(c2) => waiting(s, &[c], Outcome::Ask(c2)),
            _ => waiting(s, &[c], Outcome::Waiting),
        })
        .collect()
}

/// The folder all of `files` (members) are in, when they are in one (a
/// folder a download wraps everything in).
fn wrapper(s: &Staged, files: &[usize]) -> Option<PathBuf> {
    let mut top: Option<&std::ffi::OsStr> = None;
    for &f in files {
        let mut parts = s.members[f].rel.components();
        let first = parts.next()?.as_os_str();
        // a file right in it, or another folder: none
        parts.next()?;
        if top.is_some_and(|t| t != first) {
            return None;
        }
        top = Some(first);
    }
    top.map(PathBuf::from)
}

/// The packages `new` (and the files in no package) as one song new to the
/// library, laid out as in the download.
fn new_item(
    fs: &dyn Fs,
    s: &Staged,
    lib: &Library,
    new: &[&Package],
    all: &[Package],
    req: &Request,
) -> Item {
    let charts: Vec<ChartInfo> = new
        .iter()
        .flat_map(|p| p.charts.iter())
        .filter_map(|&c| s.charts[c].info.clone())
        .collect();
    let packed: HashSet<usize> = all.iter().flat_map(|p| p.files.iter().copied()).collect();
    let mut files: Vec<usize> = new.iter().flat_map(|p| p.files.iter().copied()).collect();
    files.extend((0..s.members.len()).filter(|f| !packed.contains(f)));
    files.sort_unstable();
    files.dedup();
    let named = |stem: &str| placement::song_folder(fs, lib, stem, &charts);
    let (dest, strip) = match &req.naming {
        Naming::Named(n) => (named(n), None),
        Naming::Download => {
            let stem = req
                .file
                .as_deref()
                .and_then(|f| {
                    // (`song.tar.gz`: `song`)
                    let name = Path::new(f).file_name()?.to_string_lossy();
                    let stem = crate::unpack::archive_stem(&name).map(str::to_owned);
                    stem.or_else(|| Some(Path::new(f).file_stem()?.to_string_lossy().into()))
                })
                .map(|s| safe_name(&s))
                .unwrap_or_else(|| "download".into());
            (named(&stem), None)
        }
        Naming::Charts { format, sjis_only } => {
            // the song's folder in the library already, by its charts'
            // titles and artists (not by a name that may have changed)
            let dest = match lib.song_folders_like(&charts).as_slice() {
                [one] => one.clone(),
                _ => {
                    let name = crate::naming::folder_name(&charts, format, *sjis_only);
                    named(&crate::naming::fit_path(lib.root(), &name))
                }
            };
            (dest, wrapper(s, &files))
        }
    };
    Item {
        moves: files
            .into_iter()
            .map(|f| {
                let rel = &s.members[f].rel;
                let to = match &strip {
                    Some(w) => rel.strip_prefix(w).unwrap_or(rel),
                    None => rel,
                };
                (s.root.join(rel), dest.join(to))
            })
            .collect(),
        outcome: Outcome::New(dest),
    }
}

/// Whether the chart `c` is of the song `want` ([`same_song`](crate::titles::same_song)).
fn of_song(c: &Option<ChartInfo>, want: &Want) -> bool {
    use crate::titles::{same_song, Song};
    c.as_ref().is_some_and(|c| {
        same_song(
            &Song::of(&want.title, &want.artist),
            &Song::of(&c.title, &c.artist),
        )
    })
}

/// What becomes of the staged download `s` in the library `lib` (read
/// through `fs`; nothing written), as `req` says. Found by name (`check`):
/// nothing unless it holds the chart (a song's archive: or a chart of the
/// song). Each package: the library's already: its files the song's folder
/// lacks come in ([`Outcome::Owned`]); new: a folder of its own
/// ([`Outcome::New`], one for all of them); charts for a song's folder
/// there: into it ([`Outcome::Into`]), or waiting for their song, or
/// asked. What a song's link (or a file brought by hand) brought goes in
/// as a song of its own unless where it goes is sure. What waits from a
/// separate chart's archive is only the chart `md5` and those of `also` (a
/// pack of many songs' charts keeps no others): nothing at all,
/// [`Failure::NotTheChart`].
pub(crate) fn plan(fs: &dyn Fs, s: &Staged, lib: &Library, req: &Request) -> Vec<Item> {
    let md5 = req.md5.as_deref().and_then(md5_key);
    if let Some(m) = md5.as_deref().filter(|_| req.check) {
        let holds = s.charts.iter().any(|c| c.md5 == m)
            || (req.role == Role::Song
                && !s.loose
                && req
                    .want
                    .as_ref()
                    .is_some_and(|w| s.charts.iter().any(|c| of_song(&c.info, w))));
        if !holds {
            return vec![Item::rejected(Failure::NotTheChart)];
        }
    }
    let diff = req.role == Role::Diff || s.loose;
    let packages = split(s);
    if packages.is_empty() {
        // no chart at all: a song's folder of whatever it is, or nothing
        return vec![if diff {
            Item::rejected(Failure::NotTheChart)
        } else {
            new_item(fs, s, lib, &[], &packages, req)
        }];
    }
    let mut items = Vec::new();
    let mut new: Vec<&Package> = Vec::new();
    for p in &packages {
        match classify(p, s, lib, req.role, req.rules) {
            PackageKind::Owned => {
                let dest = match owned_vote(s, p, lib) {
                    Vote::One(d) => Some(d),
                    _ => None,
                };
                // more than one folder holds it: none is picked (one named)
                let at = dest.clone().or_else(|| {
                    p.charts.iter().find_map(|&c| {
                        let c = &s.charts[c];
                        let at = lib.folders_of(&c.md5).first();
                        at.or(lib.folders_of(&c.sha256).first()).cloned()
                    })
                });
                // (no folder holds it after all: a song new to it)
                let Some(at) = at else {
                    new.push(p);
                    continue;
                };
                items.push(Item {
                    moves: dest
                        .map(|d| laid_out(fs, s, p, lib, &d, !req.reinstall))
                        .unwrap_or_default(),
                    outcome: Outcome::Owned(at),
                });
            }
            PackageKind::Song => new.push(p),
            PackageKind::Diff => {
                let got = diff_items(fs, s, p, lib, req);
                let sure = got.iter().all(|i| matches!(i.outcome, Outcome::Into(_)));
                // a song's link: a folder of its own unless sure
                if sure || diff {
                    items.extend(got);
                } else {
                    new.push(p);
                }
            }
        }
    }
    if !new.is_empty() {
        items.push(new_item(fs, s, lib, &new, &packages, req));
    }
    // a separate chart's archive: only the charts wanted wait
    if let (Some(m), true) = (md5.as_deref(), req.role == Role::Diff && !s.loose) {
        let also: HashSet<String> = req.also.iter().filter_map(|m| md5_key(m)).collect();
        let wanted = |c: &str| c == m || also.contains(c);
        items.retain_mut(|i| {
            if !matches!(i.outcome, Outcome::Waiting | Outcome::Ask(_)) {
                return true;
            }
            i.moves.retain(|(from, _)| {
                s.charts
                    .iter()
                    .any(|c| wanted(&c.md5) && s.root.join(&s.members[c.member].rel) == *from)
            });
            !i.moves.is_empty()
        });
    }
    if items.is_empty() {
        items.push(Item::rejected(Failure::NotTheChart));
    }
    items
}

/// Move files in (`moves`, each to a path below `root`), replacing those
/// there: each into a folder of `root` first (what can fail: a copy to
/// another disk), then into its place, those there set aside in that folder
/// until all are in. A failure (and where) leaves the files there as they
/// were.
fn move_in(
    store: &dyn Store,
    root: &Path,
    moves: &[(PathBuf, PathBuf)],
) -> Result<(), (PathBuf, io::Error)> {
    // (short names: a file's name near the disk's limit moves as well)
    let aside = root.join(format!(".in-{}", unique()));
    let new = |i: usize| aside.join(i.to_string());
    let old = |i: usize| aside.join(format!("{i}.old"));
    for (i, (from, to)) in moves.iter().enumerate() {
        if let Err(e) = move_file(store, from, &new(i)) {
            store.remove_dir(&aside);
            return Err((to.clone(), e));
        }
    }
    // (each in, and whether one was there)
    let mut done: Vec<(&Path, usize, bool)> = Vec::new();
    for (i, (_, to)) in moves.iter().enumerate() {
        let had = store.stamp(to).is_some();
        if let Some(d) = to.parent() {
            store.create_dir_all(d);
        }
        let set = match had {
            true => store.rename(to, &old(i)),
            false => Ok(()),
        };
        let r = set.and_then(|()| match store.rename(&new(i), to) {
            Err(e) if had => store.rename(&old(i), to).and(Err(e)),
            r => r,
        });
        if let Err(e) = r {
            for &(to, i, had) in done.iter().rev() {
                let _ = store.remove(to);
                if had {
                    let _ = store.rename(&old(i), to);
                }
            }
            // (a file that could not be put back stays aside)
            if (0..moves.len()).all(|i| store.stamp(&old(i)).is_none()) {
                store.remove_dir(&aside);
            }
            return Err((to.clone(), e));
        }
        done.push((to, i, had));
    }
    store.remove_dir(&aside);
    Ok(())
}

/// What a file that could not be written to `p` comes to.
fn failed(p: &Path, e: io::Error) -> Done {
    Done::Failed(format!("{}: could not be written: {e}", p.display()).into())
}

/// Carry out `items` (from [`plan`]) in `store`: what comes into the
/// library added to `lib`; charts that wait into `downloads`.
#[cfg(test)]
pub(crate) fn apply(
    store: &dyn Store,
    items: Vec<Item>,
    downloads: &Path,
    lib: &mut Library,
) -> Vec<Done> {
    applied(store, items, downloads, lib)
        .into_iter()
        .flat_map(|(_, d)| d)
        .collect()
}

/// [`apply`], each item's outcome with what came of it.
fn applied(
    store: &dyn Store,
    items: Vec<Item>,
    downloads: &Path,
    lib: &mut Library,
) -> Vec<(Outcome, Vec<Done>)> {
    let fs: &dyn Fs = store;
    // into a folder by the overwrite rule (a chart never over another)
    // (None: not needed; a failure as a Done)
    let bring = |lib: &mut Library, from: &Path, to: &Path| -> Result<Option<PathBuf>, Done> {
        let Some(dir) = to.parent() else {
            return Ok(None);
        };
        store.create_dir_all(dir);
        let at = placement::bring(store, from, dir).map_err(|e| failed(to, e))?;
        if let Some(at) = &at {
            let md5 = is_chart(at)
                .then(|| fs.read(at).ok().map(|b| md5_hex(&b)))
                .flatten();
            lib.add(at, md5.as_deref());
        }
        Ok(at)
    };
    let mut out = Vec::new();
    for item in items {
        let done = match &item.outcome {
            Outcome::Rejected(f) => vec![Done::Failed(f.clone())],
            Outcome::New(dir) => {
                // a folder of its own: the files straight in; else the same
                // song again, its files replaced
                let fresh = fs.read_dir(dir).is_err();
                let moved = match fresh {
                    true => (item.moves.iter()).try_for_each(|(from, to)| {
                        move_file(store, from, to).map_err(|e| (to.clone(), e))
                    }),
                    false => move_in(store, dir, &item.moves),
                };
                match moved {
                    Ok(()) => {
                        lib.add_folder(fs, dir);
                        vec![Done::Added(dir.clone(), item.moves.len())]
                    }
                    Err((p, e)) => {
                        if fresh {
                            store.remove_dir(dir);
                        }
                        vec![failed(&p, e)]
                    }
                }
            }
            Outcome::Into(_) => item
                .moves
                .iter()
                .filter_map(|(from, to)| match bring(lib, from, to) {
                    Ok(at) => at.filter(|at| is_chart(at)).map(Done::Placed),
                    Err(d) => Some(d),
                })
                .collect(),
            Outcome::Owned(dir) => {
                let mut done = vec![Done::Owned(dir.clone())];
                for (from, to) in &item.moves {
                    done.extend(bring(lib, from, to).err());
                }
                done
            }
            Outcome::Waiting | Outcome::Ask(_) => item
                .moves
                .iter()
                .filter_map(|(from, name)| {
                    let to = downloads.join(name);
                    // (an older copy of it replaced)
                    let moved = to == *from
                        || move_in(store, downloads, &[(from.clone(), to.clone())]).is_ok();
                    moved.then_some(Done::Waiting(to))
                })
                .collect(),
        };
        out.push((item.outcome, done));
    }
    out
}

/// The download `blob` into the library `lib`, in `store`, as `req` says:
/// unpacked into a folder of its own in `downloads` (a chart on its own
/// put there), planned, carried out; a song added,
/// the charts that waited in `downloads` for it go into its folder; the
/// staged folder goes. A file `blob` is gone once unpacked.
pub fn take(
    store: &dyn Store,
    blob: Blob,
    req: &Request,
    downloads: &Path,
    lib: &mut Library,
) -> Vec<Done> {
    let (out, placed) = taken(store, blob, req, downloads, lib);
    (out.into_iter().flat_map(|(_, d)| d))
        .chain(placed.into_iter().map(Done::Placed))
        .collect()
}

/// [`take`]: each package's outcome with what came of it, and the charts
/// that waited in `downloads` placed.
pub(crate) fn taken(
    store: &dyn Store,
    blob: Blob,
    req: &Request,
    downloads: &Path,
    lib: &mut Library,
) -> (Vec<(Outcome, Vec<Done>)>, Vec<PathBuf>) {
    let fs: &dyn Fs = store;
    let file = match &blob {
        Blob::File(p) => Some(p.clone()),
        Blob::Bytes(_) => None,
    };
    let mut req = req.clone();
    if req.file.is_none() {
        req.file = file.as_deref().map(file_name);
    }
    let stage = downloads.join(format!(".take-{}", unique()));
    let loose = req.file.as_deref().filter(|n| is_chart(Path::new(n)));
    let members = match loose {
        Some(name) => put_loose(store, blob, &stage.join(safe_name(name))),
        None => unpack(blob, store, &stage, BUDGET),
    };
    let members = match members {
        Ok(m) => m,
        Err(e) => {
            store.remove_dir(&stage);
            let f = Failure::Other(e);
            return (
                vec![(Outcome::Rejected(f.clone()), vec![Done::Failed(f)])],
                Vec::new(),
            );
        }
    };
    // (it is in the folder now)
    if let Some(p) = &file {
        let _ = store.remove(p);
    }
    let s = Staged::read(fs, &stage, members, loose.is_some());
    let items = plan(fs, &s, lib, &req);
    let out = applied(store, items, downloads, lib);
    // (a song added, or files a song's folder lacked brought in)
    let came = out.iter().any(|(o, d)| match o {
        Outcome::New(_) => matches!(d.as_slice(), [Done::Added(..)]),
        Outcome::Owned(_) | Outcome::Into(_) => true,
        _ => false,
    });
    let placed = match came {
        true => super::place_waiting(store, downloads, lib),
        false => Vec::new(),
    };
    store.remove_dir(&stage);
    (out, placed)
}

/// A chart on its own (`blob`) as the file `to`.
fn put_loose(store: &dyn Store, blob: Blob, to: &Path) -> Result<Vec<Member>, String> {
    (blob.put(store, to)).map_err(|e| format!("{}: could not be written: {e}", to.display()))?;
    Ok(vec![Member {
        rel: PathBuf::from(file_name(to)),
        size: store.stamp(to).map_or(0, |s| s.1),
        mtime: None,
    }])
}

#[cfg(test)]
mod tests;
