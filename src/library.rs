//! A BMS library as the install tools see it: its songs' folders (those
//! holding charts) with the files below each by their path in it, the
//! charts' titles and artists, and which folders hold a chart of an MD5 or
//! SHA-256. Read from the files (all at once, or folder by folder
//! when first needed: [`Library::new`]) or given by a player that keeps its
//! own records of its charts ([`Library::from_catalog`]: a web page cannot
//! read its library's charts when it likes).

use crate::chart::ChartInfo;
use crate::fs::{file_name, Fs, Store};
use crate::placement::{is_chart, rel, song_folders_except, Class, Folder, Keys, Names};
use crate::titles;
use std::cell::{OnceCell, RefCell};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// A chart a player knows it has: where, its hashes (hex; either may be
/// empty), its title and artist.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Owned {
    pub path: PathBuf,
    pub md5: String,
    pub sha256: String,
    pub title: String,
    pub artist: String,
}

/// The library below a folder.
#[derive(Default)]
pub struct Library {
    root: PathBuf,
    /// A folder below passed by (the downloads waiting).
    skip: Option<PathBuf>,
    /// Its songs' folders, read when first needed.
    songs: OnceCell<Songs>,
    /// Charts read or given.
    charts: RefCell<Charts>,
    /// A chart's MD5 or SHA-256 (lower case hex): the folders holding it.
    by_hash: HashMap<String, Vec<PathBuf>>,
}

/// Charts read or given, by path (None: it could not be read), and by
/// their artist as compared: (title as compared, path).
#[derive(Default)]
struct Charts {
    by_path: HashMap<PathBuf, Option<ChartInfo>>,
    by_artist: HashMap<String, Vec<(String, PathBuf)>>,
}

impl Charts {
    fn insert(&mut self, p: PathBuf, info: Option<ChartInfo>) -> &Option<ChartInfo> {
        if let Some(Some(old)) = self.by_path.get(&p) {
            let old = titles::Song::of(&old.title, &old.artist);
            if let Some(at) = self.by_artist.get_mut(&old.artist) {
                at.retain(|(_, q)| *q != p);
            }
        }
        if let Some(i) = &info {
            let song = titles::Song::of(&i.title, &i.artist);
            if !song.title.is_empty() {
                let at = self.by_artist.entry(song.artist).or_default();
                at.push((song.title, p.clone()));
            }
        }
        let slot = self.by_path.entry(p).or_default();
        *slot = info;
        slot
    }

    /// The folders of the charts of `song` ([`titles::same_song`]) whose
    /// path `keep` keeps.
    fn folders_of(&self, song: &titles::Song, keep: impl Fn(&Path) -> bool) -> Vec<PathBuf> {
        let at = self
            .by_artist
            .get(&song.artist)
            .map_or(&[][..], Vec::as_slice);
        at.iter()
            .filter(|(title, p)| {
                let other = titles::Song {
                    title: title.clone(),
                    artist: song.artist.clone(),
                };
                keep(p) && titles::same_song(song, &other)
            })
            .filter_map(|(_, p)| p.parent().map(Path::to_path_buf))
            .collect()
    }
}

#[derive(Default)]
struct Songs {
    /// The songs' folders, by path.
    list: Vec<Song>,
    /// A folder's place in `list`.
    at: HashMap<PathBuf, usize>,
    /// The numbers of the files' keys.
    names: Names,
    /// A sound file's key (its number): the folders (places in `list`)
    /// holding one.
    by_sound: HashMap<u32, Vec<usize>>,
    /// Files come in where there is no song's folder (yet): a chart coming
    /// in above them makes them its song's.
    loose: Vec<PathBuf>,
}

struct Song {
    dir: PathBuf,
    /// Its files by their path in it, sorted.
    files: Vec<String>,
    /// The charts right in it, by name, sorted.
    charts: Vec<String>,
    keys: Keys,
}

