use super::*;
use crate::fs::Mem;
use std::io::{self, Write};

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
    Staged::read(m, root, members, false, None)
}

/// A download cancelled: nothing of it is planned or comes in, the file
/// it came as stays, and what it passed through goes.
#[test]
fn cancelled_downloads_bring_nothing() {
    let m = store(&[("bms/Song/song.bms", SONG), ("bms/Song/sound/01.ogg", "x")]);
    let l = lib(&m);
    let chart = "#TITLE Song [X]\n#ARTIST alpha\n#WAV01 sound/01.wav\n";
    let s = staged(&m, &[("Diff/x.bms", chart), ("Song2/y.bms", SONG)]);
    let stop = crate::progress::Progress::new();
    let r = Request {
        progress: Some(stop.clone()),
        ..req(Role::Any)
    };
    assert!(plan(&m, &s, &l, &r)
        .iter()
        .all(|i| !matches!(i.outcome, Outcome::Rejected(_))));
    stop.cancel();
    let cancelled = |done: &[Done]| matches!(done, [Done::Failed(f)] if f.cancelled());
    let items = plan(&m, &s, &l, &r);
    assert!(
        matches!(&items[..], [Item { outcome: Outcome::Rejected(f), moves, .. }] if f.cancelled() && moves.is_empty()),
        "{items:?}"
    );
    // the whole of it: an archive in the downloads folder, as an import's
    m.put(
        Path::new("dl/x.zip"),
        &zip(&[("Diff/x.bms", chart)], 2019),
        0,
    );
    let done = take(
        &m,
        Blob::File("dl/x.zip".into()),
        &r,
        Path::new(DL),
        &mut lib(&m),
    );
    assert!(cancelled(&done), "{done:?}");
    assert!(m.get(Path::new("dl/x.zip")).is_some());
    assert!(text(&m, "bms/Song/x.bms").is_none());
    m.0.lock()
        .unwrap()
        .retain(|p, _| !p.starts_with("dl/.take-test"));
    no_stage(&m);
}

