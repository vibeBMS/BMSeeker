//! Natively: a link resolved ([`super::Resolver`]) and its file streamed to
//! disk, with no limit on the whole (an archive from a slow host can take
//! an hour) but a limit on silence ([`STALL_MS`]) and the thread's
//! [`Progress`](crate::progress::Progress) able to cancel it.

use super::{sniff_bytes, Context, Fetch, MegaCipher, Resolver, Want, STALL_MS};
use crate::net::{percent_decode, run, Client};
use crate::text::safe_file_name;
use std::io::{Read, Write};
use std::path::Path;

/// Download the archive `link` leads to into `out`.
pub fn fetch(link: &str, want: Option<Want>, out: &Path, cx: &Context) -> Result<(), Fetch> {
    fetch_named(link, want, out, false, cx).map(|_| ())
}

/// [`fetch`], and the file's name where it came from (the folder's
/// listing, MEGA, the mirror, the server, the link). With `any`, a file
/// that is not an archive is kept too if it is not a page: a chart or a
/// movie posted on its own (an event's add-on).
pub fn fetch_named(
    link: &str,
    want: Option<Want>,
    out: &Path,
    any: bool,
    cx: &Context,
) -> Result<String, Fetch> {
    crate::progress::stage(crate::progress::Stage::Finding);
    let http = Client::new(&cx.config);
    let mut r = Resolver::new(link, want.clone(), cx);
    let mut moves = 0;
    loop {
        let mut moved = None;
        let got = run(&mut r, &http).and_then(|t| get_target(t, out, any, cx, &mut moved));
        let cancelled = crate::progress::current().is_some_and(|p| p.cancelled());
        // a page reached through redirects (a short link) on a host that
        // knows its way to the file: from there
        let rehost = moved.filter(|u| {
            let host = super::Link::parse(u, 0).and_then(|l| cx.hosts.find(&l).map(|h| h.name()));
            host.is_some_and(|h| h != "direct" && h != r.host())
        });
        match (got, rehost) {
            (Err(Fetch::Page), Some(u)) if !cancelled && moves < 2 => {
                moves += 1;
                r = Resolver::new(&u, want.clone(), cx);
            }
            // a mirror the page named, when this file did not come
            (Err(_), _) if !cancelled && r.try_next() => {
                crate::progress::stage(crate::progress::Stage::Finding);
            }
            (got, _) => return got,
        }
    }
}

/// `t`'s file into `out`: its name. A page that redirects led to elsewhere:
/// that place in `moved`.
fn get_target(
    t: super::Target,
    out: &Path,
    any: bool,
    cx: &Context,
    moved: &mut Option<String>,
) -> Result<String, Fetch> {
    let ua = &cx.config.user_agent;
    let served = match &t.mega_key {
        None => {
            let s = retrying(|| download(&t.url, out, ua))?;
            *moved = s.at.filter(|u| u != &t.url);
            s.name
        }
        Some(key) => {
            let enc = out.with_extension("mega");
            retrying(|| download(&t.url, &enc, ua))?;
            let d = mega_decrypt(&enc, out, key);
            let _ = std::fs::remove_file(&enc);
            d?;
            None
        }
    };
    let name = t
        .name
        .or(served)
        .or_else(|| url_file_name(&t.url))
        .unwrap_or_else(|| "download".into());
    let page = || {
        let mut head = [0u8; 512];
        let n = std::fs::File::open(out)
            .and_then(|mut f| f.read(&mut head))
            .unwrap_or(0);
        n == 0 || head[..n].trim_ascii_start().starts_with(b"<")
    };
    if sniff(out).is_none() && !(any && !page()) {
        let quota = drive_quota(out);
        let _ = std::fs::remove_file(out);
        return Err(match quota {
            true => Fetch::Failed(DRIVE_QUOTA.into()),
            false => Fetch::Page,
        });
    }
    Ok(name)
}

/// Google Drive's answer for a file downloaded too often lately.
pub const DRIVE_QUOTA: &str =
    "Google Drive: the file was downloaded too often lately (try again later, up to a day)";

/// Whether the page in `p` is Google Drive's "Quota exceeded".
fn drive_quota(p: &Path) -> bool {
    let mut head = vec![0u8; 4096];
    let n = std::fs::File::open(p)
        .and_then(|mut f| f.read(&mut head))
        .unwrap_or(0);
    let head = String::from_utf8_lossy(&head[..n]);
    head.contains("<title>Google Drive") && head.contains("Quota exceeded")
}

/// The file name in a `Content-Disposition` header (`filename*=UTF-8''…`
/// or `filename="…"`; UTF-8 sent as Latin-1 read back as UTF-8).
fn disposition_name(h: &str) -> Option<String> {
    let lower = h.to_ascii_lowercase();
    let value = |key: &str| {
        let at = lower.find(key)? + key.len();
        let v = h[at..].split(';').next()?.trim().trim_matches('"');
        (!v.is_empty()).then(|| v.to_string())
    };
    let name = if let Some(v) = value("filename*=") {
        let v = v
            .split_once("''")
            .map_or(v.as_str(), |(_, n)| n)
            .to_string();
        percent_decode(&v)
    } else {
        let v = value("filename=")?;
        let latin1: Option<Vec<u8>> = v.chars().map(|c| u8::try_from(c as u32).ok()).collect();
        latin1.and_then(|b| String::from_utf8(b).ok()).unwrap_or(v)
    };
    safe_file_name(&name)
}