impl Songs {
    /// The song's folder `dir` holds `files` (its paths in it).
    fn merge(&mut self, dir: &Path, files: impl IntoIterator<Item = String>) {
        let i = *self.at.entry(dir.to_path_buf()).or_insert_with(|| {
            self.list.push(Song {
                dir: dir.to_path_buf(),
                files: Vec::new(),
                charts: Vec::new(),
                keys: Keys::default(),
            });
            self.list.len() - 1
        });
        let song = &mut self.list[i];
        for f in files {
            let Err(at) = song.files.binary_search(&f) else {
                continue;
            };
            if let Some((Class::Audio, k)) = song.keys.add(&self.names, &f) {
                self.by_sound.entry(k).or_default().push(i);
            }
            if !f.contains('/') && is_chart(Path::new(&f)) {
                let c = song.charts.binary_search(&f).unwrap_or_else(|c| c);
                song.charts.insert(c, f.clone());
            }
            song.files.insert(at, f);
        }
    }
}

/// A hash as it is kept: trimmed, lower case (None: none).
fn hash_key(h: &str) -> Option<String> {
    let h = h.trim();
    (!h.is_empty()).then(|| h.to_ascii_lowercase())
}

/// A chart file as the library knows it: its hashes and what it says
/// (None: a chart it cannot make out).
struct Read {
    md5: String,
    sha256: String,
    info: Option<ChartInfo>,
}

impl Read {
    /// The chart `p` (None: it cannot be read).
    fn of(fs: &dyn Fs, p: &Path) -> Option<Read> {
        let bytes = fs.read(p).ok()?;
        Some(Read {
            md5: crate::install::md5_hex(&bytes),
            sha256: sha256_hex(&bytes),
            info: crate::chart::info(p, &bytes),
        })
    }

    /// The charts `paths`, in order (natively on as many threads as there
    /// are cores, eight at most).
    fn all(fs: &dyn Fs, paths: &[PathBuf]) -> Vec<Option<Read>> {
        #[cfg(not(target_arch = "wasm32"))]
        {
            let threads = std::thread::available_parallelism().map_or(1, |n| n.get().min(8));
            let chunk = paths.len().div_ceil(threads).max(1);
            std::thread::scope(|s| {
                let parts: Vec<_> = paths
                    .chunks(chunk)
                    .map(|part| s.spawn(move || part.iter().map(|p| Read::of(fs, p)).collect()))
                    .collect();
                let mut out: Vec<Option<Read>> = Vec::with_capacity(paths.len());
                for h in parts {
                    match h.join() {
                        Ok(part) => out.extend::<Vec<_>>(part),
                        Err(e) => std::panic::resume_unwind(e),
                    }
                }
                out
            })
        }
        #[cfg(target_arch = "wasm32")]
        paths.iter().map(|p| Read::of(fs, p)).collect()
    }
}

/// A chart in the scan's cache ([`Library::scan_cached`]).
#[derive(serde::Serialize, serde::Deserialize)]
struct Cached {
    path: PathBuf,
    /// Its modification time (seconds) and size.
    stamp: (i64, u64),
    md5: String,
    sha256: String,
    title: String,
    artist: String,
}

impl Cached {
    fn read(&self) -> Read {
        Read {
            md5: self.md5.clone(),
            sha256: self.sha256.clone(),
            info: Some(ChartInfo {
                title: self.title.clone(),
                artist: self.artist.clone(),
                ..Default::default()
            }),
        }
    }
}

/// The SHA-256 of `bytes`, in hex.
pub(crate) fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::Digest;
    crate::text::hex(&sha2::Sha256::digest(bytes))
}

impl Library {
    /// The library below `root`; its folders read when first needed, its
    /// charts as they are (no hashes known).
    pub fn new(root: &Path) -> Library {
        Library {
            root: root.to_path_buf(),
            skip: None,
            songs: OnceCell::new(),
            charts: RefCell::default(),
            by_hash: HashMap::new(),
        }
    }