/// A folder of 3,000 charts with 30,000 sounds beside them, for songs the
/// library has: the folder's files are looked at once, not for each chart.
#[test]
fn many_charts_in_one_folder() {
    let m = store(&[
        ("bms/Song/song.bms", SONG),
        ("bms/Song/sound/01.ogg", "x"),
        (
            "bms/Other/o.bms",
            "#TITLE Other\n#ARTIST b\n#WAV01 o1.wav\n",
        ),
        ("bms/Other/o1.ogg", "x"),
    ]);
    let l = lib(&m);
    let chart =
        |i| format!("#TITLE Song [{i}]\n#ARTIST alpha\n#WAV01 sound/01.wav\n#WAV02 k{i}.wav\n");
    let paths: Vec<(String, String)> = (0..3_000)
        .map(|i| (format!("Diff/{i}.bms"), chart(i)))
        .chain((0..30_000).map(|i| (format!("Diff/k{i}.ogg"), "x".into())))
        .collect();
    let files: Vec<(&str, &str)> = (paths.iter().map(|(p, b)| (p.as_str(), b.as_str()))).collect();
    let s = staged(&m, &files);
    let items = plan(&m, &s, &l, &req(Role::Diff));
    // all into the song's folder, as the download lays them out
    assert!(
        matches!(&items[..], [Item { outcome: Outcome::Into(d), moves, .. }]
            if d == Path::new("bms/Song") && moves.len() == 33_000),
        "{:?}",
        items.iter().map(|i| &i.outcome).collect::<Vec<_>>()
    );
    // each on its own (no folder holds them all): the first of the folder's
    // charts that goes somewhere takes the files beside it
    let other = "#TITLE Other [X]\n#ARTIST b\n#WAV01 o1.wav\n";
    let mut files = files;
    files.push(("Diff/other.bms", other));
    let s = staged(&m, &files);
    let items = plan(&m, &s, &l, &req(Role::Diff));
    assert_eq!(items.len(), 3_001);
    let moved: usize = items.iter().map(|i| i.moves.len()).sum();
    assert_eq!(moved, 33_001);
    let wanted = Request {
        md5: Some(md5_hex(chart(7).as_bytes())),
        ..req(Role::Diff)
    };
    assert_eq!(plan(&m, &s, &l, &wanted).len(), 3_001);
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
    let m = store(&[("bms/Song/Sub/a.bms", SONG)]);
    let l = lib(&m);
    let up = |folder: &str, sub: &str| up(&m, &l, Path::new(folder), Path::new(sub));
    assert_eq!(up("bms/Song/Sub", "sub"), Some("bms/Song".into()));
    assert_eq!(up("bms/Song/x", "sub"), None);
    assert_eq!(up("bms/Song", ""), Some("bms/Song".into()));
    // not the library's own folder (no song's folder), nor above it
    assert_eq!(up("bms/Song", "song"), None);
    assert_eq!(up("bms/Song", "bms/song"), None);
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
            over: Over::All,
            within: Vec::new(),
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
    // (the downloads on another disk, a full one: no copy is written)
    let fs = Failing {
        mem,
        fails: |from, to| from.starts_with("dl") || from == to,
    };
    let item = Item {
        outcome: Outcome::Into("bms/Song".into()),
        moves: vec![("dl/s/01.ogg".into(), "bms/Song/01.ogg".into())],
        over: Over::Newer,
        within: vec!["bms/Song".into()],
    };
    let mut lib = Library::scan(&fs, Path::new("bms"));
    let done = apply(&fs, vec![item.clone()], Path::new(DL), &mut lib);
    assert!(matches!(&done[..], [Done::Failed(_)]), "{done:?}");
    assert_eq!(text(&fs.mem, "bms/Song/01.ogg").as_deref(), Some("old"));
    assert!(text(&fs.mem, "bms/Song/.01.ogg.old").is_none());
    // another disk with room: copied in
    let fs = Failing {
        mem: fs.mem,
        fails: |from, _| from.starts_with("dl"),
    };
    let done = apply(&fs, vec![item], Path::new(DL), &mut lib);
    assert!(done.is_empty(), "{done:?}");
    assert_eq!(text(&fs.mem, "bms/Song/01.ogg").as_deref(), Some("new"));
    assert!(text(&fs.mem, "dl/s/01.ogg").is_none());
}

/// A tar holding one path twice (a file appended again): the later one
/// comes in.
#[test]
fn a_path_twice_in_a_tar() {
    let m = store(&[]);
    let files = [
        ("Song/song.bms", SONG),
        ("Song/sound/01.ogg", "old"),
        ("Song/sound/01.ogg", "new"),
    ];
    let tar = Blob::Bytes(crate::testkit::tar(&files));
    let done = take(&m, tar, &req(Role::Song), Path::new(DL), &mut lib(&m));
    let [Done::Added(d, 2)] = &done[..] else {
        panic!("{done:?}");
    };
    let ogg = d.join("Song/sound/01.ogg");
    assert_eq!(text(&m, &ogg.to_string_lossy()).as_deref(), Some("new"));
    no_stage(&m);
}

