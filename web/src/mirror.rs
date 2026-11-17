//! The files a command works on in a web page: a mirror of the target's
//! (the vibeBMS library in the page's storage, a folder the user picked, or
//! nothing: a zip to come), as [`Store`]. A browser lists and writes those
//! files only asynchronously, while bmseeker's commands are synchronous: so
//! the page lists the target first ([`Mirror::mount`]: each file's path,
//! size and time; its bytes are read when a command reads it, through
//! `source`), the command runs on the mirror, and what it changed is written
//! to the target after it ([`Mirror::changes`], then [`Mirror::settle`]).

use bmseeker::fs::{Entry, Fs, Store};
use std::collections::{BTreeMap, BTreeSet};
use std::io::{self, Write};
use std::ops::Bound;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};

/// Reads a file of the target by its path there.
pub type Source = Box<dyn Fn(&Path) -> io::Result<Vec<u8>> + Send + Sync>;

#[derive(Clone, Debug, PartialEq)]
enum Data {
    /// The target's file at this path (where it was listed or last written).
    Target(PathBuf),
    /// Written by a command, not yet to the target.
    Bytes(Arc<Vec<u8>>),
}

#[derive(Clone, Debug)]
struct Node {
    data: Data,
    size: u64,
    mtime: i64,
}

#[derive(Default)]
struct Tree {
    files: BTreeMap<PathBuf, Node>,
    /// The target's files removed (their paths there).
    gone: BTreeSet<PathBuf>,
    /// Folders removed with what was below them.
    gone_dirs: BTreeSet<PathBuf>,
}

/// What to do to the target, in this order, to make it what the mirror
/// holds.
#[derive(Clone, Debug, PartialEq)]
pub enum Change {
    /// Remove the file or folder (`dir`: with everything below it).
    Remove { path: PathBuf, dir: bool },
    /// Move the target's file `from` to `to`.
    Move { from: PathBuf, to: PathBuf },
    /// Write the file.
    Write {
        path: PathBuf,
        bytes: Arc<Vec<u8>>,
        mtime: i64,
    },
}

pub struct Mirror {
    tree: Mutex<Tree>,
    source: Source,
    /// The folders the commands may change (the vibeBMS library: `/bms`
    /// and `/bmseeker`, never its player's own files).
    writable: Vec<PathBuf>,
    /// Files keep the times they are written with (a zip does; the
    /// browser's storage takes the time of writing).
    keeps_times: bool,
    /// Now, in seconds (a file written without a time of its own).
    now: fn() -> i64,
}

fn not_found(p: &Path) -> io::Error {
    io::Error::new(io::ErrorKind::NotFound, p.display().to_string())
}

/// The files at and below `p`: paths order by component, so they are one
/// run of the map.
fn below<'a>(
    files: &'a BTreeMap<PathBuf, Node>,
    p: &'a Path,
) -> impl Iterator<Item = (&'a PathBuf, &'a Node)> {
    files
        .range::<Path, _>((Bound::Included(p), Bound::Unbounded))
        .take_while(move |(k, _)| k.starts_with(p))
}

impl Mirror {
    pub fn new(
        source: Source,
        writable: Vec<PathBuf>,
        keeps_times: bool,
        now: fn() -> i64,
    ) -> Mirror {
        Mirror {
            tree: Mutex::default(),
            source,
            writable,
            keeps_times,
            now,
        }
    }

