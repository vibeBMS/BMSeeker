use super::*;
use crate::fs::Mem;
use std::io;

/// A zip of `files` (name, contents), each of the start of `year`.
fn zip(files: &[(&str, &str)], year: u16) -> Vec<u8> {
    let at = zip::DateTime::from_date_and_time(year, 1, 1, 0, 0, 0).unwrap();
    crate::testkit::zip_at(files, at)
}

/// A store holding `files` (path, contents), of the time 1000.
fn store(files: &[(&str, &str)]) -> Mem {
    let m = Mem::default();
    for (p, b) in files {
        m.put(Path::new(p), b.as_bytes(), 1000);
    }
    m
}

fn text(m: &Mem, p: &str) -> Option<String> {
    m.get(Path::new(p))
        .map(|(b, _)| String::from_utf8(b).unwrap())
}

/// Nothing left of what passed through.
fn no_stage(m: &Mem) {
    let left: Vec<PathBuf> = m.0.lock().unwrap().keys().cloned().collect();
    assert!(
        !left.iter().any(|p| p.to_string_lossy().contains(".take-")),
        "{left:?}"
    );
}

const SONG: &str = "#TITLE Song\n#ARTIST alpha\n#WAV01 sound/01.wav\n";
const DL: &str = "dl";

fn lib(m: &Mem) -> Library {
    Library::scan(m, Path::new("bms"))
}

fn req(role: Role) -> Request {
    Request {
        role,
        ..Default::default()
    }
}

fn take_zip(m: &Mem, files: &[(&str, &str)], r: &Request, lib: &mut Library) -> Vec<Done> {
    take(m, Blob::Bytes(zip(files, 2019)), r, Path::new(DL), lib)
}

/// A 差分 package goes into its song's folder as it is laid out: its
/// sounds in `sound/` there, not beside the chart.
#[test]
fn a_packages_layout_kept() {
    let m = store(&[("bms/Song/song.bms", SONG), ("bms/Song/sound/01.ogg", "x")]);
    let chart =
        "#TITLE Song [X]\n#ARTIST alpha / obj: b\n#WAV01 sound/01.wav\n#WAV02 sound/x.wav\n";
    let files = [("Diff/song_x.bms", chart), ("Diff/sound/x.ogg", "new")];
    let done = take_zip(&m, &files, &req(Role::Diff), &mut lib(&m));
    assert!(
        matches!(&done[..], [Done::Placed(p)] if p == Path::new("bms/Song/song_x.bms")),
        "{done:?}"
    );
    assert_eq!(text(&m, "bms/Song/sound/x.ogg").as_deref(), Some("new"));
    assert!(text(&m, "bms/Song/x.ogg").is_none());
    no_stage(&m);
}

/// The audit's defect 3, in part: a song's link that brings only charts
/// for a song the library has puts them into its folder, not a new one;
/// one whose song is not there is a folder of its own (as before).
#[test]
fn a_songs_link_bringing_charts_only() {
    let m = store(&[("bms/Song/song.bms", SONG), ("bms/Song/sound/01.ogg", "x")]);
    let chart = "#TITLE Song [X]\n#ARTIST alpha\n#WAV01 sound/01.wav\n";
    let mut l = lib(&m);
    let done = take_zip(&m, &[("x.bms", chart)], &req(Role::Song), &mut l);
    assert!(
        matches!(&done[..], [Done::Placed(p)] if p == Path::new("bms/Song/x.bms")),
        "{done:?}"
    );
    let other = "#TITLE Other [X]\n#ARTIST gamma\n#WAV01 o.wav\n";
    let r = Request {
        naming: Naming::Named("Other".into()),
        ..req(Role::Song)
    };
    let done = take_zip(&m, &[("o.bms", other)], &r, &mut l);
    assert!(
        matches!(&done[..], [Done::Added(p, 1)] if p == Path::new("bms/Other")),
        "{done:?}"
    );
}

/// The library's charts in a package vote for their folder: the new chart
/// goes there, though its title says nothing.
#[test]
fn owned_charts_vote() {
    let a = "#TITLE A\n#ARTIST x\n#WAV01 01.wav\n";
    let new = "#TITLE Something else\n#ARTIST y\n#WAV01 01.wav\n#WAV02 extra.wav\n";
    let m = store(&[("bms/A/a.bms", a), ("bms/A/01.ogg", "x")]);
    let files = [("P/a.bms", a), ("P/new.bms", new), ("P/extra.ogg", "e")];
    let done = take_zip(&m, &files, &req(Role::Any), &mut lib(&m));
    assert!(
        matches!(&done[..], [Done::Placed(p)] if p == Path::new("bms/A/new.bms")),
        "{done:?}"
    );
    // the library's own chart is not brought again
    assert!(text(&m, "bms/A/a_.bms").is_none());
    assert_eq!(text(&m, "bms/A/extra.ogg").as_deref(), Some("e"));
}