/// The downloads on another disk, a full one: the n-th copy fails midway.
/// The song's folder stays as it was, nothing of the copies left.
#[test]
fn a_copy_failing_midway_leaves_nothing() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    /// Writes a byte, then fails.
    struct Half<'a>(Box<dyn Write + 'a>);
    impl Write for Half<'_> {
        fn write(&mut self, b: &[u8]) -> io::Result<usize> {
            self.0.write_all(&b[..b.len().min(1)])?;
            Err(io::ErrorKind::StorageFull.into())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    /// Copies: this many in full, the next one in part.
    struct Full {
        mem: Mem,
        copies: AtomicUsize,
    }
    impl Fs for Full {
        fn read(&self, p: &Path) -> io::Result<Vec<u8>> {
            self.mem.read(p)
        }
        fn read_dir(&self, p: &Path) -> io::Result<Vec<crate::fs::Entry>> {
            self.mem.read_dir(p)
        }
        fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
            match from.starts_with("dl") {
                true => Err(io::ErrorKind::CrossesDevices.into()),
                false => self.mem.rename(from, to),
            }
        }
        fn stamp(&self, p: &Path) -> Option<(i64, u64)> {
            self.mem.stamp(p)
        }
        fn remove(&self, p: &Path) -> io::Result<()> {
            self.mem.remove(p)
        }
    }
    impl Store for Full {
        fn create(&self, p: &Path, mtime: Option<i64>) -> io::Result<Box<dyn Write + '_>> {
            let w = self.mem.create(p, mtime)?;
            match self.copies.fetch_sub(1, Ordering::Relaxed) {
                0 => Ok(Box::new(Half(w))),
                _ => Ok(w),
            }
        }
        fn remove_dir(&self, p: &Path) {
            self.mem.remove_dir(p)
        }
    }
    for n in 0..3 {
        let mem = store(&[
            ("bms/Song/song.bms", "old"),
            ("bms/Song/01.ogg", "old"),
            ("dl/s/song.bms", "new"),
            ("dl/s/01.ogg", "new"),
            ("dl/s/02.ogg", "new"),
        ]);
        let fs = Full {
            mem,
            copies: AtomicUsize::new(n),
        };
        let moves = ["song.bms", "01.ogg", "02.ogg"]
            .map(|n| (Path::new("dl/s").join(n), Path::new("bms/Song").join(n)));
        let item = Item {
            outcome: Outcome::New("bms/Song".into()),
            moves: moves.to_vec(),
            over: Over::All,
            within: Vec::new(),
        };
        let mut lib = Library::scan(&fs, Path::new("bms"));
        let done = apply(&fs, vec![item], Path::new(DL), &mut lib);
        assert!(matches!(&done[..], [Done::Failed(_)]), "{done:?}");
        let left: Vec<PathBuf> = fs.mem.0.lock().unwrap().keys().cloned().collect();
        let song: Vec<&PathBuf> = left.iter().filter(|p| p.starts_with("bms")).collect();
        assert_eq!(song, ["bms/Song/01.ogg", "bms/Song/song.bms"], "{n}");
        assert_eq!(text(&fs.mem, "bms/Song/01.ogg").as_deref(), Some("old"));
    }
}

/// A file whose name is near the disk's limit (255 bytes) comes in: into
/// a new folder, and over itself.
#[test]
fn a_long_name_moves_in() {
    use crate::fs::Disk;
    let d = crate::testkit::tmp("long-name");
    // (250 bytes)
    let name = format!("{}.ogg", "あ".repeat(82));
    let (from, song) = (d.join("dl/s").join(&name), d.join("bms/Song"));
    let mut lib = Library::new(&d.join("bms"));
    for body in ["one", "two"] {
        std::fs::create_dir_all(from.parent().unwrap()).unwrap();
        std::fs::write(&from, body).unwrap();
        let item = Item {
            outcome: Outcome::New(song.clone()),
            moves: vec![(from.clone(), song.join(&name))],
            over: Over::All,
            within: Vec::new(),
        };
        let done = apply(&Disk, vec![item], &d.join("dl"), &mut lib);
        assert!(matches!(&done[..], [Done::Added(_, 1)]), "{done:?}");
        assert_eq!(std::fs::read(song.join(&name)).unwrap(), body.as_bytes());
        assert_eq!(std::fs::read_dir(&song).unwrap().count(), 1);
    }
}

