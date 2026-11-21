use super::*;
use crate::config::Config;
use crate::hosts::Target;
use crate::install::job::{Served, TaskJob, Unserved};
use crate::net::{Kind, Request};
use crate::testkit::{serve, tmp, Reply};

/// `url`'s body, through `n`.
fn get(n: &Native, url: &str) -> crate::net::Answer {
    let page = crate::net::get_as(url.to_string(), Kind::Page, |a| a);
    run_job(&mut TaskJob::new(page), n)
}

/// The settings that ask the tests' servers, which are on this machine.
fn local() -> Context {
    Context::new(Config {
        local_hosts: true,
        ..Default::default()
    })
}

/// The settings as they are unless they say otherwise: hosts of this
/// machine and its networks are not asked.
fn public_only() -> Context {
    Context::new(Config {
        local_hosts: false,
        ..Default::default()
    })
}

/// What `n` makes of downloading `url` into `slot`.
fn download(n: &Native, url: String, slot: &Path) -> Result<Served, Unserved> {
    let effect = Effect::Download {
        target: Target::url(url),
        slot: slot.to_path_buf(),
    };
    match n.perform(effect) {
        Fed::Downloaded(d) => d,
        fed => panic!("{fed:?}"),
    }
}

#[test]
fn file_names() {
    assert_eq!(
        crate::hosts::url_file_name("https://h.example/d/%E6%9B%B2%20x.7z?dl=1").as_deref(),
        Some("曲 x.7z")
    );
    assert_eq!(crate::hosts::url_file_name("https://h.example/"), None);
}

/// Requests to one host share a connection.
#[test]
fn one_connection() {
    let s = serve(vec![("/a", Reply::ok("1")), ("/b", Reply::ok("22"))]);
    let n = Native::new(&local());
    for p in ["/a", "/b", "/a"] {
        let page = Request::Get(format!("{}{p}", s.url), Kind::Page);
        let got = n.perform(Effect::Http(page));
        assert!(matches!(got, Fed::Http(Ok(b)) if !b.is_empty()));
    }
    assert_eq!(s.connections(), 1);
    assert_eq!(s.asked(), ["/a", "/b", "/a"]);
}

/// A page request the server is too busy for once is asked again.
#[test]
fn busy_then_answered() {
    let s = serve(vec![
        ("/t", Reply::status(503, "")),
        ("/t", Reply::ok("table")),
        ("/gone", Reply::status(404, "")),
    ]);
    let n = Native::new(&local());
    let get = |p: &str| get(&n, &format!("{}{p}", s.url));
    let t = std::time::Instant::now();
    assert_eq!(get("/t"), Ok(b"table".to_vec()));
    assert!(t.elapsed() >= Duration::from_secs(3));
    assert_eq!(get("/gone"), Err(NetError::Status(404)));
    assert_eq!(s.asked(), ["/t", "/t", "/gone"]);
}

/// A download: where its redirects ended, its name, its head; a MEGA file
/// decrypted as it comes.
#[test]
fn downloads() {
    let plain: Vec<u8> = (0..200_000u32).map(|i| (i % 253) as u8).collect();
    let (key, sent) = crate::hosts::mega_file([3; 16], [5; 8], &plain);
    let mut changed = sent.clone();
    changed[150_000] ^= 0x80;
    let s = serve(vec![
        ("/r", Reply::status(302, "").header("Location", "/f.zip")),
        (
            "/f.zip",
            Reply::ok("PK file").header("Content-Disposition", "attachment; filename=\"曲.zip\""),
        ),
        ("/mega", Reply::ok(sent)),
        ("/changed", Reply::ok(changed)),
    ]);
    let dir = tmp("native-dl");
    let n = Native::new(&local());
    let download = |url: String, mega_key| -> Served {
        let target = Target {
            mega_key,
            ..Target::url(url)
        };
        let slot = dir.join("slot");
        match n.perform(Effect::Download { target, slot }) {
            Fed::Downloaded(Ok(s)) => s,
            _ => panic!("no file"),
        }
    };
    let got = download(format!("{}/r", s.url), None);
    assert_eq!(got.at, Some(format!("{}/f.zip", s.url)));
    assert_eq!(got.name.as_deref(), Some("曲.zip"));
    assert_eq!(got.head, b"PK file");
    // a MEGA file: decrypted, and the file its key was made for
    let got = download(format!("{}/mega", s.url), Some(key.clone()));
    assert_eq!((got.name, got.at), (None, None));
    assert_eq!(got.head, plain[..crate::unpack::HEAD]);
    let crate::unpack::Blob::File(f) = &got.blob else {
        panic!("not a file")
    };
    assert_eq!(std::fs::read(f).unwrap(), plain);
    // one bit of it changed on the way: no file, and none left
    let target = Target {
        mega_key: Some(key),
        ..Target::url(format!("{}/changed", s.url))
    };
    let slot = dir.join("changed");
    let Fed::Downloaded(Err(e)) = n.perform(Effect::Download {
        target,
        slot: slot.clone(),
    }) else {
        panic!("a changed file came")
    };
    assert_eq!(e.error, NetError::Other(crate::hosts::MEGA_CHANGED.into()));
    assert!(!slot.exists());
}

