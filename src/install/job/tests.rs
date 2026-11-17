use super::*;
use crate::hosts::{Host, Hosts, Resolved};
use crate::net::{BoxTask, Ready};
use crate::progress::Progress;
use crate::testkit::zip;
use std::cell::RefCell;
use std::sync::Arc;

/// A driver of canned answers: requests by URL; downloads by URL, one
/// answer after another; what was done, kept.
#[derive(Default)]
pub(crate) struct Canned {
    pub http: Vec<(String, Answer)>,
    pub files: RefCell<Vec<(String, Result<Served, Unserved>)>>,
    pub done: RefCell<Vec<String>>,
    /// Cancel this progress at the first download.
    pub cancel: Option<Arc<Progress>>,
}

impl Canned {
    pub fn file(&self, url: &str, got: Result<Served, Unserved>) {
        self.files.borrow_mut().push((url.into(), got));
    }
}

impl Driver for Canned {
    fn perform(&self, effect: Effect) -> Fed {
        match effect {
            Effect::Http(r) => {
                self.done.borrow_mut().push(format!("get {}", r.url()));
                let a = self.http.iter().find(|(u, _)| u == r.url());
                Fed::Http(a.map_or_else(
                    || Err(format!("no answer for {}", r.url()).into()),
                    |a| a.1.clone(),
                ))
            }
            Effect::Sleep(d) => {
                self.done
                    .borrow_mut()
                    .push(format!("sleep {}", d.as_secs()));
                Fed::Woke
            }
            Effect::Download { target, .. } => {
                self.done
                    .borrow_mut()
                    .push(format!("download {}", target.url));
                if let Some(p) = &self.cancel {
                    p.cancel();
                    return Fed::Downloaded(Err(NetError::Cancelled.into()));
                }
                let mut files = self.files.borrow_mut();
                match files.iter().position(|(u, _)| *u == target.url) {
                    Some(i) => Fed::Downloaded(files.remove(i).1),
                    None => Fed::Downloaded(Err(NetError::Status(404).into())),
                }
            }
        }
    }
}

pub(crate) const ZIP: &[u8] = b"PK\x05\x06\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0";

pub(crate) fn zip_served() -> Result<Served, Unserved> {
    Ok(Served::bytes(ZIP.to_vec(), None, None))
}

fn page(at: Option<&str>) -> Result<Served, Unserved> {
    Ok(Served::bytes(
        b"<html>a page</html>".to_vec(),
        None,
        at.map(String::from),
    ))
}

fn status(n: u16) -> Result<Served, Unserved> {
    Err(NetError::Status(n).into())
}

fn acquire(link: &str, cx: &Context, d: &Canned) -> Result<Got, Fetch> {
    run_job(&mut Acquire::new(link, None, false, "slot".into(), cx), d)
}

fn steps(d: &Canned) -> Vec<String> {
    d.done.borrow().clone()
}

#[test]
fn a_file_and_its_name() {
    let d = Canned::default();
    d.file("http://e.test/a%20b.zip", zip_served());
    let got = acquire("http://e.test/a%20b.zip", &Context::default(), &d).unwrap();
    assert_eq!(got.name, "a b.zip");
    assert!(matches!(got.blob, Blob::Bytes(b) if b == ZIP));
    // the server's name over the URL's
    let d = Canned::default();
    d.file(
        "http://e.test/dl?id=1",
        Ok(Served::bytes(ZIP.to_vec(), Some("曲.zip".into()), None)),
    );
    assert_eq!(
        acquire("http://e.test/dl?id=1", &Context::default(), &d)
            .unwrap()
            .name,
        "曲.zip"
    );
}

