//! The files the library tools read and move: a disk ([`Disk`]) or whatever
//! a player keeps its library in. [`Fs`] reads and moves; a [`Store`] also
//! writes (what an archive unpacks to).

use std::io::{self, Read, Seek, Write};
use std::path::{Path, PathBuf};

/// A directory entry.
#[derive(Clone, Debug)]
pub struct Entry {
    pub path: PathBuf,
    pub is_dir: bool,
}

pub trait Fs: Sync {
    fn read(&self, p: &Path) -> io::Result<Vec<u8>>;
    fn read_dir(&self, p: &Path) -> io::Result<Vec<Entry>>;
    /// Move a file; an error when `to` exists (`AlreadyExists`) or the
    /// move fails.
    fn rename(&self, from: &Path, to: &Path) -> io::Result<()>;
    /// A file's (modification time in seconds, size); None: no such file.
    fn stamp(&self, p: &Path) -> Option<(i64, u64)>;
    /// Delete a file.
    fn remove(&self, p: &Path) -> io::Result<()>;
    /// Whether files keep the times they are written with ([`Store::create`])
    /// and those [`Fs::stamp`] gives can be compared (not so in a web
    /// page's storage).
    fn keeps_times(&self) -> bool {
        true
    }
}

/// What an archive is read through.
pub trait ReadSeek: Read + Seek {}

impl<T: Read + Seek> ReadSeek for T {}

/// Files written as well as read: where archives are unpacked.
pub trait Store: Fs {
    /// A new file `p` (its folders made, a file there replaced), written
    /// through what this gives; once that is dropped, its modification
    /// time is `mtime` (seconds) if the store can keep it (a failure to is
    /// no error).
    fn create(&self, p: &Path, mtime: Option<i64>) -> io::Result<Box<dyn Write + '_>>;
    /// Remove the folder `p` and everything below it.
    fn remove_dir(&self, p: &Path);
    /// The file `p` to read from (an archive in the store): all of it in
    /// memory, unless the store can do better.
    fn open(&self, p: &Path) -> io::Result<Box<dyn ReadSeek + Send>> {
        Ok(Box::new(io::Cursor::new(self.read(p)?)))
    }
    /// Make the folder `p` and those above it (a store without folders of
    /// their own needs nothing).
    fn create_dir_all(&self, _p: &Path) {}
    /// Make the folder `p` (and those above it), which must not be there
    /// (`AlreadyExists`): a folder what passes through is staged in is
    /// this run's own, not one another made first. A store that can makes
    /// it for its owner only.
    fn create_new_dir(&self, p: &Path) -> io::Result<()> {
        if self.stamp(p).is_some() || self.read_dir(p).is_ok() {
            return Err(io::ErrorKind::AlreadyExists.into());
        }
        self.create_dir_all(p);
        Ok(())
    }
    /// Copy the file `from` to the new file `to`, with its time: read
    /// whole and written, unless the store can do better.
    fn copy(&self, from: &Path, to: &Path) -> io::Result<()> {
        let bytes = self.read(from)?;
        let mut w = self.create(to, self.stamp(from).map(|(t, _)| t))?;
        w.write_all(&bytes)?;
        w.flush()
    }
}

/// Move the file `from` to `to` (none there; its folder made): renamed,
/// else copied with its time.
pub(crate) fn move_file(store: &dyn Store, from: &Path, to: &Path) -> io::Result<()> {
    if let Some(d) = to.parent() {
        store.create_dir_all(d);
    }
    match store.rename(from, to) {
        Err(e) if e.kind() != io::ErrorKind::AlreadyExists => {
            store.copy(from, to)?;
            // (what cannot be removed stays where it was)
            let _ = store.remove(from);
            Ok(())
        }
        r => r,
    }
}

/// An archive: a file in the store, or bytes.
#[derive(Debug)]
pub enum Blob {
    File(PathBuf),
    Bytes(Vec<u8>),
}

impl Blob {
    /// The blob as the file `to` of `store` (a file moved there).
    pub(crate) fn put(self, store: &dyn Store, to: &Path) -> io::Result<()> {
        match self {
            Blob::File(p) => move_file(store, &p, to),
            Blob::Bytes(b) => {
                let mut w = store.create(to, None)?;
                w.write_all(&b)?;
                w.flush()
            }
        }
    }
}