/// The last part of a URL's path, decoded.
fn url_file_name(url: &str) -> Option<String> {
    let path = url.split(['?', '#']).next()?.trim_end_matches('/');
    let last = path.rsplit('/').next()?;
    if path.matches('/').count() < 3 {
        return None; // just the host
    }
    safe_file_name(&percent_decode(last))
}

/// The archive format of a file from its first bytes.
pub fn sniff(p: &Path) -> Option<&'static str> {
    let mut head = [0u8; 8];
    let n = std::fs::File::open(p).ok()?.read(&mut head).ok()?;
    sniff_bytes(&head[..n])
}

fn mega_decrypt(from: &Path, to: &Path, k: &[u8]) -> Result<(), String> {
    let mut c = MegaCipher::new(k);
    let mut src = std::io::BufReader::new(std::fs::File::open(from).map_err(|e| e.to_string())?);
    let mut dst = std::io::BufWriter::new(std::fs::File::create(to).map_err(|e| e.to_string())?);
    let mut buf = vec![0u8; 1 << 16];
    loop {
        let n = read_full(&mut src, &mut buf).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        c.apply(&mut buf[..n]);
        dst.write_all(&buf[..n]).map_err(|e| e.to_string())?;
    }
    dst.flush().map_err(|e| e.to_string())
}

/// Fill `buf` (short only at the end), so chunks stay on 16-byte blocks.
fn read_full(r: &mut impl Read, buf: &mut [u8]) -> std::io::Result<usize> {
    let mut n = 0;
    while n < buf.len() {
        match r.read(&mut buf[n..])? {
            0 => break,
            k => n += k,
        }
    }
    Ok(n)
}

const STALL: std::time::Duration = std::time::Duration::from_millis(STALL_MS);

/// Stream `url` into `out`. No limit on the whole, but it ends when no data
/// comes for [`STALL_MS`], or when the thread's progress is cancelled.
pub fn download_to(url: &str, out: &Path) -> Result<(), String> {
    let ua = crate::config::Config::default().user_agent;
    retrying(|| download(url, out, &ua)).map(|_| ())
}

/// `f` again after a server's passing failure (busy, or a gateway's error:
/// manbow answers 502 and 503 at times), twice at most, waiting longer each
/// time.
pub(crate) fn retrying<T>(mut f: impl FnMut() -> Result<T, String>) -> Result<T, String> {
    let mut waits = [3u64, 10].into_iter();
    loop {
        match f() {
            Err(e) if crate::net::passing(&e) => match waits.next() {
                Some(s) if !crate::progress::current().is_some_and(|p| p.cancelled()) => {
                    std::thread::sleep(std::time::Duration::from_secs(s));
                }
                _ => return Err(e),
            },
            r => return r,
        }
    }
}

/// What a download got from the server.
struct Served {
    /// The file name it gave (`Content-Disposition`).
    name: Option<String>,
    /// Where its redirects ended.
    at: Option<String>,
}

