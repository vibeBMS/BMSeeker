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

fn owns_both(s: &Session) -> bool {
    s.with_library(|l| l.owns(&entry(1).md5) && l.owns(&entry(2).md5))
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
    assert!(s.with_library(|l| l.owns(&x.md5)));
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

/// A job that panicked with the library: the session's jobs end at once.
#[cfg(not(target_arch = "wasm32"))]
#[test]
fn a_lost_library_ends_the_jobs() {
    let s = session(&Arc::new(Mem::default()));
    let held = s.clone();
    let panicked = std::thread::spawn(move || {
        let _lent = held.0.state.lock();
        panic!("a step that panics");
    })
    .join();
    assert!(panicked.is_err());
    let mut job = s.job(entry(1), &Context::default());
    let out = done(job.start());
    assert!(
        matches!(&out[..], [Done::Failed(Failure::Other(e))] if e == LOST),
        "{out:?}"
    );
}