/// Seconds since 1970.
pub(crate) fn now() -> u64 {
    web_time::SystemTime::now()
        .duration_since(web_time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// `mtime` (seconds since 1970) as a time (None: before it).
pub(crate) fn system_time(mtime: i64) -> Option<std::time::SystemTime> {
    let secs = u64::try_from(mtime).ok()?;
    std::time::UNIX_EPOCH.checked_add(std::time::Duration::from_secs(secs))
}

/// A path's file name (lossy), empty when it has none.
pub fn file_name(p: &Path) -> String {
    p.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// Whether Windows keeps the file name `name` for a device, whatever
/// comes after its first dot (`CON`, `nul.txt`, `COM1`, `LPT²`).
pub(crate) fn device_name(name: &str) -> bool {
    let stem = name.split('.').next().unwrap_or("").trim_end_matches(' ');
    let stem = stem.to_ascii_uppercase();
    let numbered = |port: &str| {
        let mut rest = stem.strip_prefix(port).unwrap_or("").chars();
        let digit = rest.next().is_some_and(|c| "0123456789¹²³".contains(c));
        digit && rest.next().is_none()
    };
    let devices = ["CON", "PRN", "AUX", "NUL", "CONIN$", "CONOUT$", "CLOCK$"];
    devices.contains(&stem.as_str()) || numbered("COM") || numbered("LPT")
}

/// One part of a path (a file's or a folder's name) as every file system
/// holds it, Windows' too, wherever it is written (a library may be
/// copied there): the characters Windows refuses and control characters
/// as `_`, no dot or space at its end (Windows drops them: `a.bms.` would
/// be `a.bms`), `_` before a device's name. None: nothing is left, or it
/// is `.` or `..`.
pub(crate) fn safe_part(part: &str) -> Option<String> {
    let part: String = part
        .chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            c if c.is_control() => '_',
            c => c,
        })
        .collect();
    let part = part.trim_end_matches(['.', ' ']);
    match part {
        "" => None,
        p if device_name(p) => Some(format!("_{p}")),
        p => Some(p.to_string()),
    }
}

/// `std::fs`.
pub struct Disk;

impl Fs for Disk {
    fn read(&self, p: &Path) -> io::Result<Vec<u8>> {
        std::fs::read(p)
    }

    fn read_dir(&self, p: &Path) -> io::Result<Vec<Entry>> {
        Ok(std::fs::read_dir(p)?
            .flatten()
            .map(|e| {
                let path = e.path();
                let is_dir = e
                    .file_type()
                    .map(|t| t.is_dir() || (t.is_symlink() && path.is_dir()))
                    .unwrap_or(false);
                Entry { path, is_dir }
            })
            .collect())
    }

    fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
        // a link is made only where there is no file (a rename replaces
        // one, and checking first leaves a moment for another to come)
        match std::fs::hard_link(from, to) {
            Ok(()) => std::fs::remove_file(from).inspect_err(|_| {
                let _ = std::fs::remove_file(to);
            }),
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => Err(e),
            // no links there (FAT, exFAT; another device: the rename fails too)
            Err(_) if to.exists() => Err(io::ErrorKind::AlreadyExists.into()),
            Err(_) => std::fs::rename(from, to),
        }
    }

    fn stamp(&self, p: &Path) -> Option<(i64, u64)> {
        let m = std::fs::metadata(p).ok().filter(|m| m.is_file())?;
        let t = m
            .modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map_or(0, |d| d.as_secs() as i64);
        Some((t, m.len()))
    }

    fn remove(&self, p: &Path) -> io::Result<()> {
        std::fs::remove_file(p)
    }
}

/// A file on a disk being written; its time set once it is done.
struct Timed {
    w: io::BufWriter<std::fs::File>,
    mtime: Option<std::time::SystemTime>,
}

impl Write for Timed {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.w.write(buf)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.w.flush()
    }
}

impl Drop for Timed {
    fn drop(&mut self) {
        // (the archive's time if it can be kept; else the time of writing)
        if self.w.flush().is_ok() {
            if let Some(t) = self.mtime {
                let _ = self.w.get_ref().set_modified(t);
            }
        }
    }
}