#[test]
fn a_page_is_a_page() {
    let d = Canned::default();
    d.file("http://e.test/a", page(None));
    assert!(matches!(
        acquire("http://e.test/a", &Context::default(), &d),
        Err(Fetch::Page)
    ));
    // wanted as it is (a chart, a movie): kept unless a page
    let d = Canned::default();
    d.file(
        "http://e.test/a.bms",
        Ok(Served::bytes(b"#TITLE x".to_vec(), None, None)),
    );
    let mut j = Acquire::new(
        "http://e.test/a.bms",
        None,
        true,
        "slot".into(),
        &Context::default(),
    );
    assert_eq!(run_job(&mut j, &d).unwrap().name, "a.bms");
    // an empty file: no page, a failure
    let d = Canned::default();
    d.file(
        "http://e.test/a.bms",
        Ok(Served::bytes(Vec::new(), None, None)),
    );
    let mut j = Acquire::new(
        "http://e.test/a.bms",
        None,
        true,
        "slot".into(),
        &Context::default(),
    );
    assert!(matches!(
        run_job(&mut j, &d),
        Err(Fetch::Failed(NetError::Other(_)))
    ));
}

/// A server's passing failure: tried again after 3 s, then 10 s; then
/// given up.
#[test]
fn passing_failures_tried_again() {
    let link = "http://e.test/a.zip";
    let d = Canned::default();
    d.file(link, status(429));
    d.file(link, status(503));
    d.file(link, zip_served());
    assert!(acquire(link, &Context::default(), &d).is_ok());
    let fetched = format!("download {link}");
    assert_eq!(
        steps(&d),
        [&fetched, "sleep 3", &fetched, "sleep 10", &fetched]
    );
    let d = Canned::default();
    for _ in 0..3 {
        d.file(link, status(502));
    }
    let got = acquire(link, &Context::default(), &d);
    assert!(
        matches!(got, Err(Fetch::Failed(NetError::Status(502)))),
        "{got:?}"
    );
    // a file that is not there: not again
    let d = Canned::default();
    d.file(link, status(404));
    assert!(matches!(
        acquire(link, &Context::default(), &d),
        Err(Fetch::Failed(NetError::Status(404)))
    ));
    assert_eq!(steps(&d), [fetched]);
}

/// Silence for a minute (the transport's stall) is a failure, not tried
/// again; a cancel ends it without waiting.
#[test]
fn stalled_and_cancelled() {
    let link = "http://e.test/a.zip";
    let d = Canned::default();
    d.file(link, Err(NetError::Stalled(60).into()));
    assert!(matches!(
        acquire(link, &Context::default(), &d),
        Err(Fetch::Failed(NetError::Stalled(60)))
    ));
    let p = Progress::new();
    let cx = Context::default().with_progress(p.clone());
    let d = Canned {
        cancel: Some(p),
        ..Default::default()
    };
    assert!(matches!(
        acquire(link, &cx, &d),
        Err(Fetch::Failed(NetError::Cancelled))
    ));
    assert_eq!(steps(&d), [format!("download {link}")]);
    // a passing failure once cancelled: no wait
    let p = Progress::new();
    p.cancel();
    let cx = Context::default().with_progress(p);
    let d = Canned::default();
    d.file(link, status(503));
    assert!(acquire(link, &cx, &d).is_err());
    assert!(!steps(&d).iter().any(|s| s.starts_with("sleep")));
}

#[test]
fn drive_quota() {
    let link = "https://drive.google.com/file/d/ID1/view";
    let file = "https://drive.usercontent.google.com/download?id=ID1&export=download&confirm=t";
    let d = Canned::default();
    d.file(
        file,
        Ok(Served::bytes(
            b"<html><title>Google Drive - Quota exceeded</title>Quota exceeded".to_vec(),
            None,
            None,
        )),
    );
    let got = acquire(link, &Context::default(), &d);
    assert!(
        matches!(&got, Err(Fetch::Failed(NetError::Other(e))) if e == crate::hosts::DRIVE_QUOTA),
        "{got:?}"
    );
}

