//! Downloaded songs: archives (.zip, .7z, .rar, .lzh) are unpacked into the
//! library, a chart on its own goes into its song's folder (see
//! `placement`), as [`install::take`](crate::install::take) does with a
//! song's download. On any [`Store`] (a disk, a player's files).

use crate::fs::{copy, file_name, Store};
use crate::install::plan::{taken, Outcome, Request};
use crate::install::Done;
use crate::library::Library;
use crate::placement;
use crate::unpack::{is_archive, safe_rel, unpack, Blob, BUDGET};
use std::path::{Path, PathBuf};

/// What became of a downloaded file.
#[derive(Debug, PartialEq)]
#[non_exhaustive]
pub enum Imported {
    /// Unpacked into this folder (files written).
    Unpacked(PathBuf, usize),
    /// Its charts are in the library already, in this song's folder (the
    /// files it lacked brought in).
    Owned(PathBuf),
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

/// Bring a downloaded `file` into the library `lib` (what comes in added
/// to it), as `req` says (where the file came from; `reinstall`...): as
/// [`install::take`](crate::install::take) does, as one outcome. Archives
/// are unpacked: a song new to the library into a folder named after the
/// archive (when one of that name holds another song: `name (2)`...); a
/// song the library has all the charts of stays as it is (the files its
/// folder lacks brought in); charts for a song's folder there go into it.
/// The archive goes. What it unpacks passes through a folder beside it; a
/// chart that waits stays there.
pub fn import(
    store: &dyn Store,
    file: &Path,
    lib: &mut Library,
    req: &Request,
) -> Result<Imported, String> {
    if !is_archive(file) && !placement::is_chart(file) {
        return Ok(Imported::Other(file.to_path_buf()));
    }
    let downloads = file.parent().unwrap_or(Path::new("")).to_path_buf();
    let req = Request {
        file: Some(file_name(file)),
        ..req.clone()
    };
    let out = taken(
        store,
        Blob::File(file.to_path_buf()),
        &req,
        &downloads,
        lib,
        false,
    );
    // what counts most: a song added, a chart placed, ... what failed
    let rank = |o: &Outcome, d: &Done| -> Option<(u8, Imported)> {
        Some(match (o, d) {
            (_, Done::Added(dir, n)) => (0, Imported::Unpacked(dir.clone(), *n)),
            (_, Done::Placed(p)) => (1, Imported::Placed(p.clone())),
            (_, Done::Owned(p)) => (2, Imported::Owned(p.clone())),
            (Outcome::Ask(c), Done::Waiting(p)) => (3, Imported::Ask(p.clone(), c.clone())),
            (_, Done::Waiting(p)) => (4, Imported::Waiting(p.clone())),
            _ => return None,
        })
    };
    let best = out
        .iter()
        .flat_map(|(o, done)| done.iter().filter_map(move |d| rank(o, d)))
        .min_by_key(|(r, _)| *r);
    if let Some((_, i)) = best {
        return Ok(i);
    }
    Err(out
        .iter()
        .flat_map(|(_, d)| d)
        .find_map(|d| match d {
            Done::Failed(e) => Some(e.to_string()),
            _ => None,
        })
        .unwrap_or_else(|| "nothing came of it".into()))
}

/// Put `file` (published as `name`) into folder `dest` of `store`: an
/// archive is unpacked there (with the archives in it, if it holds no
/// chart), anything else (an add-on chart or movie) is copied in. Returns
/// the number of files written.
pub fn unpack_into(
    store: &dyn Store,
    file: &Path,
    name: &str,
    dest: &Path,
) -> Result<usize, String> {
    store.create_dir_all(dest);
    if is_archive(Path::new(name)) {
        return unpack(Blob::File(file.to_path_buf()), store, dest, BUDGET).map(|m| m.len());
    }
    let to = dest.join(safe_rel(name).unwrap_or_else(|| PathBuf::from("download")));
    copy(store, file, &to).map_err(|e| format!("{}: {e}", to.display()))?;
    Ok(1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fs::Disk;
    use std::io::Write;

    /// [`import`] into the library below `library`, as by hand.
    fn import_to(store: &dyn Store, file: &Path, library: &Path) -> Result<Imported, String> {
        import(store, file, &mut Library::new(library), &Request::default())
    }

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
            import_to(&Disk, &diff, &library).unwrap(),
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
            import_to(&Disk, &pack, &library).unwrap(),
            Imported::Unpacked(dest.clone(), 3)
        );
        assert!(dest.join("Song/01.ogg").is_file() && dest.join("evil.txt").is_file());
        assert!(!pack.exists() && !root.join("evil.txt").exists());
        // now the waiting chart finds its song
        assert_eq!(
            crate::install::place_waiting(&Disk, &downloads, &mut Library::new(&library)),
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
        let got = import_to(&Disk, &pack("alpha", b"one"), &library).unwrap();
        assert_eq!(got, Imported::Unpacked(first.clone(), 2));
        let got = import_to(&Disk, &pack("beta", b"two"), &library).unwrap();
        assert_eq!(got, Imported::Unpacked(second.clone(), 2));
        assert_eq!(std::fs::read(first.join("01.ogg")).unwrap(), b"one");
        assert_eq!(std::fs::read(second.join("01.ogg")).unwrap(), b"two");
        // alpha's again (a newer copy): over its own folder
        let got = import_to(&Disk, &pack("alpha", b"new"), &library).unwrap();
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
        let got = import_to(&mem, pack("alpha"), library).unwrap();
        assert_eq!(got, Imported::Unpacked(library.join("Aurora"), 2));
        assert_eq!(
            mem.get(Path::new("bms/Aurora/sub/01.ogg")).unwrap().1,
            1_546_300_800
        );
        assert!(mem.get(Path::new("dl/Aurora.zip")).is_none());
        let got = import_to(&mem, pack("beta"), library).unwrap();
        assert_eq!(got, Imported::Unpacked(library.join("Aurora (2)"), 2));
        assert!(mem.get(Path::new("bms/Aurora (2)/sub/01.ogg")).is_some());
        // nothing left of the folder it went through
        let left: Vec<PathBuf> = mem.0.lock().unwrap().keys().cloned().collect();
        assert_eq!(left.len(), 4, "{left:?}");
    }

    /// One library for a run: a song imported into it is there for the
    /// charts that wait, and owned, without reading the library again.
    #[test]
    fn one_library_for_a_run() {
        use crate::fs::Mem;
        let mem = Mem::default();
        let put = |p: &str, b: &[u8]| mem.put(Path::new(p), b, 0);
        put("bms/Old/o.bms", b"#TITLE Old\n#WAV01 o.wav\n");
        put("bms/Old/o.ogg", b"x");
        let diff = "#TITLE Song [X]\n#ARTIST someone\n#WAV01 01.wav\n";
        put("dl/song_x.bms", diff.as_bytes());
        let mut lib = Library::new(Path::new("bms"));
        // read before the song comes
        assert!(lib.files(&mem, Path::new("bms/Old")).is_some());
        let mut out = std::io::Cursor::new(Vec::new());
        {
            let mut z = zip::ZipWriter::new(&mut out);
            let o = zip::write::SimpleFileOptions::default();
            for (name, data) in [("song.bms", CHART.as_bytes()), ("01.ogg", b"ogg")] {
                z.start_file(name, o).unwrap();
                z.write_all(data).unwrap();
            }
            z.finish().unwrap();
        }
        put("dl/Song.zip", &out.into_inner());
        let got = import(
            &mem,
            Path::new("dl/Song.zip"),
            &mut lib,
            &Request::default(),
        )
        .unwrap();
        assert_eq!(got, Imported::Unpacked("bms/Song".into(), 2));
        assert!(lib.owns(&crate::install::md5_hex(CHART.as_bytes())));
        let placed = crate::install::place_waiting(&mem, Path::new("dl"), &mut lib);
        assert_eq!(placed, [PathBuf::from("bms/Song/song_x.bms")]);
        assert_eq!(
            lib.folders_of(&crate::install::md5_hex(diff.as_bytes())),
            [PathBuf::from("bms/Song")]
        );
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
            import_to(&Disk, &pack, &library).unwrap(),
            Imported::Unpacked(dest.clone(), 3)
        );
        assert!(dest.join("Song_ogg/Song/song.bms").is_file());
        assert!(!dest.join("Song_ogg.zip").exists());
        let _ = std::fs::remove_dir_all(&root);
    }
}
