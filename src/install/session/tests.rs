use super::*;
use crate::fs::Mem;
use crate::hosts::Want;
use crate::install::job::tests::Canned;
use crate::install::job::{BUSY_ASKS, BUSY_WAIT};
use crate::install::md5_hex;
use crate::net::job::{Driver, Effect, Served};
use crate::net::NetError;
use crate::progress::Progress;
use crate::testkit::zip;
use std::path::Path;

const PACK: &str = "https://p.test/pack.zip";

fn chart(title: &str) -> String {
    format!("#TITLE {title}\n#ARTIST p\n#WAV01 01.wav\n")
}

/// One song's archive holding the charts of two entries.
fn pack() -> Vec<u8> {
    let (one, two) = (chart("Pack [1]"), chart("Pack [2]"));
    zip(&[
        ("Pack/1.bms", one.as_str()),
        ("Pack/2.bms", two.as_str()),
        ("Pack/01.wav", "RIFF"),
    ])
}

/// The entry of the pack's chart `n`.
fn entry(n: u8) -> Entry {
    let title = format!("Pack [{n}]");
    Entry {
        md5: md5_hex(chart(&title).as_bytes()),
        want: Want {
            title,
            artist: "p".into(),
        },
        song: vec![PACK.into()],
        ..Default::default()
    }
}

/// A session on the library `m` holds, wanting the pack's charts.
fn session(m: &Arc<Mem>) -> Session {
    let lib = Library::scan(&**m, Path::new("bms"));
    let s = Session::new(m.clone(), "dl".into(), lib, Sources::default());
    s.want([entry(1).md5, entry(2).md5]);
    s
}

/// The step after `step`, its effect done by `d` (one that is done stays
/// done): two jobs taken a step at a time, in the order a test says.
fn next(job: &mut SessionJob, step: Step<Vec<Done>>, d: &dyn Driver) -> Step<Vec<Done>> {
    match step {
        Step::Do(e) => job.feed(d.perform(e)),
        done => done,
    }
}

fn done(step: Step<Vec<Done>>) -> Vec<Done> {
    match step {
        Step::Done(d) => d,
        Step::Do(e) => panic!("not done: {e:?}"),
    }
}

fn waits(step: &Step<Vec<Done>>) -> bool {
    matches!(step, Step::Do(Effect::Sleep(w)) if *w == BUSY_WAIT)
}

fn downloads(d: &Canned) -> Vec<String> {
    let done = d.done.borrow();
    done.iter()
        .filter(|s| s.starts_with("download"))
        .cloned()
        .collect()
}

/// Whether the session's library has the chart of `hash`.
fn owns(s: &Session, hash: &str) -> bool {
    s.with_library(|l| l.owns(hash)) == Some(true)
}

fn owns_both(s: &Session) -> bool {
    owns(s, &entry(1).md5) && owns(s, &entry(2).md5)
}

/// Two entries whose song is one pack: the second waits while the first
/// downloads it, then finds its chart in the library. One download.
#[test]
fn a_shared_pack_downloaded_once() {
    let m = Arc::new(Mem::default());
    let s = session(&m);
    let d = Canned::default();
    d.file(PACK, Ok(Served::bytes(pack(), None, None)));
    let cx = Context::default();
    let (mut one, mut two) = (s.job(entry(1), &cx), s.job(entry(2), &cx));
    let s1 = one.start();
    assert!(
        matches!(&s1, Step::Do(Effect::Download { target, .. }) if target.url == PACK),
        "{s1:?}"
    );
    let s2 = two.start();
    assert!(waits(&s2), "{s2:?}");
    let first = done(next(&mut one, s1, &d));
    let second = done(next(&mut two, s2, &d));
    let [Done::Added(dir, _)] = &first[..] else {
        panic!("{first:?}")
    };
    assert!(
        matches!(&second[..], [Done::Owned(d)] if d.starts_with(dir)),
        "{second:?}"
    );
    assert_eq!(downloads(&d), [format!("download {PACK}")]);
    assert!(owns_both(&s));
}

/// The first job's download fails: the second does not ask for the file
/// again, and fails as it did.
#[test]
fn a_failed_download_not_asked_again() {
    let m = Arc::new(Mem::default());
    let s = session(&m);
    let d = Canned::default();
    let cx = Context::default();
    let (mut one, mut two) = (s.job(entry(1), &cx), s.job(entry(2), &cx));
    let (s1, s2) = (one.start(), two.start());
    assert!(waits(&s2), "{s2:?}");
    let first = done(next(&mut one, s1, &d));
    let second = done(next(&mut two, s2, &d));
    for out in [&first, &second] {
        assert!(
            matches!(
                &out[..],
                [Done::Failed(Failure::Net(NetError::Status(404)))]
            ),
            "{out:?}"
        );
    }
    assert_eq!(downloads(&d).len(), 1);
}