/// A short link whose redirects end at a host bmseeker knows: opened from
/// there (twice at most).
#[test]
fn rehosted() {
    let short = "https://bit.ly/x";
    let drive = "https://drive.google.com/file/d/ID2/view";
    let file = "https://drive.usercontent.google.com/download?id=ID2&export=download&confirm=t";
    let d = Canned::default();
    d.file(short, page(Some(drive)));
    d.file(file, zip_served());
    assert!(
        acquire(short, &Context::default(), &d).is_ok(),
        "{:?}",
        steps(&d)
    );
    assert_eq!(
        steps(&d),
        [format!("download {short}"), format!("download {file}")]
    );
    // the same host, or a server that sends the file itself: a page
    let d = Canned::default();
    d.file(short, page(Some("https://e.test/elsewhere")));
    assert!(matches!(
        acquire(short, &Context::default(), &d),
        Err(Fetch::Page)
    ));
}

/// A venue whose page names an author's link and a mirror.
struct Venue;

impl Host for Venue {
    fn name(&self) -> &'static str {
        "venue"
    }
    fn accepts(&self, link: &Link) -> bool {
        link.on("venue.test")
    }
    fn open(&self, _: &Link, _: Option<&Want>, _: &crate::config::Config) -> BoxTask<Resolved> {
        Ready::boxed(Resolved::Follow(vec![
            "http://author.test/a.zip".into(),
            "http://mirror.test/a.zip".into(),
        ]))
    }
}

/// The author's file does not come: the mirror the page named.
#[test]
fn a_mirror_next() {
    let mut hosts = Hosts::default();
    hosts.first(Box::new(Venue));
    let cx = Context {
        hosts: Arc::new(hosts),
        ..Default::default()
    };
    let d = Canned::default();
    d.file("http://mirror.test/a.zip", zip_served());
    assert!(acquire("https://venue.test/work/1", &cx, &d).is_ok());
    assert_eq!(
        steps(&d),
        [
            "download http://author.test/a.zip",
            "download http://mirror.test/a.zip"
        ]
    );
    // none comes: the last failure
    let d = Canned::default();
    assert!(matches!(
        acquire("https://venue.test/work/1", &cx, &d),
        Err(Fetch::Failed(NetError::Status(404)))
    ));
}

/// A web page's proxy refuses what is not an archive: a failure is a page
/// (and opened from where its redirects ended), but a cancel is a cancel.
#[test]
fn failures_as_pages() {
    let web = |link: &str, d: &Canned| {
        let mut j =
            Acquire::new(link, None, false, "".into(), &Context::default()).failures_are_pages();
        run_job(&mut j, d)
    };
    let d = Canned::default();
    d.file("http://e.test/a", status(404));
    assert!(matches!(web("http://e.test/a", &d), Err(Fetch::Page)));
    let short = "https://bit.ly/y";
    let drive = "https://drive.google.com/file/d/ID3/view";
    let file = "https://drive.usercontent.google.com/download?id=ID3&export=download&confirm=t";
    let d = Canned::default();
    d.file(
        short,
        Err(Unserved {
            error: NetError::Status(403),
            at: Some(drive.into()),
        }),
    );
    d.file(file, zip_served());
    assert!(web(short, &d).is_ok());
    let d = Canned::default();
    d.file("http://e.test/a", Err(NetError::Cancelled.into()));
    assert!(matches!(
        web("http://e.test/a", &d),
        Err(Fetch::Failed(NetError::Cancelled))
    ));
}

/// What a link brings, through `take`, into a library in memory.
#[test]
fn acquired_then_taken() {
    use crate::fs::Mem;
    use crate::install::{take, Done, Request, Role};
    use crate::library::Library;
    let z = zip(&[("Song/a.bms", SONG), ("Song/01.ogg", "x")]);
    let d = Canned::default();
    d.file("http://e.test/song.zip", Ok(Served::bytes(z, None, None)));
    let got = acquire("http://e.test/song.zip", &Context::default(), &d).unwrap();
    let m = Mem::default();
    let mut lib = Library::scan(&m, Path::new("bms"));
    let req = Request {
        role: Role::Song,
        file: Some(got.name),
        ..Default::default()
    };
    let done = take(&m, got.blob, &req, Path::new("dl"), &mut lib);
    assert!(
        matches!(&done[..], [Done::Added(p, 2)] if p == Path::new("bms/song")),
        "{done:?}"
    );
}