    /// The library below `root`, every chart read now (their hashes known).
    #[cfg(test)]
    pub(crate) fn scan(fs: &dyn Fs, root: &Path) -> Library {
        let mut lib = Library::new(root);
        lib.read_all(fs);
        lib
    }

    /// The library below `root` whose charts a player knows (`charts`, from
    /// its own records): their hashes and titles are not read from the
    /// files; its folders are, when first needed.
    pub fn from_catalog(root: &Path, charts: impl IntoIterator<Item = Owned>) -> Library {
        let mut lib = Library::new(root);
        for c in charts {
            let Some(dir) = c.path.parent() else {
                continue;
            };
            for h in [&c.md5, &c.sha256] {
                if let Some(h) = hash_key(h) {
                    lib.hold(h, dir);
                }
            }
            let info = ChartInfo {
                title: c.title,
                artist: c.artist,
                ..Default::default()
            };
            lib.charts.get_mut().insert(c.path, Some(info));
        }
        lib
    }

    pub(crate) fn root(&self) -> &Path {
        &self.root
    }

    /// Whether it holds the chart of this MD5 or SHA-256 (hex).
    pub fn owns(&self, hash: &str) -> bool {
        hash_key(hash).is_some_and(|h| self.by_hash.contains_key(&h))
    }

    /// The folders holding the chart of this MD5 or SHA-256 (hex).
    pub(crate) fn folders_of(&self, hash: &str) -> &[PathBuf] {
        hash_key(hash)
            .and_then(|h| self.by_hash.get(&h))
            .map_or(&[], |v| v.as_slice())
    }

    /// The folders holding a chart of the song `want` ([`titles::same_song`]),
    /// among the charts given or read so far (all of them once
    /// [`Library::scan`]ned).
    pub(crate) fn song_folders_of(&self, want: &crate::hosts::Want) -> Vec<PathBuf> {
        let song = titles::Song::of(&want.title, &want.artist);
        let mut out = self.charts.borrow().folders_of(&song, |_| true);
        out.sort();
        out.dedup();
        out
    }

    /// The folders in it holding a chart of the same song as one of
    /// `charts`, among the charts given or read so far.
    pub(crate) fn song_folders_like(&self, charts: &[ChartInfo]) -> Vec<PathBuf> {
        let known = self.charts.borrow();
        let mut out: Vec<PathBuf> = charts
            .iter()
            .flat_map(|c| {
                let song = titles::Song::of(&c.title, &c.artist);
                known.folders_of(&song, |p| p.starts_with(&self.root))
            })
            .collect();
        out.sort();
        out.dedup();
        out
    }

    /// `file` is in the library now (moved there; `md5`: a chart's): the
    /// song's folder it is in holds it (a chart in a folder that was none:
    /// a song's of its own, with the files that came below it before).
    pub(crate) fn add(&mut self, file: &Path, md5: Option<&str>) {
        let Some(dir) = file.parent() else {
            return;
        };
        if let Some(h) = md5.and_then(hash_key) {
            self.hold(h, dir);
        }
        let root = self.root.clone();
        let Some(s) = self.songs.get_mut() else {
            return;
        };
        if !dir.starts_with(&root) {
            return;
        }
        let song = file
            .ancestors()
            .skip(1)
            .take_while(|d| d.starts_with(&root))
            .find(|d| s.at.contains_key(*d))
            .map(|d| d.to_path_buf());
        match song {
            Some(d) if d == dir || !is_chart(file) => s.merge(&d, [rel(file, &d)]),
            _ if is_chart(file) => {
                let (below, loose) = std::mem::take(&mut s.loose)
                    .into_iter()
                    .partition(|f: &PathBuf| f.starts_with(dir));
                s.loose = loose;
                let files = below.iter().map(|f| rel(f, dir));
                s.merge(dir, std::iter::once(file_name(file)).chain(files));
            }
            _ => s.loose.push(file.to_path_buf()),
        }
    }