/// The first job cancelled while it downloads: the second downloads the
/// pack itself.
#[test]
fn a_cancelled_download_left_to_the_other() {
    let m = Arc::new(Mem::default());
    let s = session(&m);
    let d = Canned::default();
    d.file(PACK, Ok(Served::bytes(pack(), None, None)));
    let p = Progress::new();
    let cx = Context::default();
    let mut one = s.job(entry(1), &cx.with_progress(p.clone()));
    let mut two = s.job(entry(2), &cx);
    assert!(matches!(one.start(), Step::Do(Effect::Download { .. })));
    let s2 = two.start();
    assert!(waits(&s2), "{s2:?}");
    p.cancel();
    let first = done(one.feed(Fed::Downloaded(Err(NetError::Cancelled.into()))));
    assert!(
        matches!(&first[..], [Done::Failed(f)] if f.cancelled()),
        "{first:?}"
    );
    let s2 = next(&mut two, s2, &d);
    assert!(matches!(&s2, Step::Do(Effect::Download { .. })), "{s2:?}");
    let second = done(next(&mut two, s2, &d));
    assert!(matches!(&second[..], [Done::Added(..)]), "{second:?}");
    assert!(owns_both(&s));
}

/// The first job dropped while it downloads: its claim goes with it. One
/// kept but never fed again (leaked): the second waits for it
/// `BUSY_ASKS` times, then downloads the pack itself.
#[test]
fn a_dropped_or_leaked_job_holds_nothing() {
    let cx = Context::default();
    let s = session(&Arc::new(Mem::default()));
    let mut one = s.job(entry(1), &cx);
    assert!(matches!(one.start(), Step::Do(Effect::Download { .. })));
    drop(one);
    let mut two = s.job(entry(2), &cx);
    assert!(matches!(two.start(), Step::Do(Effect::Download { .. })));
    let s = session(&Arc::new(Mem::default()));
    let mut one = s.job(entry(1), &cx);
    assert!(matches!(one.start(), Step::Do(Effect::Download { .. })));
    std::mem::forget(one);
    let mut two = s.job(entry(2), &cx);
    let mut step = two.start();
    let mut asked = 0;
    while waits(&step) {
        asked += 1;
        step = two.feed(Fed::Woke);
    }
    assert_eq!(asked, BUSY_ASKS);
    assert!(
        matches!(step, Step::Do(Effect::Download { .. })),
        "{step:?}"
    );
}

/// A job that starts after another brought its song in (made before
/// that): the chart first, the song not downloaded again.
#[test]
fn a_later_job_sees_the_song() {
    const N: &str = "#TITLE Song [N]\n#ARTIST alpha\n#WAV01 01.wav\n";
    const X: &str = "#TITLE Song [X]\n#ARTIST alpha\n#WAV01 01.wav\n";
    let song = zip(&[("Song/n.bms", N), ("Song/01.ogg", "x")]);
    let e = |title: &str, body: &str, chart: &[&str]| Entry {
        md5: md5_hex(body.as_bytes()),
        want: Want {
            title: title.into(),
            artist: "alpha".into(),
        },
        song: vec!["https://e.test/song.zip".into()],
        chart: chart.iter().map(|c| c.to_string()).collect(),
        ..Default::default()
    };
    let d = Canned::default();
    d.file(
        "https://e.test/song.zip",
        Ok(Served::bytes(song, None, None)),
    );
    d.file(
        "https://e.test/x.bms",
        Ok(Served::bytes(X.into(), None, None)),
    );
    let s = session(&Arc::new(Mem::default()));
    let cx = Context::default();
    let x = e("Song [X]", X, &["https://e.test/x.bms"]);
    let mut later = s.job(x.clone(), &cx);
    let first = crate::net::job::run_job(&mut s.job(e("Song [N]", N, &[]), &cx), &d);
    assert!(matches!(&first[..], [Done::Added(..)]), "{first:?}");
    let second = crate::net::job::run_job(&mut later, &d);
    assert!(
        matches!(&second[..], [Done::Placed(p)] if p.ends_with("x.bms")),
        "{second:?}"
    );
    assert_eq!(
        downloads(&d),
        [
            "download https://e.test/song.zip",
            "download https://e.test/x.bms"
        ]
    );
    assert!(owns(&s, &x.md5));
}

/// A job that starts after another brought its chart in: in the library
/// already, nothing downloaded.
#[test]
fn a_job_started_after_its_chart_came() {
    let s = session(&Arc::new(Mem::default()));
    let d = Canned::default();
    d.file(PACK, Ok(Served::bytes(pack(), None, None)));
    let cx = Context::default();
    let mut two = s.job(entry(2), &cx);
    assert!(!two.is_empty());
    let first = crate::net::job::run_job(&mut s.job(entry(1), &cx), &d);
    assert!(matches!(&first[..], [Done::Added(..)]), "{first:?}");
    let second = done(two.start());
    assert!(matches!(&second[..], [Done::Owned(_)]), "{second:?}");
    assert_eq!(downloads(&d).len(), 1);
}