#[test]
fn a_task_as_a_job() {
    let mut j = TaskJob(crate::net::get_then("http://e.test/t".into(), |a| {
        a.is_ok()
    }));
    let d = Canned {
        http: vec![("http://e.test/t".into(), Ok(b"x".to_vec()))],
        ..Default::default()
    };
    assert!(run_job(&mut j, &d));
}

/// Files one after another: a mirror's as it is (a page too), an author's
/// through the hosts; the first that does not come ends it.
#[test]
fn files_in_order() {
    let sources = vec![
        FileLink {
            url: "http://author.test/a.zip".into(),
            plain: false,
        },
        FileLink {
            url: "http://mirror.test/b.mp4".into(),
            plain: true,
        },
    ];
    let d = Canned::default();
    d.file("http://author.test/a.zip", zip_served());
    d.file("http://mirror.test/b.mp4", page(None));
    let mut j = Files::new(sources.clone(), "dl/.w".into(), &Context::default());
    let got = run_job(&mut j, &d).unwrap();
    let names: Vec<&str> = got.iter().map(|g| g.name.as_str()).collect();
    assert_eq!(names, ["a.zip", "b.mp4"]);
    assert_eq!(
        Files::slot(Path::new("dl/.w"), 1),
        Path::new("dl/.w-1.part")
    );
    // the first that does not come
    let d = Canned::default();
    d.file("http://author.test/a.zip", page(None));
    let missed = run_job(
        &mut Files::new(sources, "s".into(), &Context::default()),
        &d,
    );
    assert_eq!(
        missed.unwrap_err(),
        Missed {
            index: 0,
            url: "http://author.test/a.zip".into(),
            fetch: Fetch::Page
        }
    );
    assert_eq!(steps(&d), ["download http://author.test/a.zip"]);
}

/// A source that knows these links (found by name, if `checked`).
struct Known(Vec<String>, bool);

impl crate::sources::Source for Known {
    fn name(&self) -> &'static str {
        "known"
    }
    fn find(&self, _: &crate::sources::Query) -> BoxTask<Vec<String>> {
        Ready::boxed(self.0.clone())
    }
    fn checked(&self) -> bool {
        self.1
    }
}

const SONG: &str = "#TITLE Song\n#ARTIST x\n#WAV01 01.wav\n";

fn install(
    m: &Arc<crate::fs::Mem>,
    q: &crate::sources::Query,
    own: &[&str],
    sources: crate::sources::Sources,
    batch: crate::install::Batch,
    cx: &Context,
    d: &Canned,
) -> Installed {
    let store: Arc<dyn crate::fs::Store + Send + Sync> = m.clone();
    let lib = crate::library::Library::scan(&**m, Path::new("bms"));
    let own = own.iter().map(|s| s.to_string()).collect();
    let mut job = Install::new(store, "dl".into(), lib, batch, q.clone(), own, sources, cx);
    run_job(&mut job, d)
}

fn song_query(md5: &str) -> crate::sources::Query {
    crate::sources::Query {
        md5: md5.into(),
        want: Want {
            title: "Song".into(),
            artist: "x".into(),
        },
        ..Default::default()
    }
}