/// An archive whose chart's path ends like the path of the library folder
/// that holds the chart (a song's link, a file brought by hand): its other
/// files do not go above the library.
#[test]
fn nothing_lands_above_the_library() {
    let m = store(&[
        ("root/bms/Some Song/known.bms", SONG),
        ("root/bms/Some Song/sound/01.ogg", "x"),
    ]);
    let mut l = Library::scan(&m, Path::new("root/bms"));
    let files = [
        ("bms/Some Song/known.bms", SONG),
        ("x.png", "img"),
        ("evil.dll", "payload"),
        ("config/config.toml", "hostile"),
    ];
    for role in [Role::Song, Role::Any] {
        let _ = take(
            &m,
            Blob::Bytes(zip(&files, 2019)),
            &req(role),
            Path::new("root/bms/dl"),
            &mut l,
        );
    }
    let keys: Vec<PathBuf> = m.0.lock().unwrap().keys().cloned().collect();
    let outside: Vec<&PathBuf> = keys.iter().filter(|p| !p.starts_with("root/bms")).collect();
    assert!(
        outside.is_empty(),
        "written outside the library: {outside:?}"
    );
}

/// The same through a table: its `org_md5` names a chart the library has,
/// its `url_diff` an archive whose new chart's path ends like that chart's
/// folder. What it brings stays in the song's folder: nothing above the
/// library, nothing in the library's own folder, no file elsewhere
/// replaced.
#[test]
fn a_tables_hint_keeps_to_the_songs_folder() {
    let m = store(&[
        ("root/bms/Some Song/known.bms", SONG),
        ("root/bms/Some Song/sound/01.ogg", "x"),
        ("root/config/config.toml", "mine"),
        ("root/bms/config/config.toml", "mine"),
    ]);
    let mut l = Library::scan(&m, Path::new("root/bms"));
    let r = Request {
        org_md5: vec![md5_hex(SONG.as_bytes())],
        ..req(Role::Diff)
    };
    for at in ["bms/Some Song", "Some Song"] {
        let (chart, body) = (format!("{at}/new.bms"), format!("#TITLE New {at}\n"));
        let files = [
            (chart.as_str(), body.as_str()),
            ("x.png", "img"),
            ("evil.dll", "payload"),
            ("config/config.toml", "hostile"),
        ];
        let blob = Blob::Bytes(zip(&files, 2019));
        let done = take(&m, blob, &r, Path::new("root/dl"), &mut l);
        assert!(
            matches!(&done[..], [Done::Placed(p)] if p.starts_with("root/bms/Some Song")),
            "{at}: {done:?}"
        );
    }
    for p in ["root/config/config.toml", "root/bms/config/config.toml"] {
        assert_eq!(text(&m, p).as_deref(), Some("mine"), "{p}");
    }
    let keys: Vec<PathBuf> = m.0.lock().unwrap().keys().cloned().collect();
    let outside: Vec<&PathBuf> = keys
        .iter()
        .filter(|p| !p.starts_with("root/bms/Some Song") && !p.ends_with("config/config.toml"))
        .collect();
    assert!(outside.is_empty(), "outside the song's folder: {outside:?}");
}

/// A player's library may hold a song outside the folder downloads go to
/// (its records name its charts): what is for that song goes into its
/// folder there, and nothing above it.
#[test]
fn a_song_kept_elsewhere() {
    let m = store(&[
        ("music/Song/song.bms", SONG),
        ("music/Song/sound/01.ogg", "x"),
    ]);
    let md5 = md5_hex(SONG.as_bytes());
    let owned = crate::library::Owned {
        path: "music/Song/song.bms".into(),
        md5: md5.clone(),
        ..Default::default()
    };
    let mut l = Library::from_catalog(Path::new("bms"), [owned]);
    // the song again: the file its folder lacks
    let files = [("Song/song.bms", SONG), ("Song/bg.png", "img")];
    let done = take_zip(&m, &files, &req(Role::Song), &mut l);
    assert!(
        matches!(&done[..], [Done::Owned(d)] if d == Path::new("music/Song")),
        "{done:?}"
    );
    assert_eq!(text(&m, "music/Song/bg.png").as_deref(), Some("img"));
    // a chart a table says is that song's
    let r = Request {
        org_md5: vec![md5],
        ..req(Role::Diff)
    };
    let done = take_zip(&m, &[("r.bms", "#TITLE Remix [X]\n")], &r, &mut l);
    assert!(
        matches!(&done[..], [Done::Placed(p)] if p == Path::new("music/Song/r.bms")),
        "{done:?}"
    );
    // paths ending like the folder's: nothing above it
    let known = [
        ("music/Song/song.bms", SONG),
        ("x.png", "img"),
        ("evil.dll", "payload"),
    ];
    take_zip(&m, &known, &req(Role::Song), &mut l);
    let new = [
        ("music/Song/new.bms", "#TITLE New\n"),
        ("x.png", "img"),
        ("evil.dll", "payload"),
    ];
    take_zip(&m, &new, &r, &mut l);
    let keys: Vec<PathBuf> = m.0.lock().unwrap().keys().cloned().collect();
    let outside: Vec<&PathBuf> = keys
        .iter()
        .filter(|p| !p.starts_with("music/Song"))
        .collect();
    assert!(outside.is_empty(), "outside the song's folder: {outside:?}");
}