/// Whether a job has anything to try is the entry's alone: what an
/// `EntryJob` on a library without the chart says, with its song there
/// or not.
#[test]
fn nothing_to_try_without_the_library() {
    let m = Arc::new(Mem::default());
    m.put(Path::new("bms/Song/n.bms"), chart("Song [N]").as_bytes(), 0);
    m.put(Path::new("bms/Song/01.wav"), b"RIFF", 0);
    let lib = Library::scan(&*m, Path::new("bms"));
    let s = Session::new(m.clone(), "dl".into(), lib.clone(), Sources::default());
    let cx = Context::default();
    let links: [&[&str]; 3] = [
        &[],
        &["", "file:///x.zip", "https://user@e.test/a.zip"],
        &["https://e.test/a.zip"],
    ];
    let l = &links;
    let combos: Vec<_> = (l.iter())
        .flat_map(|a| {
            l.iter()
                .flat_map(move |b| l.iter().map(move |c| (*a, *b, *c)))
        })
        .collect();
    let list = |l: &[&str]| l.iter().map(|u| u.to_string()).collect::<Vec<_>>();
    for title in ["Song [X]", "Other"] {
        for md5 in [String::new(), md5_hex(title.as_bytes())] {
            for (song, chart, addons) in combos.iter().copied() {
                let e = Entry {
                    md5: md5.clone(),
                    want: Want {
                        title: title.into(),
                        artist: "p".into(),
                    },
                    song: list(song),
                    chart: list(chart),
                    ..Default::default()
                };
                let (store, lib) = (m.clone(), lib.clone());
                let alone =
                    EntryJob::new(store, "dl".into(), lib, e.clone(), Sources::default(), &cx);
                let alone = alone.addons(list(addons));
                let job = s.job(e.clone(), &cx).addons(list(addons));
                assert_eq!(job.is_empty(), alone.is_empty(), "{e:?} {addons:?}");
            }
        }
    }
}

/// Making a job, asking whether it has anything to try, wanting charts
/// and dropping a job not started do not wait for a step that holds the
/// session (another job unpacking).
#[cfg(not(target_arch = "wasm32"))]
#[test]
fn a_screens_thread_never_waits() {
    let s = session(&Arc::new(Mem::default()));
    let (held, release) = std::sync::mpsc::channel::<()>();
    let (locked, is_locked) = std::sync::mpsc::channel::<()>();
    std::thread::scope(|scope| {
        let s = &s;
        let holder = scope.spawn(move || {
            let _step = s.0.state.lock().unwrap();
            locked.send(()).unwrap();
            // (a bound only for a failure: one that waits never signals)
            release
                .recv_timeout(std::time::Duration::from_secs(60))
                .is_ok()
        });
        is_locked.recv().unwrap();
        let job = s
            .job(entry(1), &Context::default())
            .addons(["https://e.test/x.bms".into()]);
        assert!(!job.is_empty());
        s.want([entry(2).md5]);
        drop(job);
        held.send(()).unwrap();
        assert!(holder.join().unwrap(), "waited for the session's lock");
    });
}

/// A driver of the pack for jobs on threads: a download takes a while, a
/// wait a moment; the downloads counted.
#[cfg(not(target_arch = "wasm32"))]
struct Slow {
    pack: Vec<u8>,
    downloads: std::sync::atomic::AtomicUsize,
}

#[cfg(not(target_arch = "wasm32"))]
impl Driver for Slow {
    fn perform(&self, effect: Effect) -> Fed {
        use std::time::Duration;
        match effect {
            Effect::Download { .. } => {
                (self.downloads).fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                std::thread::sleep(Duration::from_millis(20));
                Fed::Downloaded(Ok(Served::bytes(self.pack.clone(), None, None)))
            }
            Effect::Sleep(_) => {
                std::thread::sleep(Duration::from_millis(5));
                Fed::Woke
            }
            Effect::Http(_) => Fed::Http(Err(NetError::Status(404))),
        }
    }
}

/// Two jobs on threads of their own: one download of the pack, both
/// charts in the library after.
#[cfg(not(target_arch = "wasm32"))]
#[test]
fn two_threads_one_download() {
    let s = session(&Arc::new(Mem::default()));
    let d = Slow {
        pack: pack(),
        downloads: Default::default(),
    };
    let cx = Context::default();
    let mut states: Vec<&str> = std::thread::scope(|scope| {
        let runs: Vec<_> = [entry(1), entry(2)]
            .map(|e| {
                let mut job = s.job(e, &cx);
                let d = &d;
                scope.spawn(move || crate::net::job::run_job(&mut job, d))
            })
            .into_iter()
            .collect();
        let outs = runs.into_iter().map(|r| r.join().unwrap());
        outs.map(|o| match &o[..] {
            [only] => only.state(),
            _ => panic!("{o:?}"),
        })
        .collect()
    });
    states.sort();
    assert_eq!(states, ["added", "owned"]);
    assert_eq!(d.downloads.into_inner(), 1);
    assert!(owns_both(&s));
}

