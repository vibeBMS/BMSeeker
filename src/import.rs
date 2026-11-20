//! Downloaded songs: archives (.zip, .7z, .rar, .lzh, .tar, .tar.gz) are
//! unpacked into the library, a chart on its own goes into its song's
//! folder (see `placement`), as [`install::take`](crate::install::take)
//! does with a song's download. On any [`Store`] (a disk, a player's files).

use crate::fs::Store;
use crate::install::plan::{taken, Outcome, Request};
use crate::install::Done;
use crate::library::Library;
use crate::placement;
use crate::unpack::{is_archive, open, safe_rel, unpack_from, Blob, Opened, BUDGET};
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
/// to it), as `req` says (where the file came from; `reinstall`; the name
/// it was published under, when the file's own is not it...): as
/// [`install::take`](crate::install::take) does, as one outcome. Archives
/// are unpacked: a song new to the library into a folder named after the
/// archive (when one of that name holds another song: `name (2)`...); a
/// song the library has all the charts of stays as it is (the files its
/// folder lacks brought in); charts for a song's folder there go into it.
/// The archive goes. What it unpacks passes through a folder beside it; a
/// chart that waits stays there. With what it is, the charts that waited
/// there for what came in, placed.
pub fn import(
    store: &dyn Store,
    file: &Path,
    lib: &mut Library,
    req: &Request,
) -> Result<(Imported, Vec<PathBuf>), String> {
    let named = req.file.as_deref().map_or(file, Path::new);
    if !is_archive(named) && !placement::is_chart(named) {
        return Ok((Imported::Other(file.to_path_buf()), Vec::new()));
    }
    let downloads = file.parent().unwrap_or(Path::new(""));
    let (out, placed) = taken(store, Blob::File(file.to_path_buf()), req, downloads, lib);
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
        return Ok((i, placed));
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

/// Put `blob` (published as `name`: a file, or its bytes in memory) into
/// the folder `dest` of `store`: an archive (by its first bytes) is
/// unpacked there (with the archives in it, if it holds no chart), anything
/// else (an add-on chart or movie) is put in as it is (a file moved there);
/// but a file named as an archive is none. Returns the number of files
/// written.
pub fn unpack_into(
    store: &dyn Store,
    blob: Blob,
    name: &str,
    dest: &Path,
) -> Result<usize, String> {
    store.create_dir_all(dest);
    let blob = match open(blob, store)? {
        Opened::Archive(format, r) => {
            return Ok(unpack_from(r, format, store, dest, BUDGET, None)?.len());
        }
        Opened::Not(_) if is_archive(Path::new(name)) => return Err("not an archive".into()),
        Opened::Not(blob) => blob,
    };
    let to = dest.join(safe_rel(name).unwrap_or_else(|| PathBuf::from("download")));
    blob.put(store, &to)
        .map_err(|e| format!("{}: {e}", to.display()))?;
    Ok(1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fs::Disk;
    use crate::testkit::{tmp, zip};

    /// [`import`] into the library below `library`, as by hand.
    fn import_to(store: &dyn Store, file: &Path, library: &Path) -> Result<Imported, String> {
        import(store, file, &mut Library::new(library), &Request::default()).map(|(i, _)| i)
    }

    /// A file or its bytes: an archive unpacked, anything else put in.
    #[test]
    fn files_and_bytes_into_a_folder() {
        use crate::fs::Fs;
        let m = crate::fs::Mem::default();
        let dest = Path::new("skins/x");
        let z = zip(&[("a/b.txt", "b"), ("c.png", "c")]);
        assert_eq!(unpack_into(&m, Blob::Bytes(z), "x.zip", dest), Ok(2));
        assert_eq!(m.read(&dest.join("a/b.txt")).unwrap(), b"b");
        assert_eq!(
            unpack_into(&m, Blob::Bytes(b"#TITLE x".to_vec()), "../x.bms", dest),
            Ok(1)
        );
        assert_eq!(m.read(&dest.join("x.bms")).unwrap(), b"#TITLE x");
        m.put(Path::new("dl/movie"), b"mp4", 0);
        let file = Blob::File("dl/movie".into());
        assert_eq!(unpack_into(&m, file, "m.mp4", dest), Ok(1));
        assert_eq!(m.read(&dest.join("m.mp4")).unwrap(), b"mp4");
        assert!(m.read(Path::new("dl/movie")).is_err());
        // an archive by its bytes, whatever its name; none named as one
        let z = zip(&[("d.txt", "d")]);
        assert_eq!(unpack_into(&m, Blob::Bytes(z), "author link", dest), Ok(1));
        assert_eq!(m.read(&dest.join("d.txt")).unwrap(), b"d");
        let page = Blob::Bytes(b"<html>".to_vec());
        assert!(unpack_into(&m, page, "x.zip", dest).is_err());
    }

    const CHART: &str = "#TITLE Song\n#ARTIST someone\n#BPM 120\n#WAV01 01.wav\n#00111:01\n";

    #[test]
    fn unpacks_a_zip_and_places_its_chart_later() {
        let root = tmp("import-zip");
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
        let files = [
            ("Song/song.bms", CHART),
            ("Song/01.ogg", "ogg"),
            ("../evil.txt", "x"),
        ];
        std::fs::write(&pack, zip(&files)).unwrap();
        let dest = library.join("Song");
        // the waiting chart finds its song
        assert_eq!(
            import(
                &Disk,
                &pack,
                &mut Library::new(&library),
                &Request::default()
            )
            .unwrap(),
            (
                Imported::Unpacked(dest.clone(), 3),
                vec![dest.join("Song/song_insane.bme")]
            )
        );
        assert!(dest.join("Song/01.ogg").is_file() && dest.join("evil.txt").is_file());
        assert!(!pack.exists() && !root.join("evil.txt").exists());
    }

    /// A .tar.gz (and a .tgz) as a zip: into a folder of its name without
    /// `.tar.gz`.
    #[test]
    fn unpacks_a_tar_gz() {
        let root = tmp("import-tar");
        let library = root.join("bms");
        std::fs::create_dir_all(&library).unwrap();
        let files = [("song.bms", CHART), ("01.ogg", "ogg")];
        let pack = root.join("Song.tar.gz");
        std::fs::write(&pack, crate::testkit::tar_gz(&files)).unwrap();
        let dest = library.join("Song");
        assert_eq!(
            import_to(&Disk, &pack, &library).unwrap(),
            Imported::Unpacked(dest.clone(), 2)
        );
        assert!(dest.join("01.ogg").is_file() && !pack.exists());
        let m = crate::fs::Mem::default();
        let t = Blob::Bytes(crate::testkit::tar(&files));
        assert_eq!(unpack_into(&m, t, "x.tgz", Path::new("d")), Ok(2));
    }

    /// Two songs of one title: the second gets a folder of its own; the
    /// same song again goes into its folder, its files there kept.
    #[test]
    fn same_title_other_song() {
        let root = tmp("import-same-title");
        let library = root.join("bms");
        std::fs::create_dir_all(&library).unwrap();
        let pack = |artist: &str, sound: &[u8]| {
            let p = root.join("Aurora.zip");
            let chart = format!("#TITLE Aurora\n#ARTIST {artist}\n#WAV01 01.wav\n");
            std::fs::write(&p, zip(&[("a.bms", chart.as_bytes()), ("01.ogg", sound)])).unwrap();
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
        // alpha's again (another copy): into its own folder, nothing there
        // replaced on its titles' word
        let got = import_to(&Disk, &pack("alpha", b"new"), &library).unwrap();
        assert_eq!(got, Imported::Unpacked(first.clone(), 2));
        assert_eq!(std::fs::read(first.join("01.ogg")).unwrap(), b"one");
        assert!(!first.join("a_.bms").exists());
        assert_eq!(std::fs::read(second.join("01.ogg")).unwrap(), b"two");
        // nothing left beside the downloads
        let left: Vec<_> = std::fs::read_dir(&root).unwrap().flatten().collect();
        assert_eq!(left.len(), 1, "{left:?}");
    }

    /// The same on a store that is no disk, the song's archive times kept.
    #[test]
    fn into_a_store_of_another_kind() {
        use crate::fs::Mem;
        let mem = Mem::default();
        let library = Path::new("bms");
        let pack = |artist: &str| {
            let at = zip::DateTime::from_date_and_time(2019, 1, 1, 0, 0, 0).unwrap();
            let chart = format!("#TITLE Aurora\n#ARTIST {artist}\n#WAV01 01.wav\n");
            let files = [("a.bms", chart.as_str()), ("sub/01.ogg", "x")];
            let z = crate::testkit::zip_at(&files, at);
            mem.put(Path::new("dl/Aurora.zip"), &z, 0);
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

    /// A download kept under a name of its keeper's making (a browser's
    /// window's): what it is, and what its song's folder is named after,
    /// are by the name it was published under.
    #[test]
    fn a_file_kept_under_another_name() {
        use crate::fs::Mem;
        let mem = Mem::default();
        let kept = Path::new("dl/page-123.part");
        let chart = "#TITLE Aurora\n#WAV01 01.wav\n";
        let z = zip(&[("a.bms", chart), ("01.ogg", "x")]);
        let lib = &mut Library::new(Path::new("bms"));
        let named = |name: &str| Request {
            file: Some(name.into()),
            ..Default::default()
        };
        // by its own name, or one that is no archive's or chart's: nothing
        // to bring in, whatever the file is
        for req in [Request::default(), named("scan.json")] {
            mem.put(kept, &z, 0);
            let got = import(&mem, kept, lib, &req).unwrap().0;
            assert_eq!(got, Imported::Other(kept.into()));
        }
        let got = import(&mem, kept, lib, &named("Aurora (ogg).zip"))
            .unwrap()
            .0;
        assert_eq!(got, Imported::Unpacked("bms/Aurora (ogg)".into(), 2));
        assert!(mem.get(kept).is_none());
        // a name Windows keeps for a device names no folder
        mem.put(
            kept,
            &zip(&[("b.bms", "#TITLE Other\n"), ("02.ogg", "x")]),
            0,
        );
        let got = import(&mem, kept, lib, &named("nul.zip")).unwrap().0;
        assert_eq!(got, Imported::Unpacked("bms/_nul".into(), 2));
        // a chart on its own, by its name
        mem.put(kept, b"#TITLE Aurora [X]\n#WAV01 01.wav\n", 0);
        let got = import(&mem, kept, lib, &named("aurora_x.bms")).unwrap().0;
        assert_eq!(
            got,
            Imported::Placed("bms/Aurora (ogg)/aurora_x.bms".into())
        );
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
        put(
            "dl/Song.zip",
            &zip(&[("song.bms", CHART), ("01.ogg", "ogg")]),
        );
        let (got, placed) = import(
            &mem,
            Path::new("dl/Song.zip"),
            &mut lib,
            &Request::default(),
        )
        .unwrap();
        assert_eq!(got, Imported::Unpacked("bms/Song".into(), 2));
        assert!(lib.owns(&crate::install::md5_hex(CHART.as_bytes())));
        assert_eq!(placed, [PathBuf::from("bms/Song/song_x.bms")]);
        assert_eq!(
            lib.folders_of(&crate::install::md5_hex(diff.as_bytes())),
            [PathBuf::from("bms/Song")]
        );
    }

    #[test]
    fn unpacks_archives_in_an_archive() {
        let root = tmp("import-nested");
        let library = root.join("bms");
        std::fs::create_dir_all(&library).unwrap();
        // a shared folder as a zip: the song's archive and a readme
        let inner = zip(&[("Song/song.bms", CHART), ("Song/01.ogg", "ogg")]);
        let pack = root.join("Folder.zip");
        let outer = zip(&[
            ("Song_ogg.zip", inner.as_slice()),
            ("readme.txt", b"hi".as_slice()),
        ]);
        std::fs::write(&pack, outer).unwrap();
        let dest = library.join("Folder");
        assert_eq!(
            import_to(&Disk, &pack, &library).unwrap(),
            Imported::Unpacked(dest.clone(), 3)
        );
        assert!(dest.join("Song_ogg/Song/song.bms").is_file());
        assert!(!dest.join("Song_ogg.zip").exists());
    }
}
