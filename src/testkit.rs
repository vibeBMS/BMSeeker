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

/// A tar (ustar) of `files` (name, contents), each of 2020-01-02 03:04:05
/// UTC; a name as it is (`..` or `/` too) if it fits in 100 bytes.
pub fn tar<B: AsRef<[u8]>>(files: &[(&str, B)]) -> Vec<u8> {
    let mut t = tar::Builder::new(Vec::new());
    for (name, body) in files {
        let body = body.as_ref();
        let mut h = tar::Header::new_ustar();
        h.set_size(body.len() as u64);
        h.set_mode(0o644);
        h.set_mtime(1_577_934_245);
        h.set_entry_type(tar::EntryType::Regular);
        if name.len() <= 100 {
            h.as_old_mut().name[..name.len()].copy_from_slice(name.as_bytes());
            h.set_cksum();
            t.append(&h, body).unwrap();
        } else {
            t.append_data(&mut h, name, body).unwrap();
        }
    }
    t.into_inner().unwrap()
}

/// [`tar`] compressed with gzip (a `.tar.gz`).
pub fn tar_gz<B: AsRef<[u8]>>(files: &[(&str, B)]) -> Vec<u8> {
    gzip(&tar(files))
}

/// `bytes` compressed with gzip.
pub fn gzip(bytes: &[u8]) -> Vec<u8> {
    let mut z = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    z.write_all(bytes).unwrap();
    z.finish().unwrap()
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

/// A RAR5, not solid, of `b.zip` (8,281 bytes: a zip of `big/zeros.bin`,
/// 8 MiB of zeros) and `a.zip` (4,330 bytes: a zip of `small/readme.txt`,
/// 4,200 bytes), both compressed, `a.zip`'s header saying its size is not
/// known.
pub const SIZELESS_RAR: [u8; 394] = [
    82, 97, 114, 33, 26, 7, 1, 0, 243, 225, 130, 235, 11, 1, 5, 7, 0, 6, 1, 1, 128, 128, 128, 0,
    64, 128, 12, 206, 37, 2, 3, 11, 160, 129, 0, 4, 217, 192, 0, 164, 131, 2, 77, 167, 190, 109,
    128, 3, 1, 5, 98, 46, 122, 105, 112, 10, 3, 19, 58, 28, 9, 107, 88, 72, 86, 57, 192, 7, 157,
    38, 69, 85, 35, 246, 96, 52, 231, 95, 70, 9, 2, 86, 213, 41, 146, 4, 186, 54, 74, 37, 210, 166,
    151, 64, 194, 4, 8, 66, 16, 36, 5, 173, 105, 240, 58, 5, 208, 14, 135, 116, 11, 75, 161, 91,
    81, 208, 104, 232, 69, 161, 252, 123, 212, 160, 131, 163, 150, 140, 232, 116, 231, 78, 156,
    230, 130, 230, 154, 202, 240, 204, 179, 234, 51, 74, 229, 45, 50, 109, 75, 128, 22, 31, 88,
    123, 191, 104, 229, 221, 62, 244, 116, 14, 54, 163, 126, 126, 123, 97, 223, 77, 255, 115, 157,
    233, 253, 68, 132, 73, 243, 5, 81, 82, 182, 193, 158, 217, 160, 84, 192, 60, 207, 153, 68, 163,
    141, 126, 58, 114, 22, 127, 185, 223, 144, 170, 211, 36, 162, 230, 202, 160, 21, 145, 200, 98,
    160, 45, 239, 87, 93, 194, 70, 117, 76, 28, 45, 0, 110, 131, 58, 202, 35, 2, 3, 11, 248, 0, 12,
    234, 33, 164, 131, 2, 168, 205, 46, 165, 128, 3, 1, 5, 97, 46, 122, 105, 112, 10, 3, 19, 58,
    28, 9, 107, 47, 61, 144, 57, 192, 239, 117, 37, 85, 83, 47, 147, 57, 24, 173, 205, 19, 0, 92,
    5, 131, 128, 184, 12, 166, 96, 45, 16, 24, 8, 50, 82, 178, 132, 26, 92, 1, 185, 203, 220, 157,
    36, 138, 52, 32, 225, 83, 246, 123, 255, 249, 223, 31, 15, 96, 62, 199, 186, 29, 59, 87, 67,
    135, 49, 228, 28, 146, 148, 80, 88, 155, 91, 185, 171, 123, 48, 161, 207, 135, 63, 69, 155, 18,
    181, 155, 37, 38, 246, 42, 182, 232, 223, 27, 72, 2, 0, 57, 0, 114, 35, 140, 32, 227, 144, 164,
    255, 162, 111, 86, 195, 235, 246, 138, 93, 57, 167, 135, 241, 191, 45, 121, 227, 184, 0, 29,
    119, 86, 81, 3, 5, 4, 0,
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