/// A source that panics when asked (a bug in a step).
#[cfg(not(target_arch = "wasm32"))]
struct Boom;

#[cfg(not(target_arch = "wasm32"))]
impl crate::sources::Source for Boom {
    fn name(&self) -> &'static str {
        "boom"
    }
    fn find(&self, _: &crate::sources::Query) -> crate::net::BoxTask<Vec<String>> {
        panic!("a step that panics")
    }
}

/// A job that panicked with the library lent to it: the session's jobs
/// end at once, and the library is read as lost, not as the empty one
/// left in its place.
#[cfg(not(target_arch = "wasm32"))]
#[test]
fn a_lost_library_ends_the_jobs() {
    let m = Arc::new(Mem::default());
    let had = chart("Had [N]");
    m.put(Path::new("bms/Had/n.bms"), had.as_bytes(), 0);
    m.put(Path::new("bms/Had/01.wav"), b"RIFF", 0);
    let lib = Library::scan(&*m, Path::new("bms"));
    let mut sources = Sources::default();
    sources.push(Box::new(Boom));
    let s = Session::new(m.clone(), "dl".into(), lib, sources);
    let md5 = md5_hex(had.as_bytes());
    assert!(owns(&s, &md5));
    // no link: the sources are asked, the library lent
    let e = Entry {
        song: Vec::new(),
        ..entry(1)
    };
    let mut job = s.job(e, &Context::default());
    let panicked = std::thread::spawn(move || {
        let _ = job.start();
    })
    .join();
    assert!(panicked.is_err());
    assert!(s.with_library(|l| l.owns(&md5)).is_none());
    let out = done(s.job(entry(2), &Context::default()).start());
    assert!(
        matches!(&out[..], [Done::Failed(Failure::Other(e))] if e == LOST),
        "{out:?}"
    );
}

const PAGE: &str = "https://p.test/event.html";

fn page() -> Result<Served, crate::net::job::Unserved> {
    Ok(Served::bytes(
        b"<html>the event</html>".to_vec(),
        None,
        None,
    ))
}

/// The entry of the pack's chart `n` whose only link is `link`.
fn entry_at(n: u8, link: &str) -> Entry {
    Entry {
        song: vec![link.into()],
        ..entry(n)
    }
}

/// Two entries whose link is a page (an event's site): each is told the
/// page, one after another or at once; on the web, where a download that
/// fails is a page, too. The page asked for once.
#[test]
fn a_shared_page_is_a_page_for_each() {
    let cx = Context::default();
    let is_page = |out: &[Done]| matches!(out, [Done::Page(u)] if u == PAGE);
    let s = session(&Arc::new(Mem::default()));
    let d = Canned::default();
    d.file(PAGE, page());
    let first = crate::net::job::run_job(&mut s.job(entry_at(1, PAGE), &cx), &d);
    let second = crate::net::job::run_job(&mut s.job(entry_at(2, PAGE), &cx), &d);
    assert!(is_page(&first) && is_page(&second), "{first:?} {second:?}");
    assert_eq!(downloads(&d).len(), 1);
    // at once: the second waits, then is told the page
    let s = session(&Arc::new(Mem::default()));
    let d = Canned::default();
    d.file(PAGE, page());
    let (mut one, mut two) = (s.job(entry_at(1, PAGE), &cx), s.job(entry_at(2, PAGE), &cx));
    let (s1, s2) = (one.start(), two.start());
    assert!(waits(&s2), "{s2:?}");
    let first = done(next(&mut one, s1, &d));
    let second = done(next(&mut two, s2, &d));
    assert!(is_page(&first) && is_page(&second), "{first:?} {second:?}");
    assert_eq!(downloads(&d).len(), 1);
    // a web page's: the failed download is a page for both
    let web = Context {
        failures_are_pages: true,
        ..Default::default()
    };
    let s = session(&Arc::new(Mem::default()));
    let d = Canned::default();
    let first = crate::net::job::run_job(&mut s.job(entry(1), &web), &d);
    let second = crate::net::job::run_job(&mut s.job(entry(2), &web), &d);
    for out in [&first, &second] {
        assert!(matches!(&out[..], [Done::Page(u)] if u == PACK), "{out:?}");
    }
    assert_eq!(downloads(&d).len(), 1);
}

