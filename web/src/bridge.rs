//! A command line in, its lines out: the command-line tool's protocol
//! (`--json`: JSON Lines records; notes; an exit code), on a page's files
//! and network.

use bmseeker::cli::{Cli, Line, Out, Run};
use bmseeker::config::Config;
use bmseeker::fs::Store;
use bmseeker::hosts::Context;
use bmseeker::install::job::Driver;
use clap::Parser;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

/// Run `args` (without the program's name) with `config` on `fs` and
/// `net`, `stdin` what it reads for `-`, each line to `sink`: 0 when it
/// went well, 1 when the command failed, 2 for a command line it does not
/// take. A download that fails is a page to open (the proxy passes
/// archives only).
pub fn run(
    args: &[String],
    stdin: &str,
    config: Config,
    fs: Arc<dyn Store + Send + Sync>,
    net: &dyn Driver,
    sink: impl Fn(Line) + 'static,
) -> i32 {
    let cli = match Cli::try_parse_from(
        std::iter::once("bmseeker".to_string()).chain(args.iter().cloned()),
    ) {
        Ok(c) => c,
        Err(e) => {
            let text = e.render().to_string();
            for l in text.lines() {
                sink(Line::Note(l));
            }
            return if e.use_stderr() { 2 } else { 0 };
        }
    };
    if cli.config.is_some() {
        sink(Line::Note("bmseeker: a web page reads no settings file"));
        return 2;
    }
    let cx = Context {
        failures_are_pages: true,
        ..Context::new(cli.settings(config))
    };
    let out = Out::new(cli.json, sink);
    let run = Run {
        out: out.clone(),
        cx,
        fs,
        net,
        stdin: Rc::new(RefCell::new(std::io::Cursor::new(stdin.to_string()))),
    };
    match cli.run(&run) {
        Ok(()) => 0,
        Err(e) => {
            out.note(format!("bmseeker: {e}"));
            1
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mirror::{Change, Kept, Mirror, Source};
    use bmseeker::fs::Fs;
    use bmseeker::install::job::{Effect, Fed, Served};
    use bmseeker::net::{NetError, Request};
    use std::path::Path;

    const CHART: &str = "#PLAYER 1\n#TITLE Song A\n#ARTIST x\n#WAV01 01.wav\n#00111:01\n";

    /// Pages and files by URL.
    struct Canned(Vec<(&'static str, Vec<u8>)>);

    impl Driver for Canned {
        fn perform(&self, effect: Effect) -> Fed {
            let find = |u: &str| self.0.iter().find(|(k, _)| *k == u).map(|(_, b)| b.clone());
            match effect {
                Effect::Http(Request::Get(u, _) | Request::Post { url: u, .. }) => {
                    Fed::Http(find(&u).ok_or(NetError::Status(404)))
                }
                Effect::Download { target, .. } => Fed::Downloaded(
                    find(&target.url)
                        .map(|b| Served::bytes(b, None, None))
                        .ok_or_else(|| NetError::Status(404).into()),
                ),
                Effect::Sleep(_) => Fed::Woke,
            }
        }
    }

    /// The vibeBMS library: one song there, the player's own files beside.
    fn library() -> Arc<Mirror> {
        let source: Source = Box::new(|p: &Path| match p.to_str() {
            Some("/bms/Old/o.bms") => Ok(b"#TITLE Old\n#ARTIST y\n".to_vec()),
            _ => Err(std::io::ErrorKind::NotFound.into()),
        });
        let writable = vec!["/bms".into(), "/bmseeker".into()];
        let m = Mirror::new(source, Box::new(Kept::default()), writable, false, || 9);
        m.mount("/bms/Old/o.bms".into(), 20, 1);
        m.mount("/store/scores.json".into(), 2, 1);
        Arc::new(m)
    }

    fn lines(
        args: &[&str],
        stdin: &str,
        fs: Arc<Mirror>,
        net: &dyn Driver,
    ) -> (i32, Vec<serde_json::Value>, Vec<String>) {
        let got: Rc<RefCell<(Vec<serde_json::Value>, Vec<String>)>> = Rc::default();
        let sink = got.clone();
        let config = Config {
            downloads: "/bmseeker/downloads".into(),
            ..Default::default()
        };
        let args: Vec<String> = args.iter().map(|a| a.to_string()).collect();
        let code = run(&args, stdin, config, fs, net, move |l| match l {
            Line::Record(r) => sink.borrow_mut().0.push(serde_json::from_str(r).unwrap()),
            Line::Note(n) => sink.borrow_mut().1.push(n.to_string()),
        });
        let (records, notes) = got.take();
        (code, records, notes)
    }

    #[test]
    fn installs_into_the_library_as_the_cli_does() {
        let song = bmseeker::testkit::zip(&[("Song A/a.bms", CHART), ("Song A/01.wav", "RIFF")]);
        let net = Canned(vec![("https://h.example/a.zip", song)]);
        let m = library();
        let (code, records, _) = lines(
            &[
                "--json",
                "library",
                "install",
                "-L",
                "/bms",
                "--title",
                "Song A",
                "--own-links",
                "https://h.example/a.zip",
            ],
            "",
            m.clone(),
            &net,
        );
        assert_eq!(code, 0);
        assert_eq!(records.len(), 1, "{records:?}");
        assert_eq!(records[0]["state"], "added");
        assert_eq!(records[0]["path"], "/bms/Song A");
        let written: Vec<String> = m
            .changes()
            .into_iter()
            .filter_map(|c| match c {
                Change::Write { path, .. } => Some(path.display().to_string()),
                _ => None,
            })
            .collect();
        // (as the CLI does: a folder named after the song, the archive's as it is)
        assert_eq!(
            written,
            ["/bms/Song A/Song A/01.wav", "/bms/Song A/Song A/a.bms"]
        );
        // the table's charts the library lacks: read through the mirror
        let table = br#"<html><head><meta name="bmstable" content="h.json"></head></html>"#;
        let header = br#"{"name":"T","symbol":"t","data_url":"d.json"}"#;
        let md5 = bmseeker::install::md5_hex(CHART.as_bytes());
        let data = format!(
            r#"[{{"level":"1","title":"Song A","artist":"x","md5":"{md5}"}},{{"level":"2","title":"B","artist":"z","md5":"00000000000000000000000000000000"}}]"#
        );
        let net = Canned(vec![
            ("https://t.example/", table.to_vec()),
            ("https://t.example/h.json", header.to_vec()),
            ("https://t.example/d.json", data.into_bytes()),
        ]);
        let (code, records, notes) = lines(
            &[
                "--json",
                "table",
                "missing",
                "https://t.example/",
                "-L",
                "/bms",
                "--own-links",
            ],
            "",
            m.clone(),
            &net,
        );
        assert_eq!(code, 0, "{notes:?}");
        assert_eq!(records.len(), 1, "{records:?}");
        assert_eq!(records[0]["title"], "B");
        assert!(
            notes.iter().any(|n| n == "1 of 2 charts missing"),
            "{notes:?}"
        );
        // those chosen, read as the CLI reads stdin (`--chosen -`)
        let (code, records, notes) = lines(
            &[
                "--json",
                "table",
                "missing",
                "https://t.example/",
                "-L",
                "/bms",
                "--own-links",
                "--chosen=-",
            ],
            &format!("{md5}\n"),
            m.clone(),
            &net,
        );
        assert_eq!((code, records.len()), (0, 0), "{records:?}");
        assert!(
            notes.iter().any(|n| n == "0 of 1 charts missing"),
            "{notes:?}"
        );
        // the scan's cache kept beside the downloads, the player's files untouched
        assert!(m
            .stamp(Path::new("/bmseeker/downloads/scan.json"))
            .is_some());
        assert!(m.changes().iter().all(|c| !matches!(c,
            Change::Write { path, .. } | Change::Remove { path, .. } if path.starts_with("/store"))));
    }

    #[test]
    fn a_failure_and_a_bad_command_line() {
        let net = Canned(Vec::new());
        let (code, records, notes) = lines(
            &[
                "--json",
                "library",
                "install",
                "-L",
                "/bms",
                "--title",
                "X",
                "--own-links",
                "https://h.example/none.zip",
            ],
            "",
            library(),
            &net,
        );
        assert_eq!(code, 1);
        assert_eq!(records[0]["state"], "page", "{records:?}");
        assert_eq!(notes.last().unwrap(), "bmseeker: X: not installed");
        let (code, _, notes) = lines(&["library", "frobnicate"], "", library(), &net);
        assert_eq!(code, 2);
        assert!(notes[0].starts_with("error:"), "{notes:?}");
        let (code, _, notes) = lines(&["--version"], "", library(), &net);
        let version = concat!("bmseeker ", env!("CARGO_PKG_VERSION"));
        assert_eq!((code, notes[0].as_str()), (0, version));
    }

    /// `licenses`, as the page's Settings runs it: the texts, asking
    /// nothing and writing nothing.
    #[test]
    fn licenses_need_no_files() {
        let net = Canned(Vec::new());
        let m = library();
        let (code, records, notes) = lines(&["--json", "licenses"], "", m.clone(), &net);
        assert_eq!((code, records.len()), (0, 1), "{notes:?}");
        let text = records[0]["text"].as_str().unwrap();
        assert!(
            text.starts_with("MIT License\n") && text.contains("\n- `github.com/egoist/mygo`: ")
        );
        assert!(m.changes().is_empty());
    }

    /// Answers 429 on refusing.example, else 404; notes what was asked.
    struct Refusing(RefCell<Vec<String>>);

    impl Driver for Refusing {
        fn perform(&self, effect: Effect) -> Fed {
            let url = match &effect {
                Effect::Http(r) => r.url().to_string(),
                Effect::Download { target, .. } => target.url.clone(),
                Effect::Sleep(_) => return Fed::Woke,
            };
            self.0.borrow_mut().push(url.clone());
            let n = if url.contains("refusing.example") {
                429
            } else {
                404
            };
            match effect {
                Effect::Http(_) => Fed::Http(Err(NetError::Status(n))),
                _ => Fed::Downloaded(Err(NetError::Status(n).into())),
            }
        }
    }

    #[test]
    fn refusals_are_kept_and_told() {
        let net = Refusing(RefCell::default());
        let (_, records, notes) = lines(
            &[
                "--json",
                "--third-party",
                "--skip-host",
                "earlier.example",
                "library",
                "install",
                "-L",
                "/bms",
                "--title",
                "X",
                "--own-links",
                "https://refusing.example/a.zip",
                "https://refusing.example/b.zip",
                "https://earlier.example/c.zip",
                "https://h.example/d.zip",
            ],
            "",
            library(),
            &net,
        );
        // each host asked once at most: the refusing one once, the one
        // that refused earlier not at all
        assert_eq!(
            *net.0.borrow(),
            ["https://refusing.example/a.zip", "https://h.example/d.zip"]
        );
        // told once, as a record (a refusal is no page)
        let refused: Vec<_> = records
            .iter()
            .filter(|r| r.get("refused").is_some())
            .collect();
        assert_eq!(refused.len(), 1, "{records:?}");
        assert_eq!(refused[0]["refused"], "refusing.example");
        assert_eq!(refused[0]["status"], 429);
        assert_eq!(refused[0]["code"], "status");
        assert!(refused[0]["error"]
            .as_str()
            .unwrap()
            .contains("skipped for the rest of this run"));
        assert!(!records
            .iter()
            .any(|r| r["state"] == "page" && r["link"].as_str().unwrap().contains("refusing")));
        // the third-party notice, once, as a record
        let notice = records
            .iter()
            .filter(|r| r["notice"] == "third_party")
            .filter(|r| r["text"] == bmseeker::cli::THIRD_PARTY_NOTICE);
        assert_eq!(notice.count(), 1, "{records:?}");
        assert!(
            !notes.iter().any(|n| n.contains("third-party")),
            "{notes:?}"
        );
    }
}