/// Two folders hold the chart (the same MD5 twice): no folder is picked;
/// the new chart waits, the user asked.
#[test]
fn a_tie_is_asked() {
    let a = "#TITLE A\n#ARTIST x\n#WAV01 01.wav\n";
    let new = "#TITLE Something else\n#ARTIST y\n#WAV01 01.wav\n#WAV02 extra.wav\n";
    let m = store(&[
        ("bms/A/a.bms", a),
        ("bms/A/01.ogg", "x"),
        ("bms/B/a.bms", a),
        ("bms/B/01.ogg", "x"),
    ]);
    let l = lib(&m);
    let s = staged(
        &m,
        &[("P/a.bms", a), ("P/new.bms", new), ("P/extra.ogg", "e")],
    );
    let items = plan(&m, &s, &l, &req(Role::Diff));
    assert!(
        matches!(&items[..], [Item { outcome: Outcome::Ask(c), .. }]
            if c.iter().map(|(d, _)| d.as_path()).eq([Path::new("bms/A"), Path::new("bms/B")])),
        "{items:?}"
    );
    let done = apply(&m, items, Path::new(DL), &mut lib(&m));
    assert!(
        matches!(&done[..], [Done::Waiting(p)] if p == Path::new("dl/new.bms")),
        "{done:?}"
    );
    assert!(text(&m, "bms/A/new.bms").is_none() && text(&m, "bms/B/new.bms").is_none());
}

/// A download staged in `m` (below `dl/.take-test`).
fn staged(m: &Mem, files: &[(&str, &str)]) -> Staged {
    let root = Path::new("dl/.take-test");
    let members = files
        .iter()
        .map(|(p, b)| {
            m.put(&root.join(p), b.as_bytes(), 2000);
            Member {
                rel: PathBuf::from(p),
                size: b.len() as u64,
                mtime: Some(2000),
            }
        })
        .collect();
    Staged::read(m, root, members, false)
}

/// The song's own chart a table names: where a separate chart goes that
/// says nothing of its song.
#[test]
fn the_tables_org_md5() {
    let m = store(&[
        ("bms/Song/song.bms", SONG),
        ("bms/Song/sound/01.ogg", "x"),
        ("bms/Other/o.bms", "#TITLE Elsewhere\n"),
    ]);
    let chart = "#TITLE Remix [X]\n#ARTIST someone else\n#WAV01 sound/01.wav\n";
    let l = lib(&m);
    let s = staged(&m, &[("r.bms", chart)]);
    let items = plan(&m, &s, &l, &req(Role::Diff));
    assert!(
        matches!(
            &items[..],
            [Item {
                outcome: Outcome::Ask(_),
                ..
            }]
        ),
        "{items:?}"
    );
    let r = Request {
        org_md5: vec![md5_hex(SONG.as_bytes()).to_uppercase()],
        ..req(Role::Diff)
    };
    let items = plan(&m, &s, &l, &r);
    assert!(
        matches!(&items[..], [Item { outcome: Outcome::Into(d), .. }] if d == Path::new("bms/Song")),
        "{items:?}"
    );
    // a hint whose folder lacks the chart's sounds is not taken
    let r = Request {
        org_md5: vec![md5_hex(b"#TITLE Elsewhere\n")],
        ..req(Role::Diff)
    };
    assert!(matches!(
        &plan(&m, &s, &l, &r)[..],
        [Item {
            outcome: Outcome::Ask(_),
            ..
        }]
    ));
}