/// A retry in the same session (`again`) tries again what failed: a
/// download that failed (the server has it now), a page (on the web: a
/// failed download), a file that came broken. In the same round the
/// failure answers at once, the file not asked for; what came in stays
/// known after `again`.
#[test]
fn a_retry_in_a_new_round_tries_again() {
    let cx = Context::default();
    let web = Context {
        failures_are_pages: true,
        ..Default::default()
    };
    let mut cut = pack();
    cut.truncate(cut.len() / 2);
    let cases: [(&Context, Option<Vec<u8>>); 3] = [(&cx, None), (&web, None), (&cx, Some(cut))];
    for (cx, first) in cases {
        let s = session(&Arc::new(Mem::default()));
        let d = Canned::default();
        if let Some(body) = first {
            d.file(PACK, Ok(Served::bytes(body, None, None)));
        }
        let first = crate::net::job::run_job(&mut s.job(entry(1), cx), &d);
        assert!(!crate::install::got(&first), "{first:?}");
        // the server has it now
        d.file(PACK, Ok(Served::bytes(pack(), None, None)));
        let same = crate::net::job::run_job(&mut s.job(entry(1), cx), &d);
        assert!(!crate::install::got(&same), "{same:?}");
        assert_eq!(downloads(&d).len(), 1, "{first:?} {same:?}");
        s.again();
        let again = crate::net::job::run_job(&mut s.job(entry(1), cx), &d);
        assert!(matches!(&again[..], [Done::Added(..)]), "{again:?}");
        assert_eq!(downloads(&d).len(), 2);
        // what came is known: not downloaded again
        s.again();
        let two = crate::net::job::run_job(&mut s.job(entry(2), cx), &d);
        assert!(matches!(&two[..], [Done::Owned(_)]), "{two:?}");
        assert_eq!(downloads(&d).len(), 2);
    }
}

/// A download of the round before that fails after `again` answers no
/// job of the new round: the one waiting for it downloads the file itself.
#[test]
fn a_failure_of_the_round_before_answers_no_retry() {
    let s = session(&Arc::new(Mem::default()));
    let cx = Context::default();
    let mut one = s.job(entry(1), &cx);
    assert!(matches!(one.start(), Step::Do(Effect::Download { .. })));
    s.again();
    let mut two = s.job(entry(2), &cx);
    let s2 = two.start();
    assert!(waits(&s2), "{s2:?}");
    let first = done(one.feed(Fed::Downloaded(Err(NetError::Status(404).into()))));
    assert!(
        matches!(&first[..], [Done::Failed(Failure::Net(_))]),
        "{first:?}"
    );
    let d = Canned::default();
    d.file(PACK, Ok(Served::bytes(pack(), None, None)));
    let s2 = next(&mut two, s2, &d);
    assert!(matches!(&s2, Step::Do(Effect::Download { .. })), "{s2:?}");
    let second = done(next(&mut two, s2, &d));
    assert!(matches!(&second[..], [Done::Added(..)]), "{second:?}");
}

/// A separate charts' archive for two songs, its first chart's entry
/// downloaded before the second's chart was wanted (the queue grew, or it
/// was chosen alone): the second's chart is brought all the same. Wanted
/// from the start, the archive is downloaded once.
#[test]
fn a_chart_wanted_later_is_brought() {
    let chart = |title: &str, artist: &str| {
        format!("#TITLE {title}\n#ARTIST {artist}\n#WAV01 01.wav\n#WAV02 02.wav\n")
    };
    let (a_n, b_n, a_x, b_x) = (
        chart("Alpha [N]", "aa"),
        chart("Beta [N]", "bb"),
        chart("Alpha [X]", "aa / obj: z"),
        chart("Beta [X]", "bb / obj: z"),
    );
    let e = |body: &str, title: &str, artist: &str, song: &str| Entry {
        md5: md5_hex(body.as_bytes()),
        want: Want {
            title: title.into(),
            artist: artist.into(),
        },
        song: vec![format!("https://e.test/{song}")],
        chart: vec!["https://e.test/pack.zip".into()],
        ..Default::default()
    };
    let (ea, eb) = (
        e(&a_x, "Alpha [X]", "aa", "alpha.zip"),
        e(&b_x, "Beta [X]", "bb", "beta.zip"),
    );
    // (what is wanted first, and before the second's turn)
    let run = |first: &[&String], later: &[&String]| {
        let d = Canned::default();
        let file = |name: &str, files: &[(&str, &str)]| {
            d.file(
                &format!("https://e.test/{name}"),
                Ok(Served::bytes(zip(files), None, None)),
            );
        };
        file(
            "alpha.zip",
            &[
                ("Alpha/n.bms", &a_n),
                ("Alpha/01.ogg", "1"),
                ("Alpha/02.ogg", "2"),
            ],
        );
        file(
            "beta.zip",
            &[
                ("Beta/n.bms", &b_n),
                ("Beta/01.ogg", "1"),
                ("Beta/02.ogg", "2"),
            ],
        );
        file("pack.zip", &[("p/ax.bms", &a_x), ("p/bx.bms", &b_x)]);
        file("pack.zip", &[("p/ax.bms", &a_x), ("p/bx.bms", &b_x)]);
        let m = Arc::new(Mem::default());
        let lib = Library::scan(&*m, Path::new("bms"));
        let s = Session::new(m.clone(), "bms/.dl".into(), lib, Sources::default());
        s.want(first.iter().map(|m| m.to_string()));
        let cx = Context::default();
        let one = crate::net::job::run_job(&mut s.job(ea.clone(), &cx), &d);
        s.want(later.iter().map(|m| m.to_string()));
        let two = crate::net::job::run_job(&mut s.job(eb.clone(), &cx), &d);
        let ok = |out: &[Done]| {
            matches!(out, [Done::Added(..), ..]) && out.iter().any(|d| matches!(d, Done::Placed(_)))
        };
        assert!(ok(&one) && ok(&two), "{one:?} {two:?}");
        assert!(owns(&s, &ea.md5) && owns(&s, &eb.md5));
        downloads(&d).len()
    };
    assert_eq!(run(&[&ea.md5, &eb.md5], &[]), 3);
    assert_eq!(run(&[&ea.md5], &[&eb.md5]), 4);
    // each chosen alone: nothing wanted
    assert_eq!(run(&[], &[]), 4);
}