/// A plan whose files would go outside the library (a chart that waits:
/// outside the downloads) is not carried out.
#[test]
fn moves_keep_to_the_library() {
    let m = store(&[("dl/s/a.ogg", "new")]);
    let mut l = lib(&m);
    let into = || Outcome::Into("bms/Song".into());
    let plans = [
        (into(), "bms/Song/../../a.ogg"),
        (into(), "elsewhere/a.ogg"),
        (into(), "/bms/Song/a.ogg"),
        (Outcome::Into("elsewhere".into()), "elsewhere/a.ogg"),
        (Outcome::Into("bms".into()), "bms/a.ogg"),
        (Outcome::Owned("bms/Song".into()), "bms"),
        (Outcome::New("other".into()), "other/a.ogg"),
        (Outcome::Waiting, "../a.ogg"),
        (Outcome::Waiting, "/tmp/a.ogg"),
    ];
    for (outcome, to) in plans {
        let item = Item {
            outcome,
            moves: vec![("dl/s/a.ogg".into(), to.into())],
            over: Over::Keep,
            within: Vec::new(),
        };
        let done = apply(&m, vec![item], Path::new(DL), &mut l);
        assert!(matches!(&done[..], [Done::Failed(_)]), "{to}: {done:?}");
    }
    let left: Vec<PathBuf> = m.0.lock().unwrap().keys().cloned().collect();
    assert_eq!(left, [Path::new("dl/s/a.ogg")]);
}

/// A download whose charts only say they are a song the library has (its
/// title and artist, a folder of its name) replaces none of its files:
/// only those it lacks come in, a chart of a name there beside it;
/// reinstalled, the files are replaced.
#[test]
fn a_song_by_its_name_alone_replaces_nothing() {
    let m = store(&[("bms/Song/song.bms", SONG), ("bms/Song/01.ogg", "old")]);
    let other = format!("{SONG}#PLAYLEVEL 12\n");
    let files = [
        ("song.bms", other.as_str()),
        ("01.ogg", "new"),
        ("02.ogg", "new"),
    ];
    let r = Request {
        naming: Naming::Named("Song".into()),
        ..req(Role::Song)
    };
    let done = take_zip(&m, &files, &r, &mut lib(&m));
    assert!(
        matches!(&done[..], [Done::Added(d, _)] if d == Path::new("bms/Song")),
        "{done:?}"
    );
    assert_eq!(text(&m, "bms/Song/song.bms").as_deref(), Some(SONG));
    assert_eq!(text(&m, "bms/Song/song_.bms"), Some(other.clone()));
    assert_eq!(text(&m, "bms/Song/01.ogg").as_deref(), Some("old"));
    assert_eq!(text(&m, "bms/Song/02.ogg").as_deref(), Some("new"));
    assert!(text(&m, "bms/Song (2)/song.bms").is_none());
    no_stage(&m);
    // by its charts' titles and artists, the folder's name another's
    let m = store(&[
        ("bms/Old Name/song.bms", SONG),
        ("bms/Old Name/01.ogg", "old"),
    ]);
    let r = Request {
        naming: Naming::Charts {
            format: crate::naming::DEFAULT_FORMAT.into(),
            sjis_only: false,
        },
        ..req(Role::Song)
    };
    let done = take_zip(&m, &files, &r, &mut lib(&m));
    assert!(
        matches!(&done[..], [Done::Added(d, _)] if d == Path::new("bms/Old Name")),
        "{done:?}"
    );
    assert_eq!(text(&m, "bms/Old Name/song.bms").as_deref(), Some(SONG));
    assert_eq!(text(&m, "bms/Old Name/01.ogg").as_deref(), Some("old"));
    // brought in again: replaced
    let m = store(&[("bms/Song/song.bms", SONG), ("bms/Song/01.ogg", "old")]);
    let again = Request {
        naming: Naming::Named("Song".into()),
        reinstall: true,
        ..req(Role::Song)
    };
    take_zip(&m, &files, &again, &mut lib(&m));
    assert_eq!(text(&m, "bms/Song/song.bms"), Some(other));
    assert_eq!(text(&m, "bms/Song/01.ogg").as_deref(), Some("new"));
}