/// A song the library has: skipped but for the files its folder lacks
/// (what is there stays); reinstalled: newer files replace those there.
#[test]
fn owned_skipped_or_reinstalled() {
    let files = [
        ("Song/song.bms", SONG),
        ("Song/sound/01.ogg", "archive's"),
        ("Song/bg.png", "img"),
    ];
    let m = store(&[]);
    let mut l = Library::new(Path::new("bms"));
    let r = Request {
        naming: Naming::Named("Song".into()),
        ..req(Role::Song)
    };
    let done = take_zip(&m, &files, &r, &mut l);
    assert!(matches!(&done[..], [Done::Added(_, 3)]), "{done:?}");
    // its folder changed since: a file gone, another of its own
    m.remove(Path::new("bms/Song/Song/bg.png")).unwrap();
    m.put(Path::new("bms/Song/Song/sound/01.ogg"), b"mine", 1000);
    let done = take_zip(&m, &files, &r, &mut lib(&m));
    assert!(
        matches!(&done[..], [Done::Owned(d)] if d == Path::new("bms/Song/Song")),
        "{done:?}"
    );
    assert_eq!(text(&m, "bms/Song/Song/bg.png").as_deref(), Some("img"));
    assert_eq!(
        text(&m, "bms/Song/Song/sound/01.ogg").as_deref(),
        Some("mine")
    );
    assert!(text(&m, "bms/Song (2)/Song/song.bms").is_none());
    // again: the archive's (newer) over it
    let again = Request {
        reinstall: true,
        ..r.clone()
    };
    let done = take_zip(&m, &files, &again, &mut lib(&m));
    assert!(matches!(&done[..], [Done::Owned(_)]), "{done:?}");
    assert_eq!(
        text(&m, "bms/Song/Song/sound/01.ogg").as_deref(),
        Some("archive's")
    );
    no_stage(&m);
}

/// Found by name: nothing comes in unless it holds the chart (a song's
/// archive: or a chart of the song).
#[test]
fn checked() {
    let m = store(&[]);
    let files = [("Song/song.bms", SONG), ("Song/sound/01.ogg", "x")];
    let r = |role, md5: &str| Request {
        md5: Some(md5.into()),
        check: true,
        want: Some(Want {
            title: "Song [ANOTHER]".into(),
            artist: "alpha".into(),
        }),
        ..req(role)
    };
    let other = md5_hex(b"#TITLE Other\n");
    let mut l = Library::new(Path::new("bms"));
    let done = take_zip(&m, &files, &r(Role::Diff, &other), &mut l);
    assert!(
        matches!(&done[..], [Done::Failed(Failure::NotTheChart)]),
        "{done:?}"
    );
    assert!(m.0.lock().unwrap().is_empty());
    // the song's own archive, its chart a separate one: the song
    let done = take_zip(&m, &files, &r(Role::Song, &other), &mut l);
    assert!(matches!(&done[..], [Done::Added(..)]), "{done:?}");
    // a chart's link: by its MD5 only
    let r2 = Request {
        file: Some("song.bms".into()),
        ..r(Role::Song, &other)
    };
    let done = take(&m, Blob::Bytes(SONG.into()), &r2, Path::new(DL), &mut l);
    assert!(matches!(&done[..], [Done::Failed(Failure::NotTheChart)]));
}

/// A song in memory end to end: unpacked, a folder of its own, the chart
/// that waited for it placed; a separate chart's pack keeps only the chart
/// wanted waiting.
#[test]
fn taken_in_memory() {
    let m = store(&[]);
    let mut l = Library::new(Path::new("bms"));
    let ours = "#TITLE Song [X]\n#ARTIST alpha\n#WAV01 sound/01.wav\n";
    let other = "#TITLE Other [Y]\n#ARTIST beta\n#WAV01 o.wav\n";
    let pack = [("pack/ours.bms", ours), ("pack/other.bms", other)];
    // a pack without the chart wanted: nothing
    let r = |md5: &str| Request {
        md5: Some(md5.into()),
        ..req(Role::Diff)
    };
    let done = take_zip(&m, &pack, &r(&md5_hex(b"x")), &mut l);
    assert!(matches!(&done[..], [Done::Failed(Failure::NotTheChart)]));
    let done = take_zip(&m, &pack, &r(&md5_hex(ours.as_bytes())), &mut l);
    assert!(
        matches!(&done[..], [Done::Waiting(p)] if p == Path::new("dl/ours.bms")),
        "{done:?}"
    );
    // the song: its folder, then the chart that waited into it
    let song = [("Song/song.bms", SONG), ("Song/sound/01.ogg", "x")];
    let r = Request {
        naming: Naming::Named("Song".into()),
        ..req(Role::Song)
    };
    let done = take_zip(&m, &song, &r, &mut l);
    assert!(
        matches!(&done[..], [Done::Added(d, 2), Done::Placed(p)]
            if d == Path::new("bms/Song") && p == Path::new("bms/Song/Song/ours.bms")),
        "{done:?}"
    );
    assert_eq!(
        m.get(Path::new("bms/Song/Song/sound/01.ogg")).unwrap().1,
        1_546_300_800
    );
    assert!(text(&m, "dl/ours.bms").is_none());
    no_stage(&m);
    // a chart on its own that is the library's: owned
    let r = Request {
        file: Some("ours.bms".into()),
        ..req(Role::Diff)
    };
    let done = take(&m, Blob::Bytes(ours.into()), &r, Path::new(DL), &mut l);
    assert!(
        matches!(&done[..], [Done::Owned(d)] if d == Path::new("bms/Song/Song")),
        "{done:?}"
    );
}

