//! Downloaded songs: archives (.zip, .7z, .rar, .lzh) are unpacked into the
//! library, a chart on its own goes into its song's folder (see
//! `placement`). On any [`Store`] (a disk, a player's files).

use crate::fs::{Disk, Store};
use crate::placement::{self, Located};
use crate::unpack::{safe_rel, unpack, Blob, BUDGET};
use std::path::{Path, PathBuf};

/// What became of a downloaded file.
#[derive(Debug, PartialEq)]
pub enum Imported {
    /// Unpacked into this folder (files written).
    Unpacked(PathBuf, usize),
    /// A chart moved into its song's folder.
    Placed(PathBuf),
    /// A chart whose song is not in the library yet (it waits in the
    /// downloads folder and is placed when the song arrives).
    Waiting(PathBuf),
    /// A chart with several possible songs: the user picks one.
    Ask(PathBuf, Vec<(PathBuf, String)>),
    /// Not a song (kept in the downloads folder).
    Other(PathBuf),
}

fn ext(p: &Path) -> String {
    p.extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .unwrap_or_default()
}

fn is_archive(p: &Path) -> bool {
    matches!(ext(p).as_str(), "zip" | "7z" | "rar" | "lzh" | "lha")
}

/// Bring a downloaded `file` into `library` (`<root>/bms`). Archives are
/// unpacked into a folder named after them (when one of that name holds
/// another song: `name (2)`...) and removed.
pub fn import(fs: &dyn Store, file: &Path, library: &Path) -> Result<Imported, String> {
    if is_archive(file) {
        let stem = file
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "download".into());
        let unpack_to = |dest: &Path| -> Result<usize, String> {
            unpack(Blob::File(file.to_path_buf()), fs, dest, BUDGET)
                .map(|m| m.len())
                .inspect_err(|_| fs.remove_dir(dest))
        };
        let first = library.join(&stem);
        if fs.read_dir(&first).is_err() {
            let n = unpack_to(&first)?;
            fs.remove(file);
            return Ok(Imported::Unpacked(first, n));
        }
        // a folder of that name: unpacked beside the file first, then into
        // it (the same song again) or a free name (another song)
        let tmp = file.with_file_name(format!(".unpack-{}", crate::install::unique()));
        let n = unpack_to(&tmp)?;
        fs.remove(file);
        let read = fs.as_fs();
        let dest =
            placement::song_folder(read, library, &stem, &placement::charts_below(read, &tmp));
        let moved = move_tree(fs, &tmp, &dest);
        fs.remove_dir(&tmp);
        moved?;
        return Ok(Imported::Unpacked(dest, n));
    }
    if !placement::is_chart(file) {
        return Ok(Imported::Other(file.to_path_buf()));
    }
    let read = fs.as_fs();
    Ok(match placement::locate_loose(read, file, library) {
        Some(Located::Move(dir)) => match placement::bring(read, file, &dir) {
            Some(to) => Imported::Placed(to),
            None => Imported::Waiting(file.to_path_buf()),
        },
        Some(Located::Ask(c)) => Imported::Ask(file.to_path_buf(), c),
        _ => Imported::Waiting(file.to_path_buf()),
    })
}

/// Move everything below `from` into `to` (a file of the same name there
/// replaced).
fn move_tree(fs: &dyn Store, from: &Path, to: &Path) -> Result<(), String> {
    if fs.read_dir(to).is_err() && fs.rename(from, to) {
        return Ok(());
    }
    let mut todo = vec![from.to_path_buf()];
    while let Some(d) = todo.pop() {
        for e in fs.read_dir(&d).map_err(|e| e.to_string())? {
            if e.is_dir {
                todo.push(e.path);
                continue;
            }
            let rel = e.path.strip_prefix(from).map_err(|e| e.to_string())?;
            let at = to.join(rel);
            fs.remove(&at);
            if !fs.rename(&e.path, &at) {
                // (a folder not there yet) copied, with its time
                copy(fs, &e.path, &at).map_err(|err| format!("{}: {err}", at.display()))?;
            }
        }
    }
    Ok(())
}

/// Copy the file `from` to `to` in `fs` (its folders made), keeping its time.
fn copy(fs: &dyn Store, from: &Path, to: &Path) -> std::io::Result<()> {
    let bytes = fs.read(from)?;
    let mtime = fs.stamp(from).map(|(t, _)| t);
    let mut w = fs.create(to, mtime)?;
    w.write_all(&bytes)?;
    w.flush()
}