/// A 差分 placed by its sounds and its title (no chart of the library in
/// it) brings no file over one there, however new; one whose package holds
/// a chart of the folder does, by the overwrite rule.
#[test]
fn a_diff_by_its_sounds_replaces_nothing() {
    let m = store(&[
        ("bms/Song/song.bms", SONG),
        ("bms/Song/sound/01.ogg", "old"),
        ("bms/Song/sound/02.ogg", "old"),
    ]);
    let chart = "#TITLE Song [X]\n#ARTIST alpha\n#WAV01 sound/01.wav\n#WAV02 sound/02.wav\n";
    let files = [("x.bms", chart), ("sound/01.ogg", "new")];
    let done = take_zip(&m, &files, &req(Role::Diff), &mut lib(&m));
    assert!(
        matches!(&done[..], [Done::Placed(p)] if p == Path::new("bms/Song/x.bms")),
        "{done:?}"
    );
    assert_eq!(text(&m, "bms/Song/sound/01.ogg").as_deref(), Some("old"));
    let chart = chart.replace("[X]", "[Y]");
    let files = [
        ("song.bms", SONG),
        ("y.bms", chart.as_str()),
        ("sound/01.ogg", "new"),
    ];
    let done = take_zip(&m, &files, &req(Role::Diff), &mut lib(&m));
    assert!(
        matches!(&done[..], [Done::Placed(p)] if p == Path::new("bms/Song/y.bms")),
        "{done:?}"
    );
    assert_eq!(text(&m, "bms/Song/sound/01.ogg").as_deref(), Some("new"));
}

/// A chart of the library's hash in a folder whose charts are of another
/// title and artist altogether (an MD5 made to match) does not make the
/// download that song: it is a song of its own.
#[test]
fn a_hash_unlike_its_folder_is_not_the_song() {
    use crate::library::Owned;
    let m = store(&[("bms/Song/song.bms", SONG), ("bms/Song/01.ogg", "old")]);
    let other = "#TITLE Zzz\n#ARTIST q\n#WAV01 01.wav\n";
    let files = [("Zzz/z.bms", other), ("Zzz/01.ogg", "new")];
    let s = staged(&m, &files);
    let catalog = |title: &str| Owned {
        path: "bms/Song/song.bms".into(),
        md5: s.charts[0].md5.clone(),
        title: title.into(),
        artist: "alpha".into(),
        ..Default::default()
    };
    let lib = Library::from_catalog(Path::new("bms"), [catalog("Song")]);
    let items = plan(&m, &s, &lib, &req(Role::Song));
    assert!(
        matches!(
            &items[..],
            [Item {
                outcome: Outcome::New(_),
                ..
            }]
        ),
        "{items:?}"
    );
    // a title the folder's: the library's song
    let lib = Library::from_catalog(Path::new("bms"), [catalog("Zzz")]);
    let items = plan(&m, &s, &lib, &req(Role::Song));
    assert!(
        matches!(&items[..], [Item { outcome: Outcome::Owned(d), .. }] if d == Path::new("bms/Song")),
        "{items:?}"
    );
}