/// Its own links in order, then the sources' (found by name: checked);
/// the first that brings it in ends it; what passed through goes.
#[test]
fn install_tries_links_in_order() {
    use crate::install::{md5_hex, Batch, Done, Failure};
    let m = Arc::new(crate::fs::Mem::default());
    let d = Canned::default();
    // its own: a page, then nothing there
    d.file("http://e.test/page", page(None));
    // a source's, found by name: another song's archive, then the song's
    let other = zip(&[("o.bms", "#TITLE Other\n#ARTIST y\n"), ("01.ogg", "x")]);
    d.file(
        "http://found.test/other.zip",
        Ok(Served::bytes(other, None, None)),
    );
    let song = zip(&[("Song/a.bms", SONG), ("Song/01.ogg", "x")]);
    d.file(
        "http://found.test/song.zip",
        Ok(Served::bytes(song, None, None)),
    );
    let mut sources = crate::sources::Sources::default();
    sources.push(Box::new(Known(
        vec![
            "http://found.test/other.zip".into(),
            "http://found.test/song.zip".into(),
        ],
        true,
    )));
    let q = song_query(&md5_hex(SONG.as_bytes()));
    let own = ["http://e.test/page", "http://e.test/gone.zip"];
    let out = install(
        &m,
        &q,
        &own,
        sources,
        Batch::default(),
        &Context::default(),
        &d,
    );
    assert!(
        matches!(&out.done[..], [Done::Added(p, 2)] if p == Path::new("bms/Song")),
        "{:?}",
        out.done
    );
    assert!(out.lib.owns(&q.md5));
    let left: Vec<PathBuf> = m.0.lock().unwrap().keys().cloned().collect();
    assert!(!left.iter().any(|p| p.starts_with("dl")), "{left:?}");
    // nothing brings it: the first page
    let d = Canned::default();
    d.file("http://e.test/page", page(None));
    let out = install(
        &Arc::new(crate::fs::Mem::default()),
        &q,
        &own,
        Default::default(),
        Batch::default(),
        &Context::default(),
        &d,
    );
    assert!(matches!(&out.done[..], [Done::Page(u)] if u == "http://e.test/page"));
    // a chart's own link that is a page
    let d = Canned::default();
    d.file("http://e.test/x.bms", page(None));
    let mut diff = q.clone();
    diff.diff = true;
    let out = install(
        &Arc::new(crate::fs::Mem::default()),
        &diff,
        &["http://e.test/x.bms"],
        Default::default(),
        Batch::default(),
        &Context::default(),
        &d,
    );
    assert!(matches!(&out.done[..], [Done::Page(_)]), "{:?}", out.done);
    // no link at all
    let out = install(
        &Arc::new(crate::fs::Mem::default()),
        &q,
        &[],
        Default::default(),
        Batch::default(),
        &Context::default(),
        &Canned::default(),
    );
    assert!(matches!(&out.done[..], [Done::Failed(Failure::NoLink)]));
}

/// A file the batch has had is not downloaded again; a cancel ends it.
#[test]
fn install_in_a_batch_and_cancelled() {
    use crate::install::{Batch, Done};
    let q = song_query("");
    let d = Canned::default();
    let link = "https://drive.google.com/file/d/ID9/view";
    let mut batch = Batch::default();
    batch.tried(Batch::key(link, &q), &[Done::Added("bms/S".into(), 1)]);
    let other = "https://drive.google.com/uc?id=ID9";
    let out = install(
        &Arc::new(crate::fs::Mem::default()),
        &q,
        &[other],
        Default::default(),
        batch,
        &Context::default(),
        &d,
    );
    assert!(steps(&d).is_empty());
    assert!(matches!(
        &out.done[..],
        [Done::Failed(crate::install::Failure::Downloaded)]
    ));
    assert!(out.batch.has(&Batch::key(link, &q)));
    let p = Progress::new();
    p.cancel();
    let cx = Context::default().with_progress(p);
    let out = install(
        &Arc::new(crate::fs::Mem::default()),
        &q,
        &["http://e.test/a.zip"],
        Default::default(),
        Batch::default(),
        &cx,
        &Canned::default(),
    );
    assert!(matches!(&out.done[..], [Done::Failed(f)] if f.cancelled()));
}

use std::path::{Path, PathBuf};