/// A driver's downloads from one host share a connection; a big file's
/// head is its first [`HEAD`](crate::unpack::HEAD) bytes.
#[test]
fn downloads_share_a_connection() {
    let big: Vec<u8> = (0..100_000u32).map(|i| i as u8).collect();
    let s = serve(vec![
        ("/a.zip", Reply::ok("PK a")),
        ("/b.zip", Reply::ok(big.clone())),
    ]);
    let dir = tmp("native-dl2");
    let n = Native::new(&local());
    let mut heads = Vec::new();
    for p in ["/a.zip", "/b.zip", "/a.zip"] {
        let effect = Effect::Download {
            target: Target::url(format!("{}{p}", s.url)),
            slot: dir.join("slot"),
        };
        match n.perform(effect) {
            Fed::Downloaded(Ok(got)) => heads.push(got.head),
            _ => panic!("no file"),
        }
    }
    assert_eq!(s.connections(), 1);
    assert_eq!(heads[1], big[..crate::unpack::HEAD]);
}

/// The transport keeps to no refusals; under [`Guarded`] (one guard) a
/// service that refused is not asked again.
#[test]
fn one_guard() {
    let s = serve(vec![("/a", Reply::status(403, "")), ("/b", Reply::ok("b"))]);
    let cx = local();
    cx.refused.earlier(&s.url);
    let n = Native::new(&cx);
    let ask = |d: &dyn Driver, p: &str| {
        d.perform(Effect::Http(Request::Get(
            format!("{}{p}", s.url),
            Kind::Api,
        )))
    };
    assert!(matches!(ask(&n, "/b"), Fed::Http(Ok(_))));
    let cx = Context::default();
    let g = Guarded::new(&n, &cx);
    assert!(matches!(
        ask(&g, "/a"),
        Fed::Http(Err(NetError::Refused(_)))
    ));
    assert!(matches!(
        ask(&g, "/b"),
        Fed::Http(Err(NetError::Refused(_)))
    ));
    assert_eq!(s.asked(), ["/b", "/a"]);
}

/// A server that sends a little and then nothing: cancelling ends it at
/// once, its connection closed.
#[test]
fn a_stalled_download_cancels() {
    use std::io::{Read, Write};
    let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = l.local_addr().unwrap();
    let (closed, closed_rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let (mut c, _) = l.accept().unwrap();
        let mut buf = [0u8; 1024];
        let _ = c.read(&mut buf);
        let _ = c.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 1000000\r\n\r\nPK");
        // (the client hung up: its read ends)
        let _ = c.set_read_timeout(Some(Duration::from_secs(20)));
        let _ = closed.send(c.read(&mut buf).ok());
    });
    let p = Progress::new();
    let p2 = p.clone();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(800));
        p2.cancel();
    });
    let dir = tmp("stall");
    let t = std::time::Instant::now();
    let n = Native::new(&local().with_progress(p.clone()));
    let r = download(&n, format!("http://{addr}/a.zip"), &dir.join("a"));
    assert_eq!(r.map(|_| ()).map_err(|u| u.error), Err(NetError::Cancelled));
    // (what came of it is not left)
    assert!(!dir.join("a").exists());
    assert!(t.elapsed() < Duration::from_secs(5));
    assert_eq!(p.bytes(), (2, Some(1_000_000)));
    let closed = closed_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    assert_eq!(closed, Some(0));
}

/// Once cancelled, no request goes and a wait ends.
#[test]
fn cancelled() {
    let p = Progress::new();
    p.cancel();
    let n = Native::new(&Context::default().with_progress(p));
    let page = Request::Get("http://127.0.0.1:9/".into(), Kind::Page);
    let got = n.perform(Effect::Http(page));
    assert!(matches!(got, Fed::Http(Err(NetError::Cancelled))));
    let t = std::time::Instant::now();
    assert!(matches!(
        n.perform(Effect::Sleep(Duration::from_secs(10))),
        Fed::Woke
    ));
    assert!(t.elapsed() < Duration::from_secs(1));
}