/// A library of packs (`bms/Pack/X`, `bms/Pack/Y`): a download laid out as
/// the pack, holding a chart of X the library has, goes into the pack's
/// folder; it replaces files of X by the overwrite rule, none of Y's,
/// whatever its link and brought in again or not.
#[test]
fn a_pack_replaces_only_its_songs_files() {
    let x = "#TITLE X\n#ARTIST alpha\n#WAV01 01.wav\n#WAV02 02.wav\n";
    let y = "#TITLE Y\n#ARTIST beta\n#WAV01 01.wav\n";
    let pack = || {
        store(&[
            ("bms/Pack/X/x.bms", x),
            ("bms/Pack/X/01.ogg", "old"),
            ("bms/Pack/X/02.ogg", "old"),
            ("bms/Pack/Y/y.bms", y),
            ("bms/Pack/Y/01.ogg", "Y's"),
            ("bms/Pack/Y/bg.png", "Y's"),
        ])
    };
    // (a chart at its top that uses Y's sound: the whole of it one package,
    // not a song's whole folder: X's 01.ogg is not in it)
    let top = "#TITLE X [top]\n#ARTIST alpha\n#WAV01 Y/01.wav\n";
    let files = [
        ("x.bms", top),
        ("X/x.bms", x),
        ("X/02.ogg", "new"),
        ("Y/01.ogg", "new"),
        ("Y/bg.png", "new"),
    ];
    for role in [Role::Song, Role::Diff, Role::Any] {
        for reinstall in [false, true] {
            let m = pack();
            let r = Request {
                reinstall,
                ..req(role)
            };
            let done = take_zip(&m, &files, &r, &mut lib(&m));
            let at = format!("{role:?}, reinstall {reinstall}");
            assert!(
                matches!(&done[..], [Done::Placed(p)] if p == Path::new("bms/Pack/x.bms")),
                "{at}: {done:?}"
            );
            assert_eq!(
                text(&m, "bms/Pack/X/02.ogg").as_deref(),
                Some("new"),
                "{at}"
            );
            for f in ["bms/Pack/Y/01.ogg", "bms/Pack/Y/bg.png"] {
                assert_eq!(text(&m, f).as_deref(), Some("Y's"), "{at}: {f}");
            }
        }
    }
    // all its charts the library's (Y's sound in no package: the whole of
    // it one)
    let files = [("X/x.bms", x), ("X/01.ogg", "new"), ("Y/01.ogg", "new")];
    for reinstall in [false, true] {
        let m = pack();
        let r = Request {
            reinstall,
            ..req(Role::Song)
        };
        let done = take_zip(&m, &files, &r, &mut lib(&m));
        assert!(
            matches!(&done[..], [Done::Owned(d)] if d == Path::new("bms/Pack")),
            "reinstall {reinstall}: {done:?}"
        );
        let x = text(&m, "bms/Pack/X/01.ogg");
        assert_eq!(x.as_deref(), Some(if reinstall { "new" } else { "old" }));
        assert_eq!(text(&m, "bms/Pack/Y/01.ogg").as_deref(), Some("Y's"));
    }
}

