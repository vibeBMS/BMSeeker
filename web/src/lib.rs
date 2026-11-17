//! BMSeeker in a web page: the command-line tool's commands (`bmseeker::cli`)
//! run in a worker, on one of three targets, and its lines handed to the
//! page as the tool writes them (the GUI reads them as it reads the tool's).
//!
//! - `vibe`: the vibeBMS web player's library, in the page's storage (OPFS:
//!   songs in `bms/`, BMSeeker's downloads and scan in `bmseeker/`; the
//!   player's own files, `store/` and the rest, are never touched);
//! - `folder`: a folder the user picked (Chromium's `showDirectoryPicker`);
//! - `zip`: nothing at first; what comes in is downloaded as a zip.
//!
//! `www/worker.js` lists the target into a [`Session`] before a command,
//! reads its files when the command does (`bmseekerRead`, synchronously, as
//! a worker may), and writes what the command changed after it.

mod bridge;
mod mirror;
mod net;
pub mod zip;

pub use bridge::run;
pub use mirror::{Change, Mirror};
pub use net::{proxied, sends_cors, PROXY};

use bmseeker::config::Config;
use std::path::PathBuf;

/// What a target is: the folders commands may change, whether files keep
/// their times, where downloads wait, the library, and what a zip of it
/// leaves out.
pub struct Target {
    pub writable: Vec<PathBuf>,
    pub keeps_times: bool,
    pub downloads: &'static str,
    pub library: &'static str,
}

impl Target {
    pub fn of(kind: &str) -> Option<Target> {
        Some(match kind {
            "vibe" => Target {
                writable: vec!["/bms".into(), "/bmseeker".into()],
                keeps_times: false,
                downloads: "/bmseeker/downloads",
                library: "/bms",
            },
            "folder" => Target {
                writable: vec!["/".into()],
                keeps_times: false,
                downloads: ".bmseeker",
                library: "/",
            },
            "zip" => Target {
                writable: vec!["/".into()],
                keeps_times: true,
                downloads: ".bmseeker",
                library: "/",
            },
            _ => return None,
        })
    }

    pub fn config(&self) -> Config {
        Config {
            downloads: self.downloads.into(),
            ..Default::default()
        }
    }
}

#[cfg(target_arch = "wasm32")]
mod page {
    use super::*;
    use bmseeker::cli::Line;
    use bmseeker::fs::Fs;
    use std::path::Path;
    use std::sync::Arc;
    use wasm_bindgen::prelude::*;

    #[wasm_bindgen]
    extern "C" {
        /// The target's file at `path`, read synchronously (worker.js).
        #[wasm_bindgen(catch, js_name = bmseekerRead)]
        fn read_target(path: &str) -> Result<js_sys::Uint8Array, JsValue>;
    }

    fn now() -> i64 {
        (js_sys::Date::now() / 1000.0) as i64
    }

    /// A target's mirror and the commands run on it.
    #[wasm_bindgen]
    pub struct Session {
        mirror: Arc<Mirror>,
        config: Config,
        library: &'static str,
        net: net::Xhr,
    }

    #[wasm_bindgen]
    impl Session {
        /// A session on the target `kind` (`vibe`, `folder`, `zip`), its
        /// requests through `proxy`.
        #[wasm_bindgen(constructor)]
        pub fn new(kind: &str, proxy: &str) -> Result<Session, JsValue> {
            let t = Target::of(kind).ok_or_else(|| JsValue::from_str("no such target"))?;
            let source: mirror::Source = Box::new(|p: &Path| {
                read_target(&p.to_string_lossy())
                    .map(|a| a.to_vec())
                    .map_err(|e| {
                        let msg = js_sys::Reflect::get(&e, &"message".into())
                            .ok()
                            .and_then(|m| m.as_string())
                            .unwrap_or_else(|| format!("{e:?}"));
                        std::io::Error::other(format!("{}: {msg}", p.display()))
                    })
            });
            Ok(Session {
                mirror: Arc::new(Mirror::new(source, t.writable.clone(), t.keeps_times, now)),
                config: t.config(),
                library: t.library,
                net: net::Xhr {
                    proxy: proxy.to_string(),
                },
            })
        }