#[test]
fn votes() {
    let set = |v: &[&str]| v.iter().map(PathBuf::from).collect::<HashSet<_>>();
    assert_eq!(tally(vec![]), Vote::None);
    assert_eq!(
        tally(vec![set(&["a"]), set(&["a", "b"])]),
        Vote::One("a".into())
    );
    assert_eq!(
        tally(vec![set(&["b", "a"])]),
        Vote::Tie(vec!["a".into(), "b".into()])
    );
    assert_eq!(
        up(Path::new("bms/Song/Sub"), Path::new("sub")),
        Some("bms/Song".into())
    );
    assert_eq!(up(Path::new("bms/Song/x"), Path::new("sub")), None);
    assert_eq!(
        up(Path::new("bms/Song"), Path::new("")),
        Some("bms/Song".into())
    );
}

/// Folders named after their charts (opt-in): `[artist] title`, the folder
/// the archive wraps everything in left out; the same song already in the
/// library (under another name) goes into its folder, not a second one.
#[test]
fn folders_named_after_their_charts() {
    let naming = Naming::Charts {
        format: crate::naming::DEFAULT_FORMAT.into(),
        sjis_only: false,
    };
    let r = Request {
        naming,
        ..req(Role::Song)
    };
    let n = "#TITLE Song [NORMAL]\n#ARTIST alpha\n#WAV01 sound/01.wav\n";
    let h = "#TITLE Song [HYPER]\n#ARTIST alpha / obj: x\n#WAV01 sound/01.wav\n";
    let files = [
        ("Song_v1/n.bms", n),
        ("Song_v1/h.bms", h),
        ("Song_v1/sound/01.ogg", "x"),
    ];
    let m = store(&[]);
    let done = take_zip(&m, &files, &r, &mut lib(&m));
    assert!(
        matches!(&done[..], [Done::Added(p, 3)] if p == Path::new("bms/[alpha] Song")),
        "{done:?}"
    );
    assert!(text(&m, "bms/[alpha] Song/sound/01.ogg").is_some());
    // the song in the library under another name: its folder
    let m = store(&[("bms/Old/n.bms", n), ("bms/Old/sound/01.ogg", "x")]);
    let a = "#TITLE Song [ANOTHER]\n#ARTIST alpha\n#WAV01 sound/01.wav\n#WAV02 sound/a.wav\n";
    let files = [
        ("Song_v2/a.bms", a),
        ("Song_v2/sound/01.ogg", "x"),
        ("Song_v2/sound/a.ogg", "y"),
    ];
    let done = take_zip(&m, &files, &r, &mut lib(&m));
    assert!(
        done.iter().all(|d| match d {
            Done::Added(p, _) | Done::Placed(p) => p.starts_with("bms/Old"),
            _ => false,
        }),
        "{done:?}"
    );
    assert!(text(&m, "bms/Old/a.bms").is_some() && text(&m, "bms/Old/sound/a.ogg").is_some());
    no_stage(&m);
}

/// A 差分 bringing sounds its song's folder lacked: a chart waiting for
/// them is placed too.
#[test]
fn a_diffs_sounds_place_a_waiting_chart() {
    let song = "#TITLE Song\n#ARTIST alpha\n#WAV01 01.wav\n";
    let two = "#WAV01 01.wav\n#WAV02 02.wav\n";
    let m = store(&[
        ("bms/Song/song.bms", song),
        ("bms/Song/01.ogg", "x"),
        (
            "dl/w.bms",
            &format!("#TITLE Song [W]\n#ARTIST alpha\n{two}"),
        ),
    ]);
    let diff = format!("#TITLE Song [D]\n#ARTIST alpha\n{two}");
    let files = [("d.bms", diff.as_str()), ("02.ogg", "y")];
    let done = take_zip(&m, &files, &req(Role::Diff), &mut lib(&m));
    assert!(
        matches!(&done[..], [Done::Placed(d), Done::Placed(w)]
            if d == Path::new("bms/Song/d.bms") && w == Path::new("bms/Song/w.bms")),
        "{done:?}"
    );
    no_stage(&m);
}