/// The job downloading a file shows life while its bytes come: one
/// waiting for it does not take it over, however long. Silent through the
/// wait, it is taken over, once: another waiting waits for the taker.
#[test]
fn a_download_that_moves_is_not_taken_over() {
    let s = session(&Arc::new(Mem::default()));
    let d = Canned::default();
    for _ in 0..2 {
        d.file(PACK, Ok(Served::bytes(pack(), None, None)));
    }
    let cx = Context::default();
    let (p1, p2) = (Progress::new(), Progress::new());
    let mut one = s.job(entry(1), &cx.with_progress(p1.clone()));
    let mut two = s.job(entry(2), &cx.with_progress(p2.clone()));
    let mut three = s.job(entry(2), &cx);
    let s1 = one.start();
    assert!(matches!(&s1, Step::Do(Effect::Download { .. })));
    let (mut s2, mut s3) = (two.start(), three.start());
    assert_eq!(p2.stage(), crate::progress::Stage::Waiting);
    for i in 0..3 * BUSY_ASKS {
        assert!(waits(&s2) && waits(&s3), "{i}: {s2:?} {s3:?}");
        if i % 100 == 0 {
            p1.add(1);
        }
        (s2, s3) = (two.feed(Fed::Woke), three.feed(Fed::Woke));
    }
    // silent from here
    while waits(&s2) {
        assert!(waits(&s3));
        s2 = two.feed(Fed::Woke);
        if waits(&s2) {
            s3 = three.feed(Fed::Woke);
        }
    }
    assert!(matches!(&s2, Step::Do(Effect::Download { .. })), "{s2:?}");
    for _ in 0..BUSY_ASKS - 1 {
        s3 = three.feed(Fed::Woke);
        assert!(waits(&s3), "{s3:?}");
    }
    let outs = [
        done(next(&mut one, s1, &d)),
        done(next(&mut two, s2, &d)),
        done(next(&mut three, s3, &d)),
    ];
    let states: Vec<&str> = outs.iter().map(|o| o[0].state()).collect();
    assert_eq!(states, ["added", "owned", "owned"], "{outs:?}");
    assert_eq!(downloads(&d).len(), 2);
}

/// The wait is for one job: a job that claimed the file since (the first
/// cancelled) is waited for as long.
#[test]
fn the_wait_is_counted_per_holder() {
    let s = session(&Arc::new(Mem::default()));
    let p = Progress::new();
    let cx = Context::default();
    let mut one = s.job(entry(1), &cx.with_progress(p.clone()));
    let mut two = s.job(entry(2), &cx);
    assert!(matches!(one.start(), Step::Do(Effect::Download { .. })));
    let mut s2 = two.start();
    for _ in 0..BUSY_ASKS - 2 {
        assert!(waits(&s2));
        s2 = two.feed(Fed::Woke);
    }
    p.cancel();
    let _ = done(one.feed(Fed::Downloaded(Err(NetError::Cancelled.into()))));
    let third = Entry {
        md5: md5_hex(chart("Pack [3]").as_bytes()),
        ..entry(1)
    };
    let mut three = s.job(third, &cx);
    assert!(matches!(three.start(), Step::Do(Effect::Download { .. })));
    for _ in 0..=BUSY_ASKS {
        assert!(waits(&s2), "{s2:?}");
        s2 = two.feed(Fed::Woke);
    }
    assert!(matches!(&s2, Step::Do(Effect::Download { .. })), "{s2:?}");
}