    fn tree(&self) -> MutexGuard<'_, Tree> {
        self.tree.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn check(&self, p: &Path) -> io::Result<()> {
        if self.writable.iter().any(|w| p.starts_with(w)) {
            Ok(())
        } else {
            Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                format!("{}: not BMSeeker's to change", p.display()),
            ))
        }
    }

    /// The target's file `p` (as listed): its size and time.
    pub fn mount(&self, p: PathBuf, size: u64, mtime: i64) {
        let node = Node {
            data: Data::Target(p.clone()),
            size,
            mtime,
        };
        self.tree().files.insert(p, node);
    }

    /// Every file, with its size.
    pub fn files(&self) -> Vec<(PathBuf, u64)> {
        let t = self.tree();
        t.files.iter().map(|(p, n)| (p.clone(), n.size)).collect()
    }

    /// What was changed since the target was last written, in the order to
    /// do it: files removed (but those below a folder removed whole, which
    /// goes last), moved, written.
    pub fn changes(&self) -> Vec<Change> {
        let t = self.tree();
        // a folder removed whole unless something is in it again
        let whole: Vec<&PathBuf> = t
            .gone_dirs
            .iter()
            .filter(|d| below(&t.files, d).next().is_none())
            .collect();
        let mut out: Vec<Change> = t
            .gone
            .iter()
            .filter(|p| !whole.iter().any(|d| p.starts_with(d)))
            .map(|p| Change::Remove {
                path: p.clone(),
                dir: false,
            })
            .collect();
        for (p, n) in &t.files {
            if let Data::Target(from) = &n.data {
                if from != p {
                    out.push(Change::Move {
                        from: from.clone(),
                        to: p.clone(),
                    });
                }
            }
        }
        for (p, n) in &t.files {
            if let Data::Bytes(b) = &n.data {
                out.push(Change::Write {
                    path: p.clone(),
                    bytes: b.clone(),
                    mtime: n.mtime,
                });
            }
        }
        out.extend(
            whole
                .into_iter()
                // (a folder within another removed whole goes with it)
                .filter(|d| !t.gone_dirs.iter().any(|o| o != *d && d.starts_with(o)))
                .map(|d| Change::Remove {
                    path: d.clone(),
                    dir: true,
                }),
        );
        out
    }

    /// The changes were made to the target: `written` are there now (with
    /// the size and time the target gives them), `failed` are not (they
    /// are forgotten here: the target is what counts).
    pub fn settle(&self, written: &[(PathBuf, u64, i64)], failed: &[PathBuf]) {
        let mut t = self.tree();
        for (p, size, mtime) in written {
            if let Some(n) = t.files.get_mut(p) {
                *n = Node {
                    data: Data::Target(p.clone()),
                    size: *size,
                    mtime: *mtime,
                };
            }
        }
        for p in failed {
            t.files.remove(p);
        }
        t.gone.clear();
        t.gone_dirs.clear();
    }

    /// Every file's bytes (`skip`: but those below it), with its time: what
    /// a zip of the mirror holds.
    pub fn contents(&self, skip: &Path) -> Vec<(PathBuf, io::Result<Vec<u8>>, i64)> {
        let nodes: Vec<(PathBuf, Node)> = self
            .tree()
            .files
            .iter()
            .filter(|(p, _)| !p.starts_with(skip))
            .map(|(p, n)| (p.clone(), n.clone()))
            .collect();
        nodes
            .into_iter()
            .map(|(p, n)| {
                let bytes = self.bytes(&n);
                (p, bytes, n.mtime)
            })
            .collect()
    }

    fn bytes(&self, n: &Node) -> io::Result<Vec<u8>> {
        match &n.data {
            Data::Bytes(b) => Ok(b.as_ref().clone()),
            Data::Target(at) => (self.source)(at),
        }
    }

    /// Put `node` at `p`: a target's file moved there before is not
    /// moved after all (it goes).
    fn put(t: &mut Tree, p: PathBuf, node: Node) {
        if let Some(Node {
            data: Data::Target(from),
            ..
        }) = t.files.insert(p.clone(), node)
        {
            if from != p {
                t.gone.insert(from);
            }
        }
    }
}

impl Fs for Mirror {
    fn read(&self, p: &Path) -> io::Result<Vec<u8>> {
        let node = self.tree().files.get(p).cloned();
        // (the lock let go of while the target's file is read)
        self.bytes(&node.ok_or_else(|| not_found(p))?)
    }

    fn read_dir(&self, p: &Path) -> io::Result<Vec<Entry>> {
        let t = self.tree();
        let mut out: Vec<Entry> = Vec::new();
        for (k, _) in below(&t.files, p) {
            let Ok(rest) = k.strip_prefix(p) else {
                continue;
            };
            let mut comps = rest.components();
            let Some(first) = comps.next() else { continue };
            // a folder's files are one run too
            if out.last().is_some_and(|e| e.path.ends_with(first)) {
                continue;
            }
            out.push(Entry {
                path: p.join(first),
                is_dir: comps.next().is_some(),
            });
        }
        if out.is_empty() {
            return Err(not_found(p));
        }
        Ok(out)
    }

    fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
        self.check(from)?;
        self.check(to)?;
        let mut t = self.tree();
        if t.files.contains_key(to) {
            return Err(io::ErrorKind::AlreadyExists.into());
        }
        let n = t.files.remove(from).ok_or_else(|| not_found(from))?;
        t.files.insert(to.to_path_buf(), n);
        Ok(())
    }

    fn stamp(&self, p: &Path) -> Option<(i64, u64)> {
        self.tree().files.get(p).map(|n| (n.mtime, n.size))
    }

    fn remove(&self, p: &Path) -> io::Result<()> {
        self.check(p)?;
        let mut t = self.tree();
        match t.files.remove(p).ok_or_else(|| not_found(p))?.data {
            Data::Target(from) => {
                t.gone.insert(from);
            }
            Data::Bytes(_) => {}
        }
        Ok(())
    }

    fn keeps_times(&self) -> bool {
        self.keeps_times
    }
}

/// A file being written: in the mirror once done.
struct Writing<'a> {
    mirror: &'a Mirror,
    p: PathBuf,
    buf: Vec<u8>,
    mtime: i64,
}

impl Write for Writing<'_> {
    fn write(&mut self, b: &[u8]) -> io::Result<usize> {
        self.buf.extend_from_slice(b);
        Ok(b.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl Drop for Writing<'_> {
    fn drop(&mut self) {
        let bytes = std::mem::take(&mut self.buf);
        let node = Node {
            size: bytes.len() as u64,
            data: Data::Bytes(Arc::new(bytes)),
            mtime: self.mtime,
        };
        Mirror::put(&mut self.mirror.tree(), std::mem::take(&mut self.p), node);
    }
}

impl Store for Mirror {
    fn create(&self, p: &Path, mtime: Option<i64>) -> io::Result<Box<dyn Write + '_>> {
        self.check(p)?;
        let mtime = match mtime {
            Some(t) if self.keeps_times => t,
            _ => (self.now)(),
        };
        Ok(Box::new(Writing {
            mirror: self,
            p: p.to_path_buf(),
            buf: Vec::new(),
            mtime,
        }))
    }

    fn remove_dir(&self, p: &Path) {
        if self.check(p).is_err() {
            return;
        }
        let mut t = self.tree();
        let inside: Vec<PathBuf> = below(&t.files, p).map(|(k, _)| k.clone()).collect();
        for k in inside {
            if let Some(Node {
                data: Data::Target(from),
                ..
            }) = t.files.remove(&k)
            {
                t.gone.insert(from);
            }
        }
        t.gone_dirs.insert(p.to_path_buf());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A target of two files, read through `source`.
    fn mirror() -> Mirror {
        let source: Source = Box::new(|p: &Path| match p.to_str() {
            Some("/bms/a/x.bms") => Ok(b"#TITLE x".to_vec()),
            Some("/bms/a/1.ogg") => Ok(b"ogg".to_vec()),
            _ => Err(io::ErrorKind::NotFound.into()),
        });
        let m = Mirror::new(source, vec!["/bms".into(), "/bmseeker".into()], false, || 7);
        m.mount("/bms/a/x.bms".into(), 8, 1);
        m.mount("/bms/a/1.ogg".into(), 3, 2);
        m.mount("/store/ir.json".into(), 2, 3);
        m
    }

    fn write(m: &Mirror, p: &str, body: &[u8]) {
        let mut w = m.create(Path::new(p), Some(100)).unwrap();
        w.write_all(body).unwrap();
    }

    #[test]
    fn reads_the_target_and_what_was_written() {
        let m = mirror();
        assert_eq!(m.read(Path::new("/bms/a/x.bms")).unwrap(), b"#TITLE x");
        assert_eq!(m.stamp(Path::new("/bms/a/1.ogg")), Some((2, 3)));
        let dir = m.read_dir(Path::new("/bms")).unwrap();
        assert_eq!(dir.len(), 1);
        assert!(dir[0].is_dir && dir[0].path == Path::new("/bms/a"));
        assert_eq!(m.read_dir(Path::new("/bms/a")).unwrap().len(), 2);
        assert!(m.read_dir(Path::new("/bms/b")).is_err());
        write(&m, "/bms/b/y.bms", b"y");
        // the browser's storage takes the time of writing
        assert_eq!(m.stamp(Path::new("/bms/b/y.bms")), Some((7, 1)));
        assert_eq!(m.read(Path::new("/bms/b/y.bms")).unwrap(), b"y");
        assert!(!m.keeps_times());
    }

    #[test]
    fn changes_only_what_is_its_own() {
        let m = mirror();
        let denied = |r: io::Result<()>| r.unwrap_err().kind() == io::ErrorKind::PermissionDenied;
        assert!(denied(m.remove(Path::new("/store/ir.json"))));
        assert!(denied(
            m.rename(Path::new("/store/ir.json"), Path::new("/bms/ir.json"))
        ));
        assert!(m.create(Path::new("/store/x"), None).is_err());
        m.remove_dir(Path::new("/store"));
        assert!(m.changes().is_empty());
        assert!(m.stamp(Path::new("/store/ir.json")).is_some());
    }

    #[test]
    fn changes_in_order() {
        let m = mirror();
        // a download staged, then moved into the library
        write(&m, "/bmseeker/downloads/s/2.ogg", b"22");
        m.rename(
            Path::new("/bmseeker/downloads/s/2.ogg"),
            Path::new("/bms/a/2.ogg"),
        )
        .unwrap();
        m.remove_dir(Path::new("/bmseeker/downloads/s"));
        // a file of the target's moved, another replaced
        m.rename(Path::new("/bms/a/x.bms"), Path::new("/bms/c/x.bms"))
            .unwrap();
        assert_eq!(
            m.rename(Path::new("/bms/a/1.ogg"), Path::new("/bms/c/x.bms"))
                .unwrap_err()
                .kind(),
            io::ErrorKind::AlreadyExists
        );
        write(&m, "/bms/a/1.ogg", b"new");
        assert_eq!(m.read(Path::new("/bms/c/x.bms")).unwrap(), b"#TITLE x");
        let got = m.changes();
        let short: Vec<String> = got
            .iter()
            .map(|c| match c {
                Change::Remove { path, dir } => {
                    format!("rm{} {}", if *dir { " -r" } else { "" }, path.display())
                }
                Change::Move { from, to } => format!("mv {} {}", from.display(), to.display()),
                Change::Write { path, bytes, .. } => {
                    format!("write {} {}", path.display(), bytes.len())
                }
            })
            .collect();
        assert_eq!(
            short,
            [
                "mv /bms/a/x.bms /bms/c/x.bms",
                "write /bms/a/1.ogg 3",
                "write /bms/a/2.ogg 2",
                "rm -r /bmseeker/downloads/s",
            ]
        );
        m.settle(
            &[
                ("/bms/a/1.ogg".into(), 3, 50),
                ("/bms/c/x.bms".into(), 8, 1),
            ],
            &["/bms/a/2.ogg".into()],
        );
        assert!(m.changes().is_empty());
        assert_eq!(m.stamp(Path::new("/bms/a/1.ogg")), Some((50, 3)));
        assert_eq!(m.stamp(Path::new("/bms/a/2.ogg")), None);
    }

    #[test]
    fn a_folder_removed_and_written_again() {
        let m = mirror();
        m.remove_dir(Path::new("/bms/a"));
        assert!(m.read_dir(Path::new("/bms/a")).is_err());
        write(&m, "/bms/a/x.bms", b"again");
        // its other file goes on its own, the folder stays
        assert_eq!(
            m.changes(),
            [
                Change::Remove {
                    path: "/bms/a/1.ogg".into(),
                    dir: false
                },
                Change::Remove {
                    path: "/bms/a/x.bms".into(),
                    dir: false
                },
                Change::Write {
                    path: "/bms/a/x.bms".into(),
                    bytes: Arc::new(b"again".to_vec()),
                    mtime: 7
                },
            ]
        );
    }

    #[test]
    fn a_zip_keeps_times() {
        let source: Source = Box::new(|_| Err(io::ErrorKind::NotFound.into()));
        let m = Mirror::new(source, vec!["/".into()], true, || 7);
        write(&m, "/s/a.bms", b"a");
        write(&m, "/.bmseeker/scan.json", b"[]");
        assert_eq!(m.stamp(Path::new("/s/a.bms")), Some((100, 1)));
        let got = m.contents(Path::new("/.bmseeker"));
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].0, Path::new("/s/a.bms"));
        assert_eq!(got[0].1.as_ref().unwrap(), b"a");
    }
}