/// A store whose moves (and new files) fail where `fails` says (a full
/// disk, a file in use).
struct Failing {
    mem: Mem,
    fails: fn(&Path, &Path) -> bool,
}

impl Fs for Failing {
    fn read(&self, p: &Path) -> io::Result<Vec<u8>> {
        self.mem.read(p)
    }
    fn read_dir(&self, p: &Path) -> io::Result<Vec<crate::fs::Entry>> {
        self.mem.read_dir(p)
    }
    fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
        if (self.fails)(from, to) {
            return Err(io::ErrorKind::PermissionDenied.into());
        }
        self.mem.rename(from, to)
    }
    fn stamp(&self, p: &Path) -> Option<(i64, u64)> {
        self.mem.stamp(p)
    }
    fn remove(&self, p: &Path) -> io::Result<()> {
        self.mem.remove(p)
    }
}

impl Store for Failing {
    fn create(&self, p: &Path, mtime: Option<i64>) -> io::Result<Box<dyn Write + '_>> {
        if (self.fails)(p, p) {
            return Err(io::ErrorKind::StorageFull.into());
        }
        self.mem.create(p, mtime)
    }
    fn remove_dir(&self, p: &Path) {
        self.mem.remove_dir(p)
    }
}

/// A song's files that cannot all come in (one cannot be
/// written; one there cannot be set aside) leave its folder as it was.
#[test]
fn a_failed_move_keeps_the_song() {
    let cases: [fn(&Path, &Path) -> bool; 2] = [
        |_, to| to.to_string_lossy().contains("locked"),
        |from, _| from == Path::new("bms/Song/01.ogg"),
    ];
    for fails in cases {
        let mem = store(&[
            ("bms/Song/song.bms", "old"),
            ("bms/Song/01.ogg", "old"),
            ("dl/s/song.bms", "new"),
            ("dl/s/01.ogg", "new"),
            ("dl/s/locked.ogg", "new"),
        ]);
        let fs = Failing { mem, fails };
        let moves = ["song.bms", "01.ogg", "locked.ogg"]
            .map(|n| (Path::new("dl/s").join(n), Path::new("bms/Song").join(n)));
        let item = Item {
            outcome: Outcome::New("bms/Song".into()),
            moves: moves.to_vec(),
        };
        let mut lib = Library::scan(&fs, Path::new("bms"));
        let done = apply(&fs, vec![item], Path::new(DL), &mut lib);
        assert!(matches!(&done[..], [Done::Failed(_)]), "{done:?}");
        let left: Vec<PathBuf> = fs.mem.0.lock().unwrap().keys().cloned().collect();
        let song: Vec<&PathBuf> = left.iter().filter(|p| p.starts_with("bms")).collect();
        assert_eq!(song, ["bms/Song/01.ogg", "bms/Song/song.bms"]);
        assert_eq!(text(&fs.mem, "bms/Song/song.bms").as_deref(), Some("old"));
        assert_eq!(text(&fs.mem, "bms/Song/01.ogg").as_deref(), Some("old"));
    }
}

/// A newer file that cannot take the place of the one there (smart
/// overwrite) leaves it.
#[test]
fn a_failed_overwrite_keeps_the_file() {
    let mem = store(&[("bms/Song/song.bms", SONG), ("bms/Song/01.ogg", "old")]);
    mem.put(Path::new("dl/s/01.ogg"), b"new", 2000);
    let fs = Failing {
        mem,
        fails: |from, _| from.starts_with("dl"),
    };
    let item = Item {
        outcome: Outcome::Into("bms/Song".into()),
        moves: vec![("dl/s/01.ogg".into(), "bms/Song/01.ogg".into())],
    };
    let mut lib = Library::scan(&fs, Path::new("bms"));
    let done = apply(&fs, vec![item], Path::new(DL), &mut lib);
    assert!(matches!(&done[..], [Done::Failed(_)]), "{done:?}");
    assert_eq!(text(&fs.mem, "bms/Song/01.ogg").as_deref(), Some("old"));
    assert!(text(&fs.mem, "bms/Song/.01.ogg.old").is_none());
}
