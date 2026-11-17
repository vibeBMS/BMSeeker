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

/// A store as files to read (`&dyn Store` as `&dyn Fs`).
pub trait AsFs {
    fn as_fs(&self) -> &dyn Fs;
}

impl<T: Fs> AsFs for T {
    fn as_fs(&self) -> &dyn Fs {
        self
    }
}

/// Files written as well as read: where archives are unpacked.
pub trait Store: Fs + AsFs {
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
    /// Copy the file `from` to the new file `to`, with its time: read
    /// whole and written, unless the store can do better.
    fn copy(&self, from: &Path, to: &Path) -> io::Result<()> {
        let bytes = self.read(from)?;
        let mut w = self.create(to, self.stamp(from).map(|(t, _)| t))?;
        w.write_all(&bytes)?;
        w.flush()
    }
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
        Ok(Box::new(std::fs::File::open(p)?))
    }

    fn create_dir_all(&self, p: &Path) {
        let _ = std::fs::create_dir_all(p);
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
}