impl Store for Disk {
    fn create(&self, p: &Path, mtime: Option<i64>) -> io::Result<Box<dyn Write + '_>> {
        if let Some(dir) = p.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let f = std::fs::File::create(p)?;
        Ok(Box::new(Timed {
            w: io::BufWriter::new(f),
            mtime: mtime.and_then(system_time),
        }))
    }

    fn remove_dir(&self, p: &Path) {
        let _ = std::fs::remove_dir_all(p);
    }

    fn open(&self, p: &Path) -> io::Result<Box<dyn ReadSeek + Send>> {
        // (the archives' readers read headers a few bytes at a time)
        Ok(Box::new(io::BufReader::new(std::fs::File::open(p)?)))
    }

    fn create_dir_all(&self, p: &Path) {
        let _ = std::fs::create_dir_all(p);
    }

    fn create_new_dir(&self, p: &Path) -> io::Result<()> {
        if let Some(dir) = p.parent() {
            std::fs::create_dir_all(dir)?;
        }
        #[cfg_attr(not(unix), allow(unused_mut))]
        let mut dir = std::fs::DirBuilder::new();
        #[cfg(unix)]
        std::os::unix::fs::DirBuilderExt::mode(&mut dir, 0o700);
        dir.create(p)
    }

    fn copy(&self, from: &Path, to: &Path) -> io::Result<()> {
        if let Some(dir) = to.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::copy(from, to)?;
        let mtime = std::fs::metadata(from)?.modified()?;
        std::fs::File::options()
            .write(true)
            .open(to)?
            .set_modified(mtime)
    }
}

/// A store in memory, for tests: (bytes, time) by path.
#[cfg(test)]
#[derive(Default)]
pub(crate) struct Mem(pub std::sync::Mutex<std::collections::BTreeMap<PathBuf, (Vec<u8>, i64)>>);

#[cfg(test)]
impl Mem {
    pub(crate) fn put(&self, p: &Path, data: &[u8], mtime: i64) {
        self.0
            .lock()
            .unwrap()
            .insert(p.to_path_buf(), (data.to_vec(), mtime));
    }
    pub(crate) fn get(&self, p: &Path) -> Option<(Vec<u8>, i64)> {
        self.0.lock().unwrap().get(p).cloned()
    }
}

#[cfg(test)]
impl Fs for Mem {
    fn read(&self, p: &Path) -> io::Result<Vec<u8>> {
        self.get(p)
            .map(|f| f.0)
            .ok_or_else(|| io::Error::from(io::ErrorKind::NotFound))
    }
    fn read_dir(&self, p: &Path) -> io::Result<Vec<Entry>> {
        let files = self.0.lock().unwrap();
        let mut out: Vec<Entry> = Vec::new();
        for k in files.keys() {
            let Ok(rest) = k.strip_prefix(p) else {
                continue;
            };
            let mut c = rest.components();
            let Some(first) = c.next() else { continue };
            let path = p.join(first);
            if !out.iter().any(|e| e.path == path) {
                out.push(Entry {
                    path,
                    is_dir: c.next().is_some(),
                });
            }
        }
        if out.is_empty() {
            return Err(io::ErrorKind::NotFound.into());
        }
        Ok(out)
    }
    fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
        let mut files = self.0.lock().unwrap();
        if files.contains_key(to) {
            return Err(io::ErrorKind::AlreadyExists.into());
        }
        let f = files.remove(from).ok_or(io::ErrorKind::NotFound)?;
        files.insert(to.to_path_buf(), f);
        Ok(())
    }
    fn stamp(&self, p: &Path) -> Option<(i64, u64)> {
        self.get(p).map(|(b, t)| (t, b.len() as u64))
    }
    fn remove(&self, p: &Path) -> io::Result<()> {
        let removed = self.0.lock().unwrap().remove(p);
        removed.map(drop).ok_or(io::ErrorKind::NotFound.into())
    }
}

#[cfg(test)]
struct MemFile<'a> {
    mem: &'a Mem,
    p: PathBuf,
    buf: Vec<u8>,
    mtime: i64,
}