        /// The library's folder, for `-L`.
        #[wasm_bindgen(getter)]
        pub fn library(&self) -> String {
            self.library.to_string()
        }

        /// The target's files as listed (paths, sizes, times in seconds).
        pub fn mount(&self, paths: Vec<String>, sizes: Vec<f64>, mtimes: Vec<f64>) {
            for ((p, s), t) in paths.into_iter().zip(sizes).zip(mtimes) {
                self.mirror.mount(p.into(), s as u64, t as i64);
            }
        }

        /// How many files there are.
        pub fn count(&self) -> usize {
            self.mirror.files().len()
        }

        /// Run a command line: each line to `emit(kind, line)` (kind
        /// "record" or "note") as it comes; its exit code.
        pub fn run(&self, args: Vec<String>, emit: js_sys::Function) -> i32 {
            bridge::run(
                &args,
                self.config.clone(),
                self.mirror.clone(),
                &self.net,
                move |l| {
                    let (kind, text) = match l {
                        Line::Record(s) => ("record", s),
                        Line::Note(s) => ("note", s),
                    };
                    let _ = emit.call2(&JsValue::NULL, &kind.into(), &text.into());
                },
            )
        }

        /// What to do to the target, in order: `["remove", path, dir]`,
        /// `["move", from, to]`, `["write", path, bytes, mtime]`.
        pub fn changes(&self) -> js_sys::Array {
            let s = |p: &Path| JsValue::from_str(&p.to_string_lossy());
            self.mirror
                .changes()
                .into_iter()
                .map(|c| -> JsValue {
                    match c {
                        Change::Remove { path, dir } => {
                            js_sys::Array::of3(&"remove".into(), &s(&path), &dir.into()).into()
                        }
                        Change::Move { from, to } => {
                            js_sys::Array::of3(&"move".into(), &s(&from), &s(&to)).into()
                        }
                        Change::Write { path, bytes, mtime } => js_sys::Array::of4(
                            &"write".into(),
                            &s(&path),
                            &js_sys::Uint8Array::from(&bytes[..]),
                            &(mtime as f64).into(),
                        )
                        .into(),
                    }
                })
                .collect()
        }

        /// The changes were made: `written` (with their sizes and times
        /// there) are the target's now, `failed` are not.
        pub fn settle(
            &self,
            written: Vec<String>,
            sizes: Vec<f64>,
            mtimes: Vec<f64>,
            failed: Vec<String>,
        ) {
            let written: Vec<(PathBuf, u64, i64)> = written
                .into_iter()
                .zip(sizes)
                .zip(mtimes)
                .map(|((p, s), t)| (p.into(), s as u64, t as i64))
                .collect();
            let failed: Vec<PathBuf> = failed.into_iter().map(Into::into).collect();
            self.mirror.settle(&written, &failed);
        }

        /// The files as a zip (BMSeeker's own folder left out); a file that
        /// could not be read is an error.
        pub fn zip(&self) -> Result<js_sys::Uint8Array, JsValue> {
            let skip = Path::new("/").join(self.config.downloads.as_str());
            let (bytes, unread) =
                zip::zip(&self.mirror, &skip).map_err(|e| JsValue::from_str(&e.to_string()))?;
            if !unread.is_empty() {
                return Err(JsValue::from_str(&format!(
                    "could not read {}",
                    unread.join(", ")
                )));
            }
            Ok(js_sys::Uint8Array::from(&bytes[..]))
        }

        /// A file the user downloaded (`name`, `bytes`) put in the
        /// downloads folder: its path, for `library import`.
        pub fn put(&self, name: &str, bytes: Vec<u8>) -> Result<String, JsValue> {
            use bmseeker::fs::Store;
            use std::io::Write;
            let name = Path::new(name)
                .file_name()
                .ok_or_else(|| JsValue::from_str("no file name"))?;
            let p = Path::new(self.library)
                .join(self.config.downloads.as_str())
                .join(name);
            self.mirror
                .create(&p, None)
                .and_then(|mut w| w.write_all(&bytes))
                .map_err(|e| JsValue::from_str(&e.to_string()))?;
            Ok(p.to_string_lossy().into_owned())
        }

        /// Whether the file is there.
        pub fn has(&self, path: &str) -> bool {
            self.mirror.stamp(Path::new(path)).is_some()
        }
    }
}