use std::io::Write;

/// Put `file` (published as `name`) into folder `dest` on the disk: an
/// archive is unpacked there (with the archives in it, if it holds no
/// chart), anything else (an add-on chart or movie) is copied in. Returns
/// the number of files written.
pub fn unpack_into(file: &Path, name: &str, dest: &Path) -> Result<usize, String> {
    std::fs::create_dir_all(dest).map_err(|e| format!("{}: {e}", dest.display()))?;
    unpack_into_store(&Disk, file, name, dest)
}

/// [`unpack_into`] on any store.
pub fn unpack_into_store(
    fs: &dyn Store,
    file: &Path,
    name: &str,
    dest: &Path,
) -> Result<usize, String> {
    if is_archive(Path::new(name)) {
        return unpack(Blob::File(file.to_path_buf()), fs, dest, BUDGET).map(|m| m.len());
    }
    let to = dest.join(safe_rel(name).unwrap_or_else(|| PathBuf::from("download")));
    copy(fs, file, &to).map_err(|e| format!("{}: {e}", to.display()))?;
    Ok(1)
}

pub fn file_name(p: &Path) -> String {
    p.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

pub use crate::install::place_waiting;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fs::Disk;
    use std::io::Write;

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("vibebms-import-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    const CHART: &str = "#TITLE Song\n#ARTIST someone\n#BPM 120\n#WAV01 01.wav\n#00111:01\n";

    #[test]
    fn unpacks_a_zip_and_places_its_chart_later() {
        let root = tmp("zip");
        let (library, downloads) = (root.join("bms"), root.join("downloads"));
        std::fs::create_dir_all(&library).unwrap();
        std::fs::create_dir_all(&downloads).unwrap();
        // a 差分 chart that comes before its song waits
        let diff = downloads.join("song_insane.bme");
        std::fs::write(&diff, CHART.replace("#TITLE Song", "#TITLE Song [INSANE]")).unwrap();
        assert_eq!(
            import(&Disk, &diff, &library).unwrap(),
            Imported::Waiting(diff.clone())
        );
        // the song, zipped with a folder inside and an escaping name
        let pack = downloads.join("Song.zip");
        let mut z = zip::ZipWriter::new(std::fs::File::create(&pack).unwrap());
        let opts = zip::write::SimpleFileOptions::default();
        for (name, data) in [
            ("Song/song.bms", CHART.as_bytes()),
            ("Song/01.ogg", b"ogg".as_slice()),
            ("../evil.txt", b"x".as_slice()),
        ] {
            z.start_file(name, opts).unwrap();
            z.write_all(data).unwrap();
        }
        z.finish().unwrap();
        let dest = library.join("Song");
        assert_eq!(
            import(&Disk, &pack, &library).unwrap(),
            Imported::Unpacked(dest.clone(), 3)
        );
        assert!(dest.join("Song/01.ogg").is_file() && dest.join("evil.txt").is_file());
        assert!(!pack.exists() && !root.join("evil.txt").exists());
        // now the waiting chart finds its song
        assert_eq!(
            place_waiting(&Disk, &downloads, &library),
            vec![dest.join("Song/song_insane.bme")]
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    /// Two songs of one title: the second gets a folder of its own; the
    /// same song again goes into its folder.
    #[test]
    fn same_title_other_song() {
        let root = tmp("same-title");
        let library = root.join("bms");
        std::fs::create_dir_all(&library).unwrap();
        let pack = |artist: &str, sound: &[u8]| {
            let p = root.join("Aurora.zip");
            let mut z = zip::ZipWriter::new(std::fs::File::create(&p).unwrap());
            let opts = zip::write::SimpleFileOptions::default();
            let chart = format!("#TITLE Aurora\n#ARTIST {artist}\n#WAV01 01.wav\n");
            for (name, data) in [("a.bms", chart.as_bytes()), ("01.ogg", sound)] {
                z.start_file(name, opts).unwrap();
                z.write_all(data).unwrap();
            }
            z.finish().unwrap();
            p
        };
        let first = library.join("Aurora");
        let second = library.join("Aurora (2)");
        let got = import(&Disk, &pack("alpha", b"one"), &library).unwrap();
        assert_eq!(got, Imported::Unpacked(first.clone(), 2));
        let got = import(&Disk, &pack("beta", b"two"), &library).unwrap();
        assert_eq!(got, Imported::Unpacked(second.clone(), 2));
        assert_eq!(std::fs::read(first.join("01.ogg")).unwrap(), b"one");
        assert_eq!(std::fs::read(second.join("01.ogg")).unwrap(), b"two");
        // alpha's again (a newer copy): over its own folder
        let got = import(&Disk, &pack("alpha", b"new"), &library).unwrap();
        assert_eq!(got, Imported::Unpacked(first.clone(), 2));
        assert_eq!(std::fs::read(first.join("01.ogg")).unwrap(), b"new");
        assert_eq!(std::fs::read(second.join("01.ogg")).unwrap(), b"two");
        // nothing left beside the downloads
        let left: Vec<_> = std::fs::read_dir(&root).unwrap().flatten().collect();
        assert_eq!(left.len(), 1, "{left:?}");
        let _ = std::fs::remove_dir_all(&root);
    }

    /// The same on a store that is no disk, the song's archive times kept.
    #[test]
    fn into_a_store_of_another_kind() {
        use crate::fs::Mem;
        let mem = Mem::default();
        let library = Path::new("bms");
        let pack = |artist: &str| {
            let mut out = std::io::Cursor::new(Vec::new());
            let mut z = zip::ZipWriter::new(&mut out);
            let at = zip::DateTime::from_date_and_time(2019, 1, 1, 0, 0, 0).unwrap();
            let opts = zip::write::SimpleFileOptions::default().last_modified_time(at);
            let chart = format!("#TITLE Aurora\n#ARTIST {artist}\n#WAV01 01.wav\n");
            for (name, data) in [("a.bms", chart.as_bytes()), ("sub/01.ogg", b"x")] {
                z.start_file(name, opts).unwrap();
                z.write_all(data).unwrap();
            }
            z.finish().unwrap();
            mem.put(Path::new("dl/Aurora.zip"), &out.into_inner(), 0);
            Path::new("dl/Aurora.zip")
        };
        let got = import(&mem, pack("alpha"), library).unwrap();
        assert_eq!(got, Imported::Unpacked(library.join("Aurora"), 2));
        assert_eq!(
            mem.get(Path::new("bms/Aurora/sub/01.ogg")).unwrap().1,
            1_546_300_800
        );
        assert!(mem.get(Path::new("dl/Aurora.zip")).is_none());
        let got = import(&mem, pack("beta"), library).unwrap();
        assert_eq!(got, Imported::Unpacked(library.join("Aurora (2)"), 2));
        assert!(mem.get(Path::new("bms/Aurora (2)/sub/01.ogg")).is_some());
        // nothing left of the folder it went through
        let left: Vec<PathBuf> = mem.0.lock().unwrap().keys().cloned().collect();
        assert_eq!(left.len(), 4, "{left:?}");
    }

    #[test]
    fn unpacks_archives_in_an_archive() {
        let root = tmp("nested");
        let library = root.join("bms");
        std::fs::create_dir_all(&library).unwrap();
        let zip = |entries: &[(&str, &[u8])]| {
            let mut out = std::io::Cursor::new(Vec::new());
            let mut z = zip::ZipWriter::new(&mut out);
            for (name, data) in entries {
                z.start_file(*name, zip::write::SimpleFileOptions::default())
                    .unwrap();
                z.write_all(data).unwrap();
            }
            z.finish().unwrap();
            out.into_inner()
        };
        // a shared folder as a zip: the song's archive and a readme
        let inner = zip(&[("Song/song.bms", CHART.as_bytes()), ("Song/01.ogg", b"ogg")]);
        let pack = root.join("Folder.zip");
        std::fs::write(
            &pack,
            zip(&[("Song_ogg.zip", &inner), ("readme.txt", b"hi")]),
        )
        .unwrap();
        let dest = library.join("Folder");
        assert_eq!(
            import(&Disk, &pack, &library).unwrap(),
            Imported::Unpacked(dest.clone(), 3)
        );
        assert!(dest.join("Song_ogg/Song/song.bms").is_file());
        assert!(!dest.join("Song_ogg.zip").exists());
        let _ = std::fs::remove_dir_all(&root);
    }
}