/// `BMSEEKER_FETCH=URL cargo test -p bmseeker fetch_url -- --ignored`
#[test]
#[ignore = "network"]
fn fetch_url() {
    let url = std::env::var("BMSEEKER_FETCH").expect("BMSEEKER_FETCH");
    let out = std::env::temp_dir().join("bmseeker-fetch");
    let cx = Context::default();
    let net = Native::new(&cx);
    match fetch(&url, None, &out, false, &cx, &Guarded::new(&net, &cx)) {
        Ok(name) => println!(
            "OK {name} {:?} {} bytes",
            crate::unpack::sniff_file(&out),
            std::fs::metadata(&out).map(|m| m.len()).unwrap_or(0)
        ),
        Err(e) => println!("ERR {e:?}"),
    }
}

/// The `User-Agent` of the one request `ask` makes to the server at the
/// URL it is given.
fn user_agent_heard(ask: impl FnOnce(String)) -> String {
    use std::io::{BufRead, Write};
    let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/a.zip", l.local_addr().unwrap());
    let heard = std::thread::spawn(move || {
        let (c, _) = l.accept().unwrap();
        let mut r = std::io::BufReader::new(c.try_clone().unwrap());
        let (mut line, mut ua) = (String::new(), String::new());
        while r.read_line(&mut line).unwrap_or(0) > 0 && line != "\r\n" {
            if line.to_ascii_lowercase().starts_with("user-agent:") {
                ua = line["user-agent:".len()..].trim().to_string();
            }
            line.clear();
        }
        let ok = b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nPK";
        (&c).write_all(ok).unwrap();
        ua
    });
    ask(url);
    heard.join().unwrap()
}

/// Every native request names the HTTP library as it names itself (the
/// version built with), then BMSeeker, then the app that embeds it.
#[test]
fn user_agents_sent() {
    use crate::config::{user_agent_with, Config, HTTP_CLIENT, USER_AGENT};
    // ureq's own, unchanged: what HTTP_CLIENT says
    let ureq_own = user_agent_heard(|url| {
        ureq::get(&url).call().unwrap();
    });
    assert_eq!(ureq_own, HTTP_CLIENT, "bump config::HTTP_CLIENT");

    let n = Native::new(&local());
    let sent = user_agent_heard(|url| {
        get(&n, &url).unwrap();
    });
    assert_eq!(sent, format!("{HTTP_CLIENT} {USER_AGENT}"));

    let app = "vibeBMS/0.1.0 (+https://github.com/vibeBMS/vibe)";
    let n = Native::new(&Context::new(Config {
        user_agent: user_agent_with(app),
        local_hosts: true,
        ..Default::default()
    }));
    let dir = tmp("native-ua");
    let sent = user_agent_heard(|url| {
        let target = Target::url(url);
        let slot = dir.join("slot");
        let effect = Effect::Download { target, slot };
        assert!(matches!(n.perform(effect), Fed::Downloaded(Ok(_))));
    });
    assert_eq!(sent, format!("{HTTP_CLIENT} {USER_AGENT} {app}"));
}

/// A host of this machine or its networks is not asked, by its address or
/// its name, unless the settings say so: no request reaches it.
#[test]
fn local_hosts_are_not_asked() {
    let s = serve(vec![
        ("/a", Reply::ok("secret")),
        ("/a.zip", Reply::ok("PK")),
    ]);
    let port = s.url.rsplit(':').next().unwrap();
    let n = Native::new(&public_only());
    let dir = tmp("native-local");
    for host in ["127.0.0.1", "localhost", "127.1", "2130706433", "[::1]"] {
        let e = get(&n, &format!("http://{host}:{port}/a")).unwrap_err();
        assert!(e.to_string().contains("local_hosts"), "{host}: {e}");
        let d = download(&n, format!("http://{host}:{port}/a.zip"), &dir.join("slot"));
        let e = d.unwrap_err().error;
        assert!(e.to_string().contains("local_hosts"), "{host}: {e}");
    }
    assert!(s.asked().is_empty(), "{:?}", s.asked());
    assert!(!dir.join("slot").exists());
    // with the setting: asked
    let n = Native::new(&local());
    assert_eq!(get(&n, &format!("{}/a", s.url)), Ok(b"secret".to_vec()));
}