/// [`download_to`], and what the server said of it.
fn download(url: &str, out: &Path, user_agent: &str) -> Result<Served, String> {
    use std::sync::mpsc::{channel, RecvTimeoutError};
    use std::time::{Duration, Instant};
    /// What the request's thread sends: the answer's head, its data, its
    /// end (an empty chunk), or what went wrong.
    enum Part {
        Head(Option<String>, Option<u64>, Option<String>),
        Chunk(Vec<u8>),
    }
    let file = std::fs::File::create(out).map_err(|e| format!("{}: {e}", out.display()))?;
    let mut w = std::io::BufWriter::new(file);
    let progress = crate::progress::current();
    let mut req = ehttp::Request::get(url);
    req.timeout = None;
    // Dropbox answers a browser with its page instead of the file
    if !url.contains("dropbox") && !user_agent.is_empty() {
        req.headers.insert("User-Agent", user_agent);
    }
    // the request on its own thread; its parts come here. Once this returns
    // the channel is closed and it stops at its next part.
    let (tx, rx) = channel::<Result<Part, String>>();
    std::thread::spawn(move || {
        let mut r = match req.fetch_raw_native(false) {
            Ok(r) => r,
            Err(e) => return drop(tx.send(Err(e.to_string()))),
        };
        if !r.status().is_success() {
            return drop(tx.send(Err(format!("HTTP {}", r.status().as_u16()))));
        }
        let at = ureq::ResponseExt::get_uri(&r).to_string();
        let header = |k: &str| {
            r.headers()
                .get(k)
                .map(|v| crate::net::header_text(v.as_bytes()))
        };
        let name = header("content-disposition").and_then(|d| disposition_name(&d));
        let length = header("content-length").and_then(|l| l.trim().parse().ok());
        if tx.send(Ok(Part::Head(name, length, Some(at)))).is_err() {
            return;
        }
        let mut body = r.body_mut().as_reader();
        let mut buf = vec![0u8; 1 << 16];
        loop {
            match std::io::Read::read(&mut body, &mut buf) {
                Ok(n) => {
                    if tx.send(Ok(Part::Chunk(buf[..n].to_vec()))).is_err() || n == 0 {
                        return;
                    }
                }
                Err(e) => return drop(tx.send(Err(e.to_string()))),
            }
        }
    });
    let mut last = Instant::now();
    let mut served = Served {
        name: None,
        at: None,
    };
    loop {
        if progress.as_ref().is_some_and(|p| p.cancelled()) {
            return Err(crate::progress::CANCELLED.into());
        }
        match rx.recv_timeout(Duration::from_millis(200)) {
            Ok(Err(e)) => return Err(e),
            Ok(Ok(Part::Head(n, length, at))) => {
                served = Served { name: n, at };
                if let Some(p) = &progress {
                    p.start_file(length);
                }
                last = Instant::now();
            }
            Ok(Ok(Part::Chunk(c))) if c.is_empty() => {
                return w.flush().map(|_| served).map_err(|e| e.to_string());
            }
            Ok(Ok(Part::Chunk(c))) => {
                if let Some(p) = &progress {
                    p.add(c.len() as u64);
                }
                w.write_all(&c).map_err(|e| e.to_string())?;
                last = Instant::now();
            }
            Err(RecvTimeoutError::Timeout) if last.elapsed() > STALL => {
                return Err(format!("no data for {} s", STALL.as_secs()));
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => return Err("the download ended early".into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_names() {
        assert_eq!(
            disposition_name("attachment; filename=\"a b.zip\"").as_deref(),
            Some("a b.zip")
        );
        assert_eq!(
            disposition_name("attachment; filename=\"x.zip\"; filename*=UTF-8''%E6%9B%B2.zip")
                .as_deref(),
            Some("曲.zip")
        );
        // UTF-8 bytes sent as Latin-1
        let latin1: String = "曲.rar".bytes().map(|b| b as char).collect();
        assert_eq!(
            disposition_name(&format!("attachment; filename=\"{latin1}\"")).as_deref(),
            Some("曲.rar")
        );
        assert_eq!(
            url_file_name("https://h.example/d/%E6%9B%B2%20x.7z?dl=1").as_deref(),
            Some("曲 x.7z")
        );
        assert_eq!(url_file_name("https://h.example/"), None);
        assert_eq!(safe_file_name("../a/b.zip").as_deref(), Some("_a_b.zip"));
    }

    /// `VIBEBMS_FETCH=URL cargo test -p vibebms fetch_url -- --ignored`
    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    #[ignore = "network"]
    fn fetch_url() {
        let url = std::env::var("VIBEBMS_FETCH").expect("VIBEBMS_FETCH");
        let out = std::env::temp_dir().join("vibebms-fetch");
        match fetch(&url, None, &out, &Context::default()) {
            Ok(()) => println!(
                "OK {:?} {} bytes",
                sniff(&out),
                std::fs::metadata(&out).map(|m| m.len()).unwrap_or(0)
            ),
            Err(e) => println!("ERR {e:?}"),
        }
    }

    #[test]
    fn mega_ctr() {
        // AES-128 of a zero key and zero counter block, xored with zeros
        let dir = std::env::temp_dir().join(format!("vibebms-mega-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let (a, b) = (dir.join("a"), dir.join("b"));
        std::fs::write(&a, [0u8; 20]).unwrap();
        mega_decrypt(&a, &b, &[0u8; 32]).unwrap();
        let out = std::fs::read(&b).unwrap();
        assert_eq!(out.len(), 20);
        assert_eq!(&out[..4], &[0x66, 0xe9, 0x4b, 0xd4]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A server that sends a little and then nothing: cancelling ends it.
    #[test]
    fn a_stalled_download_cancels() {
        use std::io::{Read, Write};
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = l.local_addr().unwrap();
        std::thread::spawn(move || {
            let (mut c, _) = l.accept().unwrap();
            let mut buf = [0u8; 1024];
            let _ = c.read(&mut buf);
            let _ = c.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 1000000\r\n\r\nPK");
            std::thread::sleep(std::time::Duration::from_secs(30));
        });
        let p = crate::progress::Progress::new();
        let p2 = p.clone();
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(800));
            p2.cancel();
        });
        let out = std::env::temp_dir().join(format!("bmseeker-stall-{}", std::process::id()));
        let t = std::time::Instant::now();
        let r = crate::progress::scope(Some(p.clone()), || {
            download_to(&format!("http://{addr}/a.zip"), &out)
        });
        assert_eq!(r, Err(crate::progress::CANCELLED.to_string()));
        assert!(t.elapsed() < std::time::Duration::from_secs(5));
        assert_eq!(p.bytes(), (2, Some(1_000_000)));
        let _ = std::fs::remove_file(out);
    }
}
