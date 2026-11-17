//! Natively: a link resolved ([`super::Resolver`]) and its file streamed to
//! disk, with no limit on the whole (an archive from a slow host can take
//! an hour) but a limit on silence ([`STALL_MS`]) and the context's
//! [`Progress`](crate::progress::Progress) able to cancel it.

use super::{Context, Fetch, MegaCipher, Want, STALL_MS};
use crate::install::job::{self, run_job, Acquire, Driver, Effect, Fed};
use crate::net::{percent_decode, Client, NetError};
use crate::progress::Progress;
use crate::text::safe_file_name;
use crate::unpack::Blob;
use std::io::{Read, Write};
use std::path::Path;

/// Download the archive `link` leads to into `out`: its file name where it
/// came from (the folder's listing, MEGA, the mirror, the server, the
/// link). With `any`, a file that is not an archive is kept too if it is
/// not a page: a chart or a movie posted on its own (an event's add-on).
/// [`Acquire`] run with the native transport ([`Native`]).
pub fn fetch(
    link: &str,
    want: Option<Want>,
    out: &Path,
    any: bool,
    cx: &Context,
) -> Result<String, Fetch> {
    let mut job = Acquire::new(link, want, any, out.to_path_buf(), cx);
    match run_job(&mut job, &Native::new(cx)) {
        Ok(got) => Ok(got.name),
        Err(e) => {
            // (a page, or what did not come)
            let _ = std::fs::remove_file(out);
            Err(e)
        }
    }
}

/// The files of `links`, one after another, into files named after
/// `slots` ([`Files::slot`]): all of them, or which did not come and why
/// (the files that came are left for the caller to remove).
pub fn fetch_files(
    links: Vec<job::FileLink>,
    slots: &Path,
    cx: &Context,
) -> Result<Vec<job::Got>, job::Missed> {
    run_job(
        &mut job::Files::new(links, slots.to_path_buf(), cx),
        &Native::new(cx),
    )
}

/// The native transport, as a job's [`Driver`]: requests through the
/// [`Client`] (tried again after a passing failure), a file streamed to
/// its slot on a thread of its own (ended by the progress' cancel or
/// [`STALL_MS`] of silence; a MEGA file decrypted after), waits slept.
pub struct Native {
    http: Client,
    user_agent: String,
    progress: Option<std::sync::Arc<Progress>>,
}

impl Native {
    /// With `cx`'s settings and progress.
    pub fn new(cx: &Context) -> Native {
        Native {
            http: Client::in_context(cx),
            user_agent: cx.config.user_agent.clone(),
            progress: cx.progress(),
        }
    }
}

