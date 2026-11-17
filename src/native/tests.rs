use super::*;
use crate::hosts::Target;
use crate::net::Request;
use crate::testkit::{serve, tmp, Reply};

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
    let n = Native::new(&Context::default());
    for p in ["/a", "/b", "/a"] {
        let got = n.perform(Effect::Http(Request::Get(format!("{}{p}", s.url))));
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
    let n = Native::new(&Context::default());
    let get = |p: &str| n.get(&format!("{}{p}", s.url));
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
    let s = serve(vec![
        ("/r", Reply::status(302, "").header("Location", "/f.zip")),
        (
            "/f.zip",
            Reply::ok("PK file").header("Content-Disposition", "attachment; filename=\"曲.zip\""),
        ),
        ("/mega", Reply::ok([0u8; 20])),
    ]);
    let dir = tmp("native-dl");
    let n = Native::new(&Context::default());
    let download = |url: String, mega_key| {
        let target = Target {
            mega_key,
            ..Target::url(url)
        };
        let slot = dir.join("slot");
        match n.perform(Effect::Download {
            target,
            slot,
            any: false,
        }) {
            Fed::Downloaded(Ok(s)) => s,
            _ => panic!("no file"),
        }
    };
    let got = download(format!("{}/r", s.url), None);
    assert_eq!(got.at, Some(format!("{}/f.zip", s.url)));
    assert_eq!(got.name.as_deref(), Some("曲.zip"));
    assert_eq!(got.head, b"PK file");
    // AES-128 of a zero key and zero counter block, xored with zeros
    let got = download(format!("{}/mega", s.url), Some(vec![0u8; 32]));
    assert_eq!((got.name, got.at), (None, None));
    assert_eq!(got.head.len(), 20);
    assert_eq!(&got.head[..4], &[0x66, 0xe9, 0x4b, 0xd4]);
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
    let r = http::download(
        &Target::url(format!("http://{addr}/a.zip")),
        &dir.join("a"),
        "",
        Some(&p),
    )
    .map(|_| ());
    assert_eq!(r, Err(NetError::Cancelled));
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
    let got = n.perform(Effect::Http(Request::Get("http://127.0.0.1:9/".into())));
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
    match fetch(&url, None, &out, false, &Context::default()) {
        Ok(name) => println!(
            "OK {name} {:?} {} bytes",
            crate::unpack::sniff_file(&out),
            std::fs::metadata(&out).map(|m| m.len()).unwrap_or(0)
        ),
        Err(e) => println!("ERR {e:?}"),
    }
}
