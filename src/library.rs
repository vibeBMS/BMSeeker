//! A BMS library as the install tools see it: its songs' folders (those
//! holding charts) with the files below each by their path in it, the
//! charts' titles and artists, and which folders hold a chart of an MD5 or
//! SHA-256. Read from the files ([`Library::scan`], or folder by folder
//! when first needed: [`Library::new`]) or given by a player that keeps its
//! own records of its charts ([`Library::from_catalog`]: a web page cannot
//! read its library's charts when it likes).

use crate::chart::ChartInfo;
use crate::fs::{file_name, Fs};
use crate::placement::{class_of, is_chart, key, rel, song_folders_except, Class, FolderInfo};
use std::cell::{OnceCell, RefCell};
use std::collections::{HashMap, HashSet};
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
pub struct Library {
    root: PathBuf,
    /// A folder below passed by (the downloads waiting).
    skip: Option<PathBuf>,
    /// Its songs' folders, read when first needed.
    songs: OnceCell<Songs>,
    /// Charts read or given, by path (None: it could not be read).
    charts: RefCell<HashMap<PathBuf, Option<ChartInfo>>>,
    /// A chart's MD5 or SHA-256 (lower case hex): the folders holding it.
    by_hash: HashMap<String, Vec<PathBuf>>,
}

#[derive(Default)]
struct Songs {
    /// (song's folder, its files by their path in it), by path.
    list: Vec<(PathBuf, Vec<String>)>,
    /// A sound file's key: the folders (in `list`) holding one.
    by_sound: HashMap<String, Vec<usize>>,
    /// A folder's place in `list`.
    at: HashMap<PathBuf, usize>,
}

impl Songs {
    fn index(&mut self, i: usize, file: &str) {
        let (Some(Class::Audio), Some(Ok(k))) = (class_of(file), key(file)) else {
            return;
        };
        let at = self.by_sound.entry(k).or_default();
        if !at.contains(&i) {
            at.push(i);
        }
    }

    /// The song's folder `dir` holds `files` (its paths in it).
    fn merge(&mut self, dir: &Path, files: impl IntoIterator<Item = String>) {
        let i = match self.at.get(dir) {
            Some(i) => *i,
            None => {
                self.list.push((dir.to_path_buf(), Vec::new()));
                self.at.insert(dir.to_path_buf(), self.list.len() - 1);
                self.list.len() - 1
            }
        };
        for f in files {
            self.index(i, &f);
            if !self.list[i].1.contains(&f) {
                self.list[i].1.push(f);
            }
        }
    }
}