impl Driver for Native {
    fn perform(&self, effect: Effect) -> Fed {
        let p = self.progress.as_deref();
        match effect {
            Effect::Http(r) => Fed::Http(crate::net::Http::send(&self.http, &r)),
            Effect::Sleep(d) => {
                std::thread::sleep(d);
                Fed::Woke
            }
            Effect::Download { target, slot, .. } => {
                let got = match &target.mega_key {
                    None => stream(&target.url, &slot, &self.user_agent, p),
                    Some(key) => {
                        let enc = slot.with_extension("mega");
                        let s = stream(&target.url, &enc, &self.user_agent, p);
                        let d =
                            s.and_then(|_| mega_decrypt(&enc, &slot, key).map_err(NetError::Other));
                        let _ = std::fs::remove_file(&enc);
                        // (MEGA names its files itself; no redirects)
                        d.map(|_| Served {
                            name: None,
                            at: None,
                        })
                    }
                };
                Fed::Downloaded(match got {
                    Ok(s) => {
                        let mut head = vec![0u8; 4096];
                        let n = std::fs::File::open(&slot)
                            .and_then(|mut f| read_full(&mut f, &mut head))
                            .unwrap_or(0);
                        head.truncate(n);
                        Ok(job::Served {
                            blob: Blob::File(slot),
                            name: s.name,
                            at: s.at,
                            head,
                        })
                    }
                    Err(e) => Err(e.into()),
                })
            }
        }
    }
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

/// Stream `url` into `out` with `cx`'s user agent, reporting to its
/// progress. No limit on the whole, but it ends when no data comes for
/// [`STALL_MS`], or when the progress is cancelled.
pub fn download(url: &str, out: &Path, cx: &Context) -> Result<(), NetError> {
    let progress = cx.progress();
    let p = progress.as_deref();
    retrying(p, || stream(url, out, &cx.config.user_agent, p)).map(|_| ())
}

/// `f` again after a server's passing failure (busy, or a gateway's error:
/// manbow answers 502 and 503 at times), twice at most, waiting longer each
/// time (not once `progress` is cancelled).
pub(crate) fn retrying<T>(
    progress: Option<&Progress>,
    mut f: impl FnMut() -> Result<T, NetError>,
) -> Result<T, NetError> {
    let mut waits = [3u64, 10].into_iter();
    loop {
        match f() {
            Err(e) if e.passing() => match waits.next() {
                Some(s) if !progress.is_some_and(|p| p.cancelled()) => {
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

/// `url` into `out` once, reporting to `progress`: what the server said of
/// it.
fn stream(
    url: &str,
    out: &Path,
    user_agent: &str,
    progress: Option<&Progress>,
) -> Result<Served, NetError> {
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
    let mut req = ehttp::Request::get(url);
    req.timeout = None;
    // Dropbox answers a browser with its page instead of the file
    if !url.contains("dropbox") && !user_agent.is_empty() {
        req.headers.insert("User-Agent", user_agent);
    }
    // the request on its own thread; its parts come here. Once this returns
    // the channel is closed and it stops at its next part.
    let (tx, rx) = channel::<Result<Part, NetError>>();
    std::thread::spawn(move || {
        let mut r = match req.fetch_raw_native(false) {
            Ok(r) => r,
            Err(e) => return drop(tx.send(Err(e.to_string().into()))),
        };
        if !r.status().is_success() {
            return drop(tx.send(Err(NetError::Status(r.status().as_u16()))));
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
                Err(e) => return drop(tx.send(Err(e.to_string().into()))),
            }
        }
    });
    let mut last = Instant::now();
    let mut served = Served {
        name: None,
        at: None,
    };
    loop {
        if progress.is_some_and(|p| p.cancelled()) {
            return Err(NetError::Cancelled);
        }
        match rx.recv_timeout(Duration::from_millis(200)) {
            Ok(Err(e)) => return Err(e),
            Ok(Ok(Part::Head(n, length, at))) => {
                served = Served { name: n, at };
                if let Some(p) = progress {
                    p.start_file(length);
                }
                last = Instant::now();
            }
            Ok(Ok(Part::Chunk(c))) if c.is_empty() => {
                return w.flush().map(|_| served).map_err(|e| e.to_string().into());
            }
            Ok(Ok(Part::Chunk(c))) => {
                if let Some(p) = progress {
                    p.add(c.len() as u64);
                }
                w.write_all(&c).map_err(|e| e.to_string())?;
                last = Instant::now();
            }
            Err(RecvTimeoutError::Timeout) if last.elapsed() > STALL => {
                return Err(NetError::Stalled(STALL.as_secs()));
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
            super::super::url_file_name("https://h.example/d/%E6%9B%B2%20x.7z?dl=1").as_deref(),
            Some("曲 x.7z")
        );
        assert_eq!(super::super::url_file_name("https://h.example/"), None);
        assert_eq!(safe_file_name("../a/b.zip").as_deref(), Some("_a_b.zip"));
    }

    /// `BMSEEKER_FETCH=URL cargo test -p bmseeker fetch_url -- --ignored`
    #[cfg(not(target_arch = "wasm32"))]
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

    #[test]
    fn mega_ctr() {
        // AES-128 of a zero key and zero counter block, xored with zeros
        let dir = std::env::temp_dir().join(format!("bmseeker-mega-{}", std::process::id()));
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
        let p = Progress::new();
        let p2 = p.clone();
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(800));
            p2.cancel();
        });
        let out = std::env::temp_dir().join(format!("bmseeker-stall-{}", std::process::id()));
        let t = std::time::Instant::now();
        let cx = Context::default().with_progress(p.clone());
        let r = download(&format!("http://{addr}/a.zip"), &out, &cx);
        assert_eq!(r, Err(NetError::Cancelled));
        assert!(t.elapsed() < std::time::Duration::from_secs(5));
        assert_eq!(p.bytes(), (2, Some(1_000_000)));
        let _ = std::fs::remove_file(out);
    }
}
