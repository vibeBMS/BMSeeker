use super::*;
use crate::fs::Mem;

/// A zip of `files` (name, contents), each of `time` (a year).
fn zip(files: &[(&str, &str)], year: u16) -> Vec<u8> {
    use std::io::Write;
    let mut out = std::io::Cursor::new(Vec::new());
    {
        let mut z = zip::ZipWriter::new(&mut out);
        let at = zip::DateTime::from_date_and_time(year, 1, 1, 0, 0, 0).unwrap();
        let o = zip::write::SimpleFileOptions::default().last_modified_time(at);
        for (n, b) in files {
            z.start_file(*n, o).unwrap();
            z.write_all(b.as_bytes()).unwrap();
        }
        z.finish().unwrap();
    }
    out.into_inner()
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
    m.remove(Path::new("bms/Song/Song/bg.png"));
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