/// A hash as it is kept: trimmed, lower case (None: none).
fn hash_key(h: &str) -> Option<String> {
    let h = h.trim();
    (!h.is_empty()).then(|| h.to_ascii_lowercase())
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
            charts: RefCell::new(HashMap::new()),
            by_hash: HashMap::new(),
        }
    }

    /// The library below `root`, every chart read now (their hashes known).
    pub fn scan(fs: &dyn Fs, root: &Path) -> Library {
        let mut lib = Library::new(root);
        lib.read_all(fs);
        lib
    }

    /// [`Library::scan`] passing by the folder `skip` (the downloads
    /// waiting for their songs, when they are kept in the library).
    pub fn scan_except(fs: &dyn Fs, root: &Path, skip: &Path) -> Library {
        let mut lib = Library::new(root);
        lib.skip = Some(skip.to_path_buf());
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

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// The folder below which it is (as [`Library::root`]).
    pub fn library(&self) -> &Path {
        &self.root
    }

    /// Whether it holds the chart of this MD5 or SHA-256 (hex).
    pub fn owns(&self, hash: &str) -> bool {
        hash_key(hash).is_some_and(|h| self.by_hash.contains_key(&h))
    }

    /// The folders holding the chart of this MD5 or SHA-256 (hex).
    pub fn folders_of(&self, hash: &str) -> &[PathBuf] {
        hash_key(hash)
            .and_then(|h| self.by_hash.get(&h))
            .map_or(&[], |v| v.as_slice())
    }

    /// The folders holding a chart of the song `want` (its title but for
    /// the chart's name, and its artist), among the charts given or read so
    /// far (all of them once [`Library::scan`]ned).
    pub fn song_folders_of(&self, want: &crate::hosts::Want) -> Vec<PathBuf> {
        let mut out: Vec<PathBuf> = self
            .charts
            .borrow()
            .iter()
            .filter(|(_, c)| c.as_ref().is_some_and(|c| crate::install::is_song(c, want)))
            .filter_map(|(p, _)| p.parent().map(Path::to_path_buf))
            .collect();
        out.sort();
        out.dedup();
        out
    }

    /// The folders holding a chart of the same song as one of `charts` (the
    /// title but for the chart's name, and the artist), among the charts
    /// given or read so far.
    pub fn song_folders_like(&self, charts: &[ChartInfo]) -> Vec<PathBuf> {
        use crate::placement::{same_artist, same_song_title};
        let mut out: Vec<PathBuf> = self
            .charts
            .borrow()
            .iter()
            .filter(|(p, c)| {
                p.starts_with(&self.root)
                    && c.as_ref().is_some_and(|c| {
                        charts.iter().any(|n| {
                            same_song_title(&c.title, &n.title) && same_artist(&c.artist, &n.artist)
                        })
                    })
            })
            .filter_map(|(p, _)| p.parent().map(Path::to_path_buf))
            .collect();
        out.sort();
        out.dedup();
        out
    }

    /// `file` is in the library now (moved there; `md5`: a chart's): the
    /// song's folder it is in holds it (a chart in a folder that was none:
    /// a song's of its own).
    pub fn add(&mut self, file: &Path, md5: Option<&str>) {
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
            _ if is_chart(file) => s.merge(dir, [file_name(file)]),
            _ => {}
        }
    }

    /// The folder `dir` is in the library now (a song unpacked there): its
    /// songs' folders and their files, their charts read (and hashed).
    pub fn add_folder(&mut self, fs: &dyn Fs, dir: &Path) {
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
    pub fn files(&self, fs: &dyn Fs, dir: &Path) -> Option<&[String]> {
        let s = self.songs(fs);
        s.at.get(dir).map(|i| s.list[*i].1.as_slice())
    }

    /// The charts below `dir` (a song's folders there): those given or read
    /// before, else read now.
    pub fn charts_below(&self, fs: &dyn Fs, dir: &Path) -> Vec<ChartInfo> {
        let s = self.songs(fs);
        let mut out = Vec::new();
        for (d, files) in s.list.iter().filter(|(d, _)| d.starts_with(dir)) {
            out.extend(self.charts_in(fs, d, files));
        }
        out
    }

    fn hold(&mut self, hash: String, dir: &Path) {
        let at = self.by_hash.entry(hash).or_default();
        if !at.iter().any(|d| d == dir) {
            at.push(dir.to_path_buf());
        }
    }

    /// The chart `p`, read (and its hashes kept).
    fn read_chart(&mut self, fs: &dyn Fs, p: &Path) {
        let Ok(bytes) = fs.read(p) else {
            return;
        };
        if let Some(dir) = p.parent() {
            self.hold(crate::install::md5_hex(&bytes), dir);
            self.hold(sha256_hex(&bytes), dir);
        }
        let info = crate::chart::info(p, &bytes);
        self.charts.get_mut().insert(p.to_path_buf(), info);
    }

    /// Every song's folder and chart, read now.
    fn read_all(&mut self, fs: &dyn Fs) {
        self.songs(fs);
        let charts: Vec<PathBuf> = self.songs.get().map_or(Vec::new(), |s| {
            s.list
                .iter()
                .flat_map(|(d, files)| {
                    files
                        .iter()
                        .filter(|f| !f.contains('/'))
                        .map(|f| d.join(f))
                        .filter(|p| is_chart(p))
                })
                .collect()
        });
        for c in charts {
            self.read_chart(fs, &c);
        }
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

    /// The songs' folders other than `except` holding a sound file of one
    /// of `sounds` (keys), by path.
    pub(crate) fn near(
        &self,
        fs: &dyn Fs,
        sounds: &HashSet<String>,
        except: &Path,
    ) -> Vec<FolderInfo> {
        let s = self.songs(fs);
        let mut at: Vec<usize> = sounds
            .iter()
            .filter_map(|k| s.by_sound.get(k))
            .flatten()
            .copied()
            .collect();
        at.sort_unstable();
        at.dedup();
        at.into_iter()
            .map(|i| &s.list[i])
            .filter(|(d, _)| d != except)
            .map(|(dir, files)| FolderInfo {
                dir: dir.clone(),
                files: files.clone(),
                charts: Vec::new(),
            })
            .collect()
    }

    /// The charts right in the song's folder `dir` holding `files`.
    fn charts_in(&self, fs: &dyn Fs, dir: &Path, files: &[String]) -> Vec<ChartInfo> {
        let mut known = self.charts.borrow_mut();
        files
            .iter()
            .filter(|n| !n.contains('/'))
            .map(|n| dir.join(n))
            .filter(|p| is_chart(p))
            .filter_map(|p| {
                known
                    .entry(p)
                    .or_insert_with_key(|p| crate::placement::chart_info(fs, p))
                    .clone()
            })
            .collect()
    }

    /// The charts of the folder `f` (those right in it).
    pub(crate) fn folder_charts(&self, fs: &dyn Fs, f: &FolderInfo) -> Vec<ChartInfo> {
        self.charts_in(fs, &f.dir, &f.files)
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
        let lib = Library::scan_except(&m, Path::new("bms"), Path::new("bms/dl"));
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
            fn rename(&self, from: &Path, to: &Path) -> bool {
                self.0.rename(from, to)
            }
            fn stamp(&self, p: &Path) -> Option<(i64, u64)> {
                self.0.stamp(p)
            }
            fn remove(&self, p: &Path) -> bool {
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