/// A job that took a file over and whose download fails (a host that
/// refuses a second download at once) waits for the first again, keeping
/// no failure for the file: the first brings it, both charts in. If the
/// first never shows life again, it ends with its own failure, the file
/// not taken over twice. A taker cancelled leaves the file to the first.
#[test]
fn a_taker_that_fails_waits_for_the_first() {
    let take_over = |two: &mut SessionJob| {
        let mut s2 = two.start();
        while waits(&s2) {
            s2 = two.feed(Fed::Woke);
        }
        assert!(matches!(&s2, Step::Do(Effect::Download { .. })), "{s2:?}");
    };
    let failed = || Fed::Downloaded(Err(NetError::Status(404).into()));
    let cx = Context::default();
    // the first brings it
    let s = session(&Arc::new(Mem::default()));
    let (mut one, mut two) = (s.job(entry(1), &cx), s.job(entry(2), &cx));
    assert!(matches!(one.start(), Step::Do(Effect::Download { .. })));
    take_over(&mut two);
    let s2 = two.feed(failed());
    assert!(waits(&s2), "{s2:?}");
    let dead =
        s.0.state
            .lock()
            .unwrap()
            .batch
            .dead
            .lock()
            .unwrap()
            .is_empty();
    assert!(dead, "the failure kept for the file");
    let first = done(one.feed(Fed::Downloaded(Ok(Served::bytes(pack(), None, None)))));
    assert!(matches!(&first[..], [Done::Added(..)]), "{first:?}");
    let second = done(two.feed(Fed::Woke));
    assert!(matches!(&second[..], [Done::Owned(_)]), "{second:?}");
    assert!(owns_both(&s));
    // the first silent for good
    let s = session(&Arc::new(Mem::default()));
    let (mut one, mut two) = (s.job(entry(1), &cx), s.job(entry(2), &cx));
    assert!(matches!(one.start(), Step::Do(Effect::Download { .. })));
    take_over(&mut two);
    let mut s2 = two.feed(failed());
    let mut asked = 0;
    while waits(&s2) {
        asked += 1;
        s2 = two.feed(Fed::Woke);
    }
    assert_eq!(asked, BUSY_ASKS);
    let second = done(s2);
    assert!(
        matches!(
            &second[..],
            [Done::Failed(Failure::Net(NetError::Status(404)))]
        ),
        "{second:?}"
    );
    // the taker cancelled: a third job waits for the first
    let s = session(&Arc::new(Mem::default()));
    let p = Progress::new();
    let mut one = s.job(entry(1), &cx);
    let mut two = s.job(entry(2), &cx.with_progress(p.clone()));
    assert!(matches!(one.start(), Step::Do(Effect::Download { .. })));
    take_over(&mut two);
    p.cancel();
    let second = done(two.feed(Fed::Downloaded(Err(NetError::Cancelled.into()))));
    assert!(
        matches!(&second[..], [Done::Failed(f)] if f.cancelled()),
        "{second:?}"
    );
    let third = Entry {
        md5: md5_hex(chart("Pack [3]").as_bytes()),
        ..entry(1)
    };
    let s3 = s.job(third, &cx).start();
    assert!(waits(&s3), "{s3:?}");
}

/// Entries without a hash sharing a pack: the second ends by what the
/// first's take found for it, its song's folder (as its own take of the
/// pack would say), not "no link works".
#[test]
fn entries_without_a_hash_sharing_a_pack() {
    let s = session(&Arc::new(Mem::default()));
    let d = Canned::default();
    d.file(PACK, Ok(Served::bytes(pack(), None, None)));
    let cx = Context::default();
    let bare = |n: u8| Entry {
        md5: String::new(),
        ..entry(n)
    };
    let (mut one, mut two) = (s.job(bare(1), &cx), s.job(bare(2), &cx));
    let (s1, s2) = (one.start(), two.start());
    assert!(waits(&s2));
    let first = done(next(&mut one, s1, &d));
    let second = done(next(&mut two, s2, &d));
    let [Done::Added(dir, _)] = &first[..] else {
        panic!("{first:?}")
    };
    assert!(
        matches!(&second[..], [Done::Owned(d)] if d.starts_with(dir)),
        "{second:?}"
    );
    assert!(owns_both(&s));
    assert_eq!(downloads(&d).len(), 1);
}

/// A link that failed over `http://` does not fail the entry whose
/// `https://` link to it the server serves.
#[test]
fn a_failed_http_link_does_not_fail_the_https_one() {
    let s = session(&Arc::new(Mem::default()));
    let d = Canned::default();
    d.file(PACK, Ok(Served::bytes(pack(), None, None)));
    let cx = Context::default();
    let http = PACK.replace("https://", "http://");
    let first = crate::net::job::run_job(&mut s.job(entry_at(2, &http), &cx), &d);
    assert!(
        matches!(
            &first[..],
            [Done::Failed(Failure::Net(NetError::Status(404)))]
        ),
        "{first:?}"
    );
    let second = crate::net::job::run_job(&mut s.job(entry(1), &cx), &d);
    assert!(matches!(&second[..], [Done::Added(..)]), "{second:?}");
    assert_eq!(
        downloads(&d),
        [format!("download {http}"), format!("download {PACK}")]
    );
}