    /// The folder `dir` is in the library now (a song unpacked there): its
    /// songs' folders and their files, their charts read (and hashed).
    pub(crate) fn add_folder(&mut self, fs: &dyn Fs, dir: &Path) {
        let found = song_folders_except(fs, dir, None);
        for (song, files) in &found {
            for f in files.iter().filter(|f| !f.contains('/')) {
                let p = song.join(f);
                if is_chart(&p) {
                    self.read_chart(fs, &p);
                }
            }
        }
        if let Some(s) = self.songs.get_mut() {
            for (song, files) in found {
                s.merge(&song, files);
            }
        }
    }

    /// The files of the song's folder `dir`, by their path in it (None: not
    /// a song's folder).
    pub(crate) fn files(&self, fs: &dyn Fs, dir: &Path) -> Option<&[String]> {
        let s = self.songs(fs);
        s.at.get(dir).map(|i| s.list[*i].files.as_slice())
    }

    /// The charts below `dir` (a song's folders there): those given or read
    /// before, else read now.
    pub(crate) fn charts_below(&self, fs: &dyn Fs, dir: &Path) -> Vec<ChartInfo> {
        let s = self.songs(fs);
        let mut out = Vec::new();
        for song in s.list.iter().filter(|s| s.dir.starts_with(dir)) {
            out.extend(self.charts_in(fs, &song.dir, &song.charts, ChartInfo::clone));
        }
        out
    }

    fn hold(&mut self, hash: String, dir: &Path) {
        let at = self.by_hash.entry(hash).or_default();
        if !at.iter().any(|d| d == dir) {
            at.push(dir.to_path_buf());
        }
    }

    /// The chart `p` as `r` says (its hashes kept).
    fn know(&mut self, p: PathBuf, r: Read) {
        if let Some(dir) = p.parent() {
            self.hold(r.md5, dir);
            self.hold(r.sha256, dir);
        }
        self.charts.get_mut().insert(p, r.info);
    }

    /// The chart `p`, read (and its hashes kept).
    fn read_chart(&mut self, fs: &dyn Fs, p: &Path) {
        if let Some(r) = Read::of(fs, p) {
            self.know(p.to_path_buf(), r);
        }
    }

    /// The paths of its songs' charts.
    fn chart_paths(&self, fs: &dyn Fs) -> Vec<PathBuf> {
        let s = self.songs(fs);
        s.list
            .iter()
            .flat_map(|s| s.charts.iter().map(|c| s.dir.join(c)))
            .collect()
    }

    /// Every song's folder and chart, read now.
    #[cfg(test)]
    fn read_all(&mut self, fs: &dyn Fs) {
        let paths = self.chart_paths(fs);
        for (p, r) in paths.iter().zip(Read::all(fs, &paths)) {
            if let Some(r) = r {
                self.know(p.clone(), r);
            }
        }
    }

    /// The library below `root`, every chart read now, passing by the
    /// folder `skip` (the downloads waiting for their songs, when they are
    /// kept in the library); but a chart whose time and size the file
    /// `cache` (written by an earlier scan) holds is taken from it, not
    /// read: its hashes, title and artist. `cache` written again after.
    pub fn scan_cached(fs: &dyn Store, root: &Path, skip: &Path, cache: &Path) -> Library {
        let mut lib = Library::new(root);
        lib.skip = Some(skip.to_path_buf());
        let read = fs.as_fs();
        let mut known: HashMap<PathBuf, Cached> = read
            .read(cache)
            .ok()
            .and_then(|b| serde_json::from_slice::<Vec<Cached>>(&b).ok())
            .unwrap_or_default()
            .into_iter()
            .map(|c| (c.path.clone(), c))
            .collect();
        let paths = lib.chart_paths(read);
        let stamps: Vec<_> = paths.iter().map(|p| read.stamp(p)).collect();
        let hits: Vec<Option<Cached>> = paths
            .iter()
            .zip(&stamps)
            .map(|(p, stamp)| known.remove(p).filter(|c| Some(c.stamp) == *stamp))
            .collect();
        let missed: Vec<PathBuf> = paths
            .iter()
            .zip(&hits)
            .filter(|(_, hit)| hit.is_none())
            .map(|(p, _)| p.clone())
            .collect();
        let mut fresh = Read::all(read, &missed).into_iter();
        let mut kept = Vec::with_capacity(paths.len());
        for ((p, stamp), hit) in paths.into_iter().zip(stamps).zip(hits) {
            let r = match hit {
                Some(c) => {
                    let r = c.read();
                    kept.push(c);
                    r
                }
                None => {
                    let Some(r) = fresh.next().flatten() else {
                        continue;
                    };
                    if let (Some(stamp), Some(i)) = (stamp, &r.info) {
                        kept.push(Cached {
                            path: p.clone(),
                            stamp,
                            md5: r.md5.clone(),
                            sha256: r.sha256.clone(),
                            title: i.title.clone(),
                            artist: i.artist.clone(),
                        });
                    }
                    r
                }
            };
            lib.know(p, r);
        }
        match fs.create(cache, None) {
            Ok(w) => {
                if let Err(e) = serde_json::to_writer(std::io::BufWriter::new(w), &kept) {
                    log::warn!("{}: {e}", cache.display());
                }
            }
            Err(e) => log::warn!("{}: {e}", cache.display()),
        }
        lib
    }