/// Redirects are followed one at a time: the answer says where they
/// ended, and a service that refused in this run is not reached through
/// one.
#[test]
fn each_redirect_is_kept_to_the_runs_refusals() {
    let s = serve(vec![
        ("/r", Reply::status(302, "").header("Location", "/r2")),
        (
            "/r2",
            Reply::status(307, "").header("Location", "f.zip?x=1"),
        ),
        ("/f.zip?x=1", Reply::ok("PK file")),
        ("/busy", Reply::status(302, "").header("Location", "/limit")),
        ("/limit", Reply::status(429, "")),
        ("/loop", Reply::status(301, "").header("Location", "/loop")),
        (
            "/ftp",
            Reply::status(302, "").header("Location", "ftp://x/y"),
        ),
    ]);
    let dir = tmp("native-hops");
    let at = |p: &str| format!("{}{p}", s.url);
    let cx = local();
    let n = Native::new(&cx);
    let got = download(&n, at("/r"), &dir.join("slot")).unwrap();
    assert_eq!(got.at, Some(at("/f.zip?x=1")));
    assert_eq!(get(&n, &at("/r")), Ok(b"PK file".to_vec()));
    // a refusal says who refused: where the redirects ended
    let e = download(&n, at("/busy"), &dir.join("slot")).unwrap_err();
    assert_eq!((e.error, e.at), (NetError::Status(429), Some(at("/limit"))));
    let e = get(&n, &at("/loop")).unwrap_err();
    assert!(e.to_string().contains("redirects"), "{e}");
    let e = get(&n, &at("/ftp")).unwrap_err();
    assert!(e.to_string().contains("not a web link"), "{e}");
    // the service refused: the address asked is the guard's to keep to,
    // where it leads is not asked
    cx.refused.earlier(&s.url);
    let asked = s.asked().len();
    assert!(matches!(get(&n, &at("/r")), Err(NetError::Refused(_))));
    let e = download(&n, at("/r"), &dir.join("slot")).unwrap_err();
    assert!(matches!(e.error, NetError::Refused(_)));
    assert_eq!(s.asked()[asked..], ["/r", "/r"]);
}

/// An answer is read no further than its kind may be long, however small
/// it was sent.
#[test]
fn answers_are_read_to_their_kinds_limit() {
    use std::io::Write;
    let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
    gz.write_all(&vec![b'1'; (Kind::Api.limit()) + 1]).unwrap();
    let small = gz.finish().unwrap();
    assert!(small.len() < 64 << 10);
    let gzip = |body: &[u8]| Reply::ok(body.to_vec()).header("Content-Encoding", "gzip");
    let s = serve(vec![("/api", gzip(&small)), ("/page", gzip(&small))]);
    let n = Native::new(&local());
    let ask = |p: &str, kind| {
        let r = Request::Get(format!("{}{p}", s.url), kind);
        match n.perform(Effect::Http(r)) {
            Fed::Http(a) => a,
            fed => panic!("{fed:?}"),
        }
    };
    assert_eq!(
        ask("/api", Kind::Api),
        Err(crate::net::too_large(Kind::Api.limit()))
    );
    // (a page may be longer)
    assert_eq!(
        ask("/page", Kind::Page).map(|b| b.len()),
        Ok(Kind::Api.limit() + 1)
    );
}

/// A download's file is made for it alone: what is at its place is not
/// written through, and the file is the user's only.
#[cfg(unix)]
#[test]
fn a_slot_is_made_anew() {
    use std::os::unix::fs::PermissionsExt;
    let s = serve(vec![("/a.zip", Reply::ok("PK new"))]);
    let dir = tmp("native-slot");
    let (kept, slot) = (dir.join("kept"), dir.join("slot"));
    std::fs::write(&kept, "kept").unwrap();
    std::os::unix::fs::symlink(&kept, &slot).unwrap();
    let n = Native::new(&local());
    download(&n, format!("{}/a.zip", s.url), &slot).unwrap();
    assert_eq!(std::fs::read(&kept).unwrap(), b"kept");
    let made = std::fs::symlink_metadata(&slot).unwrap();
    assert!(made.is_file());
    assert_eq!(made.permissions().mode() & 0o777, 0o600);
    assert_eq!(std::fs::read(&slot).unwrap(), b"PK new");
    // an earlier try's file is replaced
    download(&n, format!("{}/a.zip", s.url), &slot).unwrap();
    assert_eq!(std::fs::read(&slot).unwrap(), b"PK new");
    // what is no file (a device, a pipe, a socket) is not removed
    use std::os::unix::fs::FileTypeExt;
    let socket = dir.join("socket");
    let _listening = std::os::unix::net::UnixListener::bind(&socket).unwrap();
    let e = download(&n, format!("{}/a.zip", s.url), &socket).unwrap_err();
    assert!(e.error.to_string().contains("not a file"), "{}", e.error);
    assert!(std::fs::symlink_metadata(&socket)
        .unwrap()
        .file_type()
        .is_socket());
}