/// A song wrapped in a folder with a file beside it at the top (a cover):
/// the song the library has gets the files its folder lacks, brought in
/// again or not; a new chart of it goes into its folder.
#[test]
fn a_wrapped_song_with_a_file_beside_it() {
    let known = "#TITLE X\n#ARTIST alpha\n#WAV01 01.wav\n";
    let files = [
        ("SongX/known.bms", known),
        ("SongX/01.ogg", "x"),
        ("SongX/02.ogg", "lacked"),
        ("cover.png", "img"),
    ];
    for reinstall in [false, true] {
        let m = store(&[("bms/SongX/known.bms", known), ("bms/SongX/01.ogg", "x")]);
        let r = Request {
            reinstall,
            ..req(Role::Song)
        };
        let done = take_zip(&m, &files, &r, &mut lib(&m));
        assert!(
            matches!(&done[..], [Done::Owned(d)] if d == Path::new("bms/SongX")),
            "reinstall {reinstall}: {done:?}"
        );
        assert_eq!(text(&m, "bms/SongX/02.ogg").as_deref(), Some("lacked"));
        assert!(text(&m, "bms/cover.png").is_none() && text(&m, "bms/SongX/cover.png").is_none());
        no_stage(&m);
    }
    let m = store(&[("bms/SongX/known.bms", known), ("bms/SongX/01.ogg", "x")]);
    let new = "#TITLE X [another]\n#ARTIST alpha\n#WAV01 01.wav\n";
    let files = [
        ("SongX/known.bms", known),
        ("SongX/new.bms", new),
        ("cover.png", "img"),
    ];
    let done = take_zip(&m, &files, &req(Role::Diff), &mut lib(&m));
    assert!(
        matches!(&done[..], [Done::Placed(p)] if p == Path::new("bms/SongX/new.bms")),
        "{done:?}"
    );
}

/// The pack library of [`a_pack_replaces_only_its_songs_files`], and a
/// chart it keeps at its top (`bms/loose.bms`).
fn packs() -> Mem {
    store(&[
        ("bms/Pack/X/x.bms", PACK_X),
        ("bms/Pack/X/01.ogg", "old"),
        ("bms/Pack/X/02.ogg", "old"),
        (
            "bms/Pack/Y/y.bms",
            "#TITLE Y\n#ARTIST beta\n#WAV01 01.wav\n",
        ),
        ("bms/Pack/Y/01.ogg", "Y's"),
        ("bms/Pack/Y/bg.png", "Y's"),
        ("bms/loose.bms", LOOSE),
    ])
}

const PACK_X: &str = "#TITLE X\n#ARTIST alpha\n#WAV01 01.wav\n#WAV02 02.wav\n";
const LOOSE: &str = "#TITLE Loose\n#ARTIST gamma\n";
/// (a chart at a pack's top that uses Y's sound: the whole of it one
/// package)
const PACK_TOP: &str = "#TITLE X [top]\n#ARTIST alpha\n#WAV01 Y/01.wav\n";

/// A pack's download that also holds, where it says nothing of where the
/// download goes, a copy of a chart the library keeps above the pack's
/// songs: the songs beside the one it is for keep their files, whatever
/// its link and brought in again or not.
#[test]
fn a_chart_kept_above_the_songs_opens_none() {
    let with_top = [
        ("x.bms", PACK_TOP),
        ("X/x.bms", PACK_X),
        ("X/02.ogg", "new"),
        ("junk/loose.bms", LOOSE),
        ("Y/01.ogg", "new"),
        ("Y/bg.png", "new"),
    ];
    // (all its charts the library's)
    let owned = [
        ("X/x.bms", PACK_X),
        ("X/02.ogg", "new"),
        ("junk/loose.bms", LOOSE),
        ("Y/01.ogg", "new"),
        ("Y/bg.png", "new"),
    ];
    for files in [&with_top[..], &owned[..]] {
        for role in [Role::Song, Role::Diff, Role::Any] {
            for reinstall in [false, true] {
                let m = packs();
                let r = Request {
                    reinstall,
                    ..req(role)
                };
                take_zip(&m, files, &r, &mut lib(&m));
                let at = format!("{} files, {role:?}, reinstall {reinstall}", files.len());
                for f in ["bms/Pack/Y/01.ogg", "bms/Pack/Y/bg.png"] {
                    assert_eq!(text(&m, f).as_deref(), Some("Y's"), "{at}: {f}");
                }
                if files.len() == with_top.len() || reinstall {
                    let x = text(&m, "bms/Pack/X/02.ogg");
                    assert_eq!(x.as_deref(), Some("new"), "{at}");
                }
            }
        }
    }
}