    fn songs(&self, fs: &dyn Fs) -> &Songs {
        self.songs.get_or_init(|| {
            let mut s = Songs::default();
            for (dir, files) in song_folders_except(fs, &self.root, self.skip.as_deref()) {
                s.merge(&dir, files);
            }
            s
        })
    }

    /// The numbers of the library's files' keys.
    pub(crate) fn names(&self, fs: &dyn Fs) -> &Names {
        &self.songs(fs).names
    }

    /// The songs' folders other than `except` holding a sound file of one
    /// of `sounds` (keys' numbers), by path (their charts not read).
    pub(crate) fn near(&self, fs: &dyn Fs, sounds: &[u32], except: &Path) -> Vec<Folder<'_>> {
        let s = self.songs(fs);
        let mut hit = vec![false; s.list.len()];
        for i in sounds.iter().filter_map(|k| s.by_sound.get(k)).flatten() {
            hit[*i] = true;
        }
        s.list
            .iter()
            .zip(hit)
            .filter(|(song, hit)| *hit && song.dir != except)
            .map(|(song, _)| Folder {
                dir: &song.dir,
                keys: &song.keys,
                titles: &[],
            })
            .collect()
    }

    /// What `of` takes from the charts `names` right in the folder `dir`.
    fn charts_in<T>(
        &self,
        fs: &dyn Fs,
        dir: &Path,
        names: &[String],
        of: impl Fn(&ChartInfo) -> T,
    ) -> Vec<T> {
        let mut known = self.charts.borrow_mut();
        names
            .iter()
            .filter_map(|n| {
                let p = dir.join(n);
                let info = match known.by_path.get(&p) {
                    Some(info) => info,
                    None => {
                        let info = crate::placement::chart_info(fs, &p);
                        known.insert(p, info)
                    }
                };
                info.as_ref().map(&of)
            })
            .collect()
    }

    /// The titles and artists of the charts right in the song's folder
    /// `dir`.
    pub(crate) fn folder_titles(&self, fs: &dyn Fs, dir: &Path) -> Vec<(String, String)> {
        let s = self.songs(fs);
        s.at.get(dir).map_or(Vec::new(), |&i| {
            self.charts_in(fs, dir, &s.list[i].charts, |c| {
                (c.title.clone(), c.artist.clone())
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fs::Mem;

    fn library() -> Mem {
        let m = Mem::default();
        let put = |p: &str, s: &str| m.put(Path::new(p), s.as_bytes(), 0);
        put(
            "bms/Song/a.bms",
            "#TITLE Song\n#ARTIST x\n#WAV01 sound/01.wav\n",
        );
        put("bms/Song/sound/01.ogg", "x");
        put("bms/Pack/Other/b.bme", "#TITLE Other\n#ARTIST y\n");
        put("bms/Pack/Other/02.ogg", "x");
        put("bms/.bmseeker/waiting.bms", "#TITLE Waiting\n");
        put("bms/dl/waiting2.bms", "#TITLE Waiting 2\n");
        m
    }

    #[test]
    fn scanned() {
        let m = library();
        let cache = Path::new("bms/dl/scan.json");
        let lib = Library::scan_cached(&m, Path::new("bms"), Path::new("bms/dl"), cache);
        let md5 = crate::install::md5_hex(b"#TITLE Other\n#ARTIST y\n");
        assert!(lib.owns(&md5) && lib.owns(&md5.to_uppercase()));
        assert_eq!(lib.folders_of(&md5), [PathBuf::from("bms/Pack/Other")]);
        assert!(lib.owns(&sha256_hex(b"#TITLE Other\n#ARTIST y\n")));
        // the downloads waiting are not the library's
        assert!(!lib.owns(&crate::install::md5_hex(b"#TITLE Waiting\n")));
        assert!(!lib.owns(&crate::install::md5_hex(b"#TITLE Waiting 2\n")));
        assert!(!lib.owns("") && lib.folders_of("").is_empty());
        assert_eq!(
            lib.files(&m, Path::new("bms/Song")).unwrap(),
            ["a.bms", "sound/01.ogg"]
        );
        let titles: Vec<String> = lib
            .charts_below(&m, Path::new("bms/Pack"))
            .into_iter()
            .map(|c| c.title)
            .collect();
        assert_eq!(titles, ["Other"]);
        // scanned again: a chart of the time and size it had is the cache's
        // (its hashes, title and artist); another is read
        let new = b"#TITLE Newer\n#ARTIST y\n";
        m.put(
            Path::new("bms/Pack/Other/b.bme"),
            b"#TITLE Wrong\n#ARTIST y\n",
            0,
        );
        m.put(Path::new("bms/Song/a.bms"), new, 5);
        let again = Library::scan_cached(&m, Path::new("bms"), Path::new("bms/dl"), cache);
        assert!(again.owns(&md5) && again.owns(&crate::install::md5_hex(new)));
        let titles = |lib: &Library| -> Vec<String> {
            lib.charts_below(&m, Path::new("bms"))
                .into_iter()
                .map(|c| c.title)
                .collect()
        };
        assert_eq!(titles(&again), ["Other", "Newer"]);
        // a cache that cannot be read: every chart read
        m.put(cache, b"{", 0);
        let read = Library::scan_cached(&m, Path::new("bms"), Path::new("bms/dl"), cache);
        assert_eq!(titles(&read), ["Wrong", "Newer"]);
        // a chart and a song coming in
        let mut lib = lib;
        lib.add(Path::new("bms/Song/c.bms"), Some("ABC"));
        assert_eq!(lib.folders_of("abc"), [PathBuf::from("bms/Song")]);
        m.put(Path::new("bms/New/n.bms"), b"#TITLE New\n", 0);
        m.put(Path::new("bms/New/n.ogg"), b"x", 0);
        lib.add_folder(&m, Path::new("bms/New"));
        assert!(lib.owns(&crate::install::md5_hex(b"#TITLE New\n")));
        assert_eq!(
            lib.files(&m, Path::new("bms/New")).unwrap(),
            ["n.bms", "n.ogg"]
        );
        // a song's sounds before its chart, file by file
        lib.add(Path::new("bms/Later/sound/01.ogg"), None);
        lib.add(Path::new("bms/Elsewhere/02.ogg"), None);
        assert!(lib.files(&m, Path::new("bms/Later")).is_none());
        lib.add(Path::new("bms/Later/l.bms"), None);
        assert_eq!(
            lib.files(&m, Path::new("bms/Later")).unwrap(),
            ["l.bms", "sound/01.ogg"]
        );
    }

    /// A song's folders by its charts' titles and artists ([`titles::same_song`]),
    /// as charts are given, read, and read again.
    #[test]
    fn songs_by_title() {
        let owned = |p: &str, title: &str, artist: &str| Owned {
            path: p.into(),
            title: title.into(),
            artist: artist.into(),
            ..Default::default()
        };
        let lib = Library::from_catalog(
            Path::new("bms"),
            [
                owned("bms/A/a.bms", "Song [NORMAL]", "alpha"),
                owned("bms/B/b.bms", "Song [HYPER]", "beta"),
                owned("other/C/c.bms", "Song", "alpha / obj: x"),
            ],
        );
        let want = |t: &str, a: &str| crate::hosts::Want {
            title: t.into(),
            artist: a.into(),
        };
        assert_eq!(
            lib.song_folders_of(&want("Song [ANOTHER]", "alpha obj: y")),
            [PathBuf::from("bms/A"), PathBuf::from("other/C")]
        );
        assert!(lib.song_folders_of(&want("Other", "alpha")).is_empty());
        let chart = |t: &str, a: &str| ChartInfo {
            title: t.into(),
            artist: a.into(),
            ..Default::default()
        };
        // in the library only
        assert_eq!(
            lib.song_folders_like(&[chart("Zzz", "q"), chart("Song", "alpha")]),
            [PathBuf::from("bms/A")]
        );
        // a chart read again: by what it is now
        let m = Mem::default();
        m.put(Path::new("bms/A/a.bms"), b"#TITLE Else\n#ARTIST alpha\n", 0);
        let mut lib = lib;
        lib.read_chart(&m, Path::new("bms/A/a.bms"));
        assert_eq!(
            lib.song_folders_of(&want("Song", "alpha")),
            [PathBuf::from("other/C")]
        );
        assert_eq!(
            lib.song_folders_of(&want("Else", "alpha")),
            [PathBuf::from("bms/A")]
        );
    }

    /// A library of 3,000 songs' folders with numbered sounds (`01.wav`
    /// ...), most of which hold most of a 差分's sounds: the time to walk
    /// its folders, to read them (walked and indexed), and to place 50
    /// charts (with a digest of where, to compare). `cargo test --release large_library -- --ignored --nocapture`
    #[test]
    #[ignore = "a measurement"]
    fn large_library() {
        use crate::fs::Entry;
        use crate::placement::{locate_by, Rules};
        use std::hash::{Hash, Hasher};
        use std::time::Instant;
        #[derive(Default)]
        struct Tree {
            dirs: HashMap<PathBuf, Vec<Entry>>,
            files: HashMap<PathBuf, Vec<u8>>,
        }
        impl Tree {
            fn put(&mut self, p: PathBuf, body: Vec<u8>) {
                let mut child = p.clone();
                let mut is_dir = false;
                while let Some(dir) = child.parent().map(Path::to_path_buf) {
                    let e = self.dirs.entry(dir.clone()).or_default();
                    if e.iter().any(|e| e.path == child) {
                        break;
                    }
                    e.push(Entry {
                        path: child,
                        is_dir,
                    });
                    (child, is_dir) = (dir, true);
                }
                self.files.insert(p, body);
            }
        }
        impl Fs for Tree {
            fn read(&self, p: &Path) -> std::io::Result<Vec<u8>> {
                self.files
                    .get(p)
                    .cloned()
                    .ok_or(std::io::ErrorKind::NotFound.into())
            }
            fn read_dir(&self, p: &Path) -> std::io::Result<Vec<Entry>> {
                self.dirs
                    .get(p)
                    .cloned()
                    .ok_or(std::io::ErrorKind::NotFound.into())
            }
            fn rename(&self, _: &Path, _: &Path) -> std::io::Result<()> {
                Err(std::io::ErrorKind::Unsupported.into())
            }
            fn stamp(&self, _: &Path) -> Option<(i64, u64)> {
                None
            }
            fn remove(&self, _: &Path) -> std::io::Result<()> {
                Err(std::io::ErrorKind::Unsupported.into())
            }
        }
        let mut t = Tree::default();
        for i in 0..3000 {
            let d = PathBuf::from(format!("bms/Song {i}"));
            let chart = format!("#TITLE Song {i}\n#ARTIST Artist {}\n", i % 300);
            t.put(d.join("song.bms"), chart.into_bytes());
            for j in 1..=100 + i % 200 {
                t.put(d.join(format!("{j:02}.wav")), Vec::new());
            }
            t.put(d.join(format!("bga/{i}.bmp")), Vec::new());
            t.put(d.join("bg.png"), Vec::new());
        }
        let started = Instant::now();
        crate::placement::song_folders(&t, Path::new("bms"));
        let walk = started.elapsed();
        let started = Instant::now();
        let lib = Library::new(Path::new("bms"));
        assert!(lib.files(&t, Path::new("bms/Song 0")).is_some());
        let read = started.elapsed();
        let charts: Vec<ChartInfo> = (0..50)
            .map(|c| ChartInfo {
                title: format!("Song {} [ANOTHER]", c * 60),
                artist: format!("Artist {} / obj: x", c * 60 % 300),
                sounds: (1..=80 + c * 3).map(|j| format!("{j:02}.wav")).collect(),
                images: vec![format!("bga/{}.bmp", c * 60)],
                stagefile: "bg.png".into(),
                ..Default::default()
            })
            .collect();
        let started = Instant::now();
        let mut digest = std::collections::hash_map::DefaultHasher::new();
        for c in &charts {
            let r = locate_by(&t, c, &[], Path::new("dl"), &lib, Rules::default());
            format!("{r:?}").hash(&mut digest);
        }
        let placed = started.elapsed();
        println!(
            "walked {walk:?}, read {read:?}, placed 50 in {placed:?}, digest {:016x}",
            digest.finish()
        );
    }

    /// A player's records stand for the charts it cannot read (a web
    /// page's): their hashes, titles and artists.
    #[test]
    fn from_a_catalog() {
        use crate::placement::{locate, Located, Rules};
        // the library's charts cannot be read; its folders can be listed
        struct Unread(Mem);
        impl Fs for Unread {
            fn read(&self, p: &Path) -> std::io::Result<Vec<u8>> {
                if p.starts_with("bms") {
                    return Err(std::io::ErrorKind::WouldBlock.into());
                }
                self.0.read(p)
            }
            fn read_dir(&self, p: &Path) -> std::io::Result<Vec<crate::fs::Entry>> {
                self.0.read_dir(p)
            }
            fn rename(&self, from: &Path, to: &Path) -> std::io::Result<()> {
                self.0.rename(from, to)
            }
            fn stamp(&self, p: &Path) -> Option<(i64, u64)> {
                self.0.stamp(p)
            }
            fn remove(&self, p: &Path) -> std::io::Result<()> {
                self.0.remove(p)
            }
        }
        let fs = Unread(library());
        fs.0.put(
            Path::new("dl/x.bms"),
            b"#TITLE Song [X]\n#ARTIST x / obj\n#WAV01 sound/01.wav\n",
            0,
        );
        let at = |lib: &Library| {
            locate(&fs, Path::new("dl/x.bms"), lib, false, Rules::default()).map(|r| r.0)
        };
        // read from the files: its song's charts unread, so not sure
        assert!(matches!(
            at(&Library::new(Path::new("bms"))),
            Some(Located::Ask(_))
        ));
        let lib = Library::from_catalog(
            Path::new("bms"),
            [Owned {
                path: "bms/Song/a.bms".into(),
                md5: "0123456789ABCDEF0123456789ABCDEF".into(),
                sha256: String::new(),
                title: "Song".into(),
                artist: "x".into(),
            }],
        );
        assert_eq!(at(&lib), Some(Located::Move("bms/Song".into())));
        assert!(lib.owns("0123456789abcdef0123456789abcdef"));
        assert_eq!(
            lib.folders_of("0123456789abcdef0123456789abcdef"),
            [PathBuf::from("bms/Song")]
        );
        assert!(!lib.owns(""));
    }
}