/// The second job waits for a file another downloads, which came but
/// brought nothing (cut off): it is told so and tries its next link.
#[test]
fn busy_then_had_then_the_next_link() {
    const CUT: &str = "https://p.test/cut.zip";
    let s = session(&Arc::new(Mem::default()));
    let d = Canned::default();
    let mut cut = pack();
    cut.truncate(cut.len() / 2);
    d.file(CUT, Ok(Served::bytes(cut, None, None)));
    d.file(PACK, Ok(Served::bytes(pack(), None, None)));
    let cx = Context::default();
    let two = Entry {
        song: vec![CUT.into(), PACK.into()],
        ..entry(2)
    };
    let (mut one, mut two) = (s.job(entry_at(1, CUT), &cx), s.job(two, &cx));
    let (s1, s2) = (one.start(), two.start());
    assert!(waits(&s2), "{s2:?}");
    let first = done(next(&mut one, s1, &d));
    assert!(
        matches!(&first[..], [Done::Failed(Failure::Other(_))]),
        "{first:?}"
    );
    let s2 = next(&mut two, s2, &d);
    assert!(matches!(&s2, Step::Do(Effect::Download { .. })), "{s2:?}");
    let second = done(next(&mut two, s2, &d));
    assert!(matches!(&second[..], [Done::Added(..)]), "{second:?}");
    assert_eq!(
        downloads(&d),
        [format!("download {CUT}"), format!("download {PACK}")]
    );
}

/// A separate chart another job's take left waiting for its song (the
/// library's folder of the song did not take it): this job is told it
/// waits, so its song's step comes next; the song brings both in.
#[test]
fn a_chart_left_waiting_by_another_job() {
    const SONG: &str = "#TITLE Song [N]\n#ARTIST alpha\n#WAV01 01.wav\n";
    let m = Arc::new(Mem::default());
    m.put(Path::new("bms/Song/n.bms"), SONG.as_bytes(), 0);
    m.put(Path::new("bms/Song/01.ogg"), b"x", 0);
    let zeta = |d: &str| format!("#TITLE Zeta [{d}]\n#ARTIST omega\n#WAV01 q.wav\n");
    let (zx, zy) = (zeta("X"), zeta("Y"));
    let e = |body: &str, title: &str| Entry {
        md5: md5_hex(body.as_bytes()),
        want: Want {
            title: title.into(),
            artist: "omega".into(),
        },
        // (the song is the library's by a chart the table names)
        org_md5: vec![md5_hex(SONG.as_bytes())],
        song: vec!["https://e.test/zeta.zip".into()],
        chart: vec!["https://e.test/charts.zip".into()],
        ..Default::default()
    };
    let (ex, ey) = (e(&zx, "Zeta [X]"), e(&zy, "Zeta [Y]"));
    let d = Canned::default();
    d.file(
        "https://e.test/charts.zip",
        Ok(Served::bytes(
            zip(&[("c/zx.bms", &zx), ("c/zy.bms", &zy)]),
            None,
            None,
        )),
    );
    let song = zip(&[
        (
            "Zeta/n.bms",
            "#TITLE Zeta [N]\n#ARTIST omega\n#WAV01 q.wav\n",
        ),
        ("Zeta/q.ogg", "x"),
    ]);
    d.file(
        "https://e.test/zeta.zip",
        Ok(Served::bytes(song, None, None)),
    );
    let lib = Library::scan(&*m, Path::new("bms"));
    let s = Session::new(m.clone(), "dl".into(), lib, Sources::default());
    s.want([ex.md5.clone(), ey.md5.clone()]);
    let cx = Context::default();
    let (mut one, mut two) = (s.job(ex.clone(), &cx), s.job(ey.clone(), &cx));
    let s1 = one.start();
    let s2 = two.start();
    assert!(waits(&s2), "{s2:?}");
    // the charts come, both wait; the first goes on to the song
    let s1 = next(&mut one, s1, &d);
    assert!(matches!(&s1, Step::Do(Effect::Download { .. })), "{s1:?}");
    let s2 = next(&mut two, s2, &d);
    assert!(waits(&s2), "{s2:?}");
    let first = done(next(&mut one, s1, &d));
    let second = done(next(&mut two, s2, &d));
    assert!(
        matches!(&second[..], [Done::Waiting(_), Done::Owned(_)]),
        "{first:?} {second:?}"
    );
    assert!(owns(&s, &ex.md5) && owns(&s, &ey.md5));
    assert_eq!(downloads(&d).len(), 2);
}