#[cfg(test)]
impl Write for MemFile<'_> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.buf.write(buf)
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
impl Drop for MemFile<'_> {
    fn drop(&mut self) {
        self.mem.put(&self.p, &self.buf, self.mtime);
    }
}

#[cfg(test)]
impl Store for Mem {
    fn create(&self, p: &Path, mtime: Option<i64>) -> io::Result<Box<dyn Write + '_>> {
        Ok(Box::new(MemFile {
            mem: self,
            p: p.to_path_buf(),
            buf: Vec::new(),
            mtime: mtime.unwrap_or(0),
        }))
    }
    fn remove_dir(&self, p: &Path) {
        self.0.lock().unwrap().retain(|k, _| !k.starts_with(p));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A move never replaces a file; a copy keeps the time.
    #[test]
    fn disk_moves_and_copies() {
        let d = std::env::temp_dir().join(format!("bmseeker-fs-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        let (a, b) = (d.join("a"), d.join("b"));
        std::fs::write(&a, "a").unwrap();
        std::fs::write(&b, "b").unwrap();
        let e = Disk.rename(&a, &b).unwrap_err();
        assert_eq!(e.kind(), io::ErrorKind::AlreadyExists);
        assert_eq!(std::fs::read(&b).unwrap(), b"b");
        assert!(a.is_file());
        let t = 1_577_836_800;
        let f = std::fs::File::options().write(true).open(&a).unwrap();
        f.set_modified(system_time(t).unwrap()).unwrap();
        let c = d.join("sub/c");
        Disk.copy(&a, &c).unwrap();
        assert_eq!(Disk.stamp(&c), Some((t, 1)));
        Disk.rename(&a, &d.join("e")).unwrap();
        assert!(!a.exists() && d.join("e").is_file());
        let _ = std::fs::remove_dir_all(&d);
    }

    /// A new folder is made only where there is nothing (not even a link
    /// that leads nowhere), for its owner alone.
    #[test]
    fn a_new_folder_is_this_runs_own() {
        let d = crate::testkit::tmp("new-dir");
        let stage = d.join("dl/.take-x");
        Disk.create_new_dir(&stage).unwrap();
        assert!(stage.is_dir());
        let e = Disk.create_new_dir(&stage).unwrap_err();
        assert_eq!(e.kind(), io::ErrorKind::AlreadyExists);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&stage).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o700);
            let link = d.join("dl/.take-y");
            std::os::unix::fs::symlink(d.join("elsewhere"), &link).unwrap();
            let e = Disk.create_new_dir(&link).unwrap_err();
            assert_eq!(e.kind(), io::ErrorKind::AlreadyExists);
            assert!(!d.join("elsewhere").exists());
        }
        // a store without folders of its own: none where files are
        let m = Mem::default();
        m.put(Path::new("dl/.take-x/a"), b"a", 0);
        let e = m.create_new_dir(Path::new("dl/.take-x")).unwrap_err();
        assert_eq!(e.kind(), io::ErrorKind::AlreadyExists);
        assert!(m.create_new_dir(Path::new("dl/.take-y")).is_ok());
    }

    #[test]
    fn parts_windows_holds_too() {
        for (name, safe) in [
            ("a.bms", Some("a.bms")),
            ("a.bms.", Some("a.bms")),
            ("a.bms . .", Some("a.bms")),
            (" a b ", Some(" a b")),
            ("CON", Some("_CON")),
            ("nul.wav", Some("_nul.wav")),
            ("Com1.tar.gz", Some("_Com1.tar.gz")),
            ("LPT² .txt", Some("_LPT² .txt")),
            ("aux ", Some("_aux")),
            ("COM10", Some("COM10")),
            ("console.log", Some("console.log")),
            ("a<b>c|d?e*f\"g", Some("a_b_c_d_e_f_g")),
            ("a\tb\u{1b}[2J", Some("a_b_[2J")),
            ("ソ.bms", Some("ソ.bms")),
            ("...", None),
            (". .", None),
            ("..", None),
            ("", None),
        ] {
            assert_eq!(safe_part(name).as_deref(), safe, "{name:?}");
        }
    }
}
