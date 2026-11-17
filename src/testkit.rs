//! What tests build their cases from: archives, and folders of their own
//! that go when done. A player's tests have it with the `testkit` feature.

use std::io::Write;
use std::path::{Path, PathBuf};

/// A zip of `files` (name, contents).
pub fn zip<B: AsRef<[u8]>>(files: &[(&str, B)]) -> Vec<u8> {
    zipped(files, zip::write::SimpleFileOptions::default())
}

/// A zip of `files` (name, contents), each of the time `at`.
pub fn zip_at<B: AsRef<[u8]>>(files: &[(&str, B)], at: zip::DateTime) -> Vec<u8> {
    zipped(
        files,
        zip::write::SimpleFileOptions::default().last_modified_time(at),
    )
}

fn zipped<B: AsRef<[u8]>>(files: &[(&str, B)], o: zip::write::SimpleFileOptions) -> Vec<u8> {
    let mut out = std::io::Cursor::new(Vec::new());
    let mut z = zip::ZipWriter::new(&mut out);
    for (name, body) in files {
        z.start_file(*name, o).unwrap();
        z.write_all(body.as_ref()).unwrap();
    }
    z.finish().unwrap();
    out.into_inner()
}

/// A RAR5 (stored) of `song/a.bms` ("#TITLE A") and `song/01.ogg`
/// ("ogg"), both of 2020-01-02 03:04:05 UTC.
pub const SONG_RAR: [u8; 117] = [
    82, 97, 114, 33, 26, 7, 1, 0, 51, 146, 181, 229, 10, 1, 5, 6, 0, 5, 1, 1, 128, 128, 0, 66, 98,
    211, 59, 32, 2, 2, 136, 0, 6, 136, 0, 164, 131, 2, 165, 93, 13, 94, 221, 184, 73, 19, 128, 0,
    1, 10, 115, 111, 110, 103, 47, 97, 46, 98, 109, 115, 35, 84, 73, 84, 76, 69, 32, 65, 227, 107,
    162, 118, 33, 2, 2, 131, 0, 6, 131, 0, 164, 131, 2, 165, 93, 13, 94, 148, 92, 160, 69, 128, 0,
    1, 11, 115, 111, 110, 103, 47, 48, 49, 46, 111, 103, 103, 111, 103, 103, 29, 119, 86, 81, 3, 5,
    4, 0,
];

/// A folder of a test's own on the disk, empty to begin with, removed
/// (with what is in it) when dropped.
pub struct Tmp(PathBuf);

/// A new [`Tmp`] named after `name` (and this process).
pub fn tmp(name: &str) -> Tmp {
    let d = std::env::temp_dir().join(format!("bmseeker-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    Tmp(d)
}

impl std::ops::Deref for Tmp {
    type Target = Path;
    fn deref(&self) -> &Path {
        &self.0
    }
}

impl AsRef<Path> for Tmp {
    fn as_ref(&self) -> &Path {
        &self.0
    }
}

impl Drop for Tmp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// What [`serve`] answers a request with.
#[derive(Clone, Debug)]
pub struct Reply {
    pub status: u16,
    pub headers: Vec<(&'static str, String)>,
    pub body: Vec<u8>,
}

impl Reply {
    /// `200 OK` with `body`.
    pub fn ok(body: impl Into<Vec<u8>>) -> Reply {
        Reply::status(200, body)
    }

    pub fn status(status: u16, body: impl Into<Vec<u8>>) -> Reply {
        Reply {
            status,
            headers: Vec::new(),
            body: body.into(),
        }
    }

    pub fn header(mut self, name: &'static str, value: impl Into<String>) -> Reply {
        self.headers.push((name, value.into()));
        self
    }
}

/// A server on this machine (its connections kept alive) answering each
/// path from `routes` (several for one path: one after another, the last
/// again and again; none: 404), keeping the paths asked and the number of
/// connections.
#[cfg(not(target_arch = "wasm32"))]
pub fn serve(routes: Vec<(&str, Reply)>) -> Server {
    use std::io::{BufRead, Read};
    use std::sync::atomic::Ordering;
    let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let server = Server {
        url: format!("http://{}", l.local_addr().unwrap()),
        asked: Default::default(),
        connections: Default::default(),
    };
    let routes: Vec<(String, Reply)> = routes.into_iter().map(|(p, r)| (p.into(), r)).collect();
    let routes = std::sync::Arc::new(std::sync::Mutex::new(routes));
    let (asked, connections) = (server.asked.clone(), server.connections.clone());
    std::thread::spawn(move || {
        for c in l.incoming() {
            let Ok(c) = c else { return };
            connections.fetch_add(1, Ordering::SeqCst);
            let (routes, asked) = (routes.clone(), asked.clone());
            std::thread::spawn(move || {
                let mut r = std::io::BufReader::new(c.try_clone().unwrap());
                let mut w = c;
                loop {
                    let (mut line, mut path, mut length) = (String::new(), None, 0);
                    loop {
                        line.clear();
                        if r.read_line(&mut line).unwrap_or(0) == 0 {
                            return;
                        }
                        if line == "\r\n" {
                            break;
                        }
                        let lower = line.to_ascii_lowercase();
                        if path.is_none() {
                            path = line.split(' ').nth(1).map(String::from);
                        } else if let Some(n) = lower.strip_prefix("content-length:") {
                            length = n.trim().parse().unwrap_or(0);
                        }
                    }
                    let _ = r.by_ref().take(length).read_to_end(&mut Vec::new());
                    let path = path.unwrap_or_default();
                    asked.lock().unwrap().push(path.clone());
                    let reply = {
                        let mut routes = routes.lock().unwrap();
                        let mut of = routes.iter().enumerate().filter(|(_, (p, _))| *p == path);
                        match (of.next(), of.next()) {
                            (Some((i, _)), Some(_)) => routes.remove(i).1,
                            (Some((_, (_, r))), None) => r.clone(),
                            (None, _) => Reply::status(404, ""),
                        }
                    };
                    let mut out = format!(
                        "HTTP/1.1 {} X\r\nContent-Length: {}\r\n",
                        reply.status,
                        reply.body.len()
                    );
                    for (k, v) in &reply.headers {
                        out.push_str(&format!("{k}: {v}\r\n"));
                    }
                    out.push_str("\r\n");
                    let mut out = out.into_bytes();
                    out.extend(&reply.body);
                    if std::io::Write::write_all(&mut w, &out).is_err() {
                        return;
                    }
                }
            });
        }
    });
    server
}

/// A server of [`serve`].
#[cfg(not(target_arch = "wasm32"))]
pub struct Server {
    /// `http://127.0.0.1:<port>`
    pub url: String,
    pub asked: std::sync::Arc<std::sync::Mutex<Vec<String>>>,
    pub connections: std::sync::Arc<std::sync::atomic::AtomicUsize>,
}

#[cfg(not(target_arch = "wasm32"))]
impl Server {
    /// The paths asked so far.
    pub fn asked(&self) -> Vec<String> {
        self.asked.lock().unwrap().clone()
    }

    pub fn connections(&self) -> usize {
        self.connections.load(std::sync::atomic::Ordering::SeqCst)
    }
}
