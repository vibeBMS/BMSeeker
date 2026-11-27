//! The command-line tool's commands (`bmseeker [--json] <noun> <verb> ...`),
//! on whatever files and network a platform gives them ([`Run`]): natively
//! the disk and `native::Native` (the `bmseeker` binary), in a web page its
//! storage and the browser's requests (the `bmseeker-web` crate). With
//! `--json` every command writes JSON Lines (one record a line, as it
//! goes); the `event` commands always do, and read records (JSON Lines, or
//! a JSON array or object) from a file or `-` (stdin).

#[cfg(not(target_arch = "wasm32"))]
mod event;
mod library;
mod link;
mod out;
mod table;

pub use out::{Line, Out};

use crate::config::Config;
use crate::fs::Store;
use crate::hosts::{Context, Want};
use crate::net::job::{run_job, Driver, TaskJob};
use crate::net::Task;
use crate::refusal::{Guarded, Refusal};
use clap::{Args, Parser, Subcommand};
use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;

#[derive(Parser)]
#[command(name = "bmseeker", version, about, propagate_version = true)]
pub struct Cli {
    /// Write JSON Lines (one record a line).
    #[arg(long, global = true)]
    pub json: bool,
    /// Settings (JSON; else $BMSEEKER_CONFIG, else <config folder>/bmseeker/config.json).
    #[arg(long, global = true, value_name = "FILE")]
    pub config: Option<PathBuf>,
    /// Also third parties' copies: other people's mirrors of dead sites and
    /// uploaders (darksabun.club, bms.hexlataia.xyz), packages by a chart's
    /// MD5 (Ginger, Konmai), the community archive (bms.kyouko.moe). What
    /// their sources bring is checked for the chart's MD5.
    #[arg(long, global = true)]
    third_party: bool,
    /// A song new to the library: its folder named after its charts
    /// (`[artist] title`, as BeMusicSeeker names one; the settings'
    /// `folder_name_format`), a folder its archive wraps everything in left
    /// out.
    #[arg(long, global = true)]
    name_folders: bool,
    /// A host that refused BMSeeker in an earlier run of this session (the
    /// window's): not asked in this one either. More than once for more.
    #[arg(long = "skip-host", global = true, value_name = "HOST")]
    skip_hosts: Vec<String>,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Links where authors put their archives.
    #[command(subcommand)]
    Link(link::Link),
    /// Where a song's links are known (its own aside).
    #[command(subcommand)]
    Song(link::Song),
    /// Difficulty tables.
    #[command(subcommand)]
    Table(table::Table),
    /// A library of songs: archives in, charts into their songs' folders.
    #[command(subcommand)]
    Library(library::Library),
    /// BMS events (manbow): their works and how their links fare.
    #[cfg(not(target_arch = "wasm32"))]
    #[command(subcommand)]
    Event(event::Event),
    /// The settings.
    #[command(subcommand)]
    Config(ConfigCommand),
}

#[derive(Subcommand)]
enum ConfigCommand {
    /// The settings in effect: the settings file's (or the defaults) with
    /// what the command line turns on, as one `config` record.
    Show,
}

impl Cli {
    /// `config` with what the command line turns on.
    pub fn settings(&self, config: Config) -> Config {
        Config {
            third_party: config.third_party || self.third_party,
            folder_naming: config.folder_naming || self.name_folders,
            ..config
        }
    }

    /// Run the command: its requests kept to the run's refusals (a
    /// service that refused is not asked again; each refusal written as a
    /// `refused` record when it comes).
    pub fn run(self, r: &Run) -> Result<(), String> {
        for h in &self.skip_hosts {
            r.cx.refused.earlier(h);
        }
        if r.cx.config.third_party {
            notice(&r.out, "third_party", THIRD_PARTY_NOTICE);
        }
        let told = |refusal: &Refusal| refused(&r.out, refusal);
        let guarded = Guarded::new(r.net, &r.cx).telling(&told);
        let run = &Run {
            net: &guarded,
            ..r.clone()
        };
        let done = match self.command {
            Command::Link(c) => link::link(run, c),
            Command::Song(c) => link::song(run, c),
            Command::Table(c) => table::table(run, c),
            Command::Library(c) => library::library(run, c),
            #[cfg(not(target_arch = "wasm32"))]
            Command::Event(c) => event::event(run, c),
            Command::Config(ConfigCommand::Show) => {
                let mut v = serde_json::json!({ "config": run.config() });
                // (as the settings file writes it: an f32 read as an f64
                // has digits of its own, 0.9900000095367432)
                if let Ok(m) = run.config().min_coverage.to_string().parse::<f64>() {
                    v["config"]["min_coverage"] = m.into();
                }
                run.out.record(v.clone(), || format!("{:#}", v["config"]));
                Ok(())
            }
        };
        // (one a task kept from the last answer: MEGA's -4)
        guarded.tell();
        done
    }
}

/// What the tool says once a run when third parties' copies are used
/// (`--third-party`, the setting): with `--json` the `text` of a
/// `{"notice": "third_party"}` record, else a note.
pub const THIRD_PARTY_NOTICE: &str = "bmseeker: third-party sources are on (other people's \
copies: mirrors, Ginger, Konmai, the community archive); keeping to their sites' terms and to \
the copyright holders' rights is up to you";

/// What a person is told once a run, whatever the command (`what`: which
/// notice, for a program): with `--json` a `notice` record (`{"notice":
/// what, "text"}`), else a note.
fn notice(out: &Out, what: &str, text: &str) {
    match out.json {
        true => out.record(
            serde_json::json!({"notice": what, "text": text}),
            String::new,
        ),
        false => out.note(text),
    }
}

/// A service that refused: a `refused` record (`{"refused": host,
/// "code", "status"?, "error"}`), and a note.
fn refused(out: &Out, r: &Refusal) {
    let mut v = serde_json::json!({
        "refused": r.host,
        "code": r.code(),
        "error": r.to_string(),
    });
    if let Some(s) = r.status() {
        v["status"] = s.into();
    }
    out.record(v, || format!("refused: {r}"));
}

/// A song looked for: by its chart's MD5, its title and artist.
#[derive(Args, Clone, Debug, Default)]
struct SongArgs {
    /// The song's title (a folder of many songs' archives: its archive).
    #[arg(short, long, default_value = "")]
    title: String,
    /// The song's artist.
    #[arg(short, long, default_value = "")]
    artist: String,
    /// One of its charts' MD5 (BMS SEARCH finds its work by it).
    #[arg(long, default_value = "")]
    md5: String,
}

impl SongArgs {
    fn want(&self) -> Want {
        Want {
            title: self.title.clone(),
            artist: self.artist.clone(),
        }
    }
}

/// What every command runs with: where its records go, the settings and
/// hosts, the files libraries are in, what does its requests, and what it
/// reads for `-`.
#[derive(Clone)]
pub struct Run<'a> {
    pub out: Out,
    pub cx: Context,
    pub fs: Arc<dyn Store + Send + Sync>,
    pub net: &'a dyn Driver,
    pub stdin: Stdin,
}

/// What a command reads for a file named `-`: natively stdin, in a web
/// page what the page hands the run.
pub type Stdin = Rc<RefCell<dyn std::io::Read>>;

impl Run<'_> {
    fn config(&self) -> &Config {
        &self.cx.config
    }

    /// The text of the file `path` (the run's files), or stdin's for `-`.
    fn read_text(&self, path: &Path) -> Result<String, String> {
        let bytes = if path == Path::new("-") {
            let mut b = Vec::new();
            std::io::Read::read_to_end(&mut *self.stdin.borrow_mut(), &mut b)
                .map_err(|e| format!("stdin: {e}"))?;
            b
        } else {
            crate::fs::Fs::read(&*self.fs, path).map_err(|e| format!("{}: {e}", path.display()))?
        };
        String::from_utf8(bytes).map_err(|_| format!("{}: not UTF-8 text", path.display()))
    }

    /// `task` run to its end.
    fn task<T: Task + ?Sized>(&self, task: &mut T) -> T::Output {
        run_job(&mut TaskJob::new(task), self.net)
    }

    /// `url`'s body (a list of links, a venue's list of works).
    fn get(&self, url: &str) -> crate::net::Answer {
        let kind = crate::net::Kind::Listing;
        self.task(&mut crate::net::get_as(url.to_string(), kind, |a| a))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::net::job::{Effect, Fed};
    use crate::net::NetError;
    use std::cell::RefCell;
    use std::rc::Rc;

    /// MEGA's API answers -4 (it limits BMSeeker's requests): the only
    /// request of the run, and still a `refused` record.
    #[test]
    fn the_last_refusal_is_written() {
        struct Limiting;
        impl Driver for Limiting {
            fn perform(&self, effect: Effect) -> Fed {
                match effect {
                    Effect::Http(_) => Fed::Http(Ok(b"-4".to_vec())),
                    Effect::Download { .. } => Fed::Downloaded(Err(NetError::Status(404).into())),
                    Effect::Sleep(_) => Fed::Woke,
                }
            }
        }
        let lines: Rc<RefCell<Vec<String>>> = Rc::default();
        let sink = lines.clone();
        let run = Run {
            out: Out::new(true, move |l| {
                if let Line::Record(r) = l {
                    sink.borrow_mut().push(r.to_string());
                }
            }),
            cx: Context::default(),
            fs: Arc::new(crate::fs::Mem::default()),
            net: &Limiting,
            stdin: Rc::new(RefCell::new(std::io::empty())),
        };
        let link = "https://mega.nz/file/bzQy3bYS#Og0LBoWkfbKwxUceGvzLHDl2exyI0l1DiQ3XNw79zMw";
        let cli = Cli::try_parse_from(["bmseeker", "--json", "link", "resolve", link]).unwrap();
        cli.run(&run).unwrap();
        let lines = lines.borrow();
        assert_eq!(lines.len(), 2, "{lines:?}");
        let refused: serde_json::Value = serde_json::from_str(&lines[1]).unwrap();
        assert_eq!(refused["refused"], "mega.nz");
        assert_eq!(refused["code"], "rate_limited");
    }

    /// The protocol as the window reads it: `testdata/records.jsonl`, each
    /// line a run (its command line, what it read on stdin, the lines it
    /// wrote in order, its exit code), which `gui/gui_test.go` replays as
    /// the CLI. Its runs are the window's: the settings, a table's missing
    /// charts, those chosen downloaded (one into the library, one a page,
    /// one whose source refuses, one the table names by its SHA-256 alone,
    /// a separate chart whose song is there and whose own link is dead, the
    /// two charts of one pack, one named by its SHA-256 without a link, and
    /// a hash the table does not list), one the library has chosen again,
    /// a run after a refusal and with third parties' sources, and the
    /// script a page's window runs. The commands must write them as the
    /// file has them; `BMSEEKER_BLESS=1 cargo test --features cli records`
    /// writes it anew (a change of the protocol, which the window's test
    /// then reads). (Not on Windows: a path's separator there is `\`.)
    #[cfg(unix)]
    #[test]
    fn the_records_the_window_reads() {
        use crate::install::md5_hex;
        use crate::net::job::Served;
        const TABLE: &str = "https://t.example/table.json";
        let chart = |title: &str, artist: &str| {
            format!("#TITLE {title}\n#ARTIST {artist}\n#WAV01 01.wav\n#00111:01\n")
        };
        let (song_a, song_d) = (chart("Song A", "x"), chart("Song D", "w"));
        let (pack_1, pack_2) = (chart("Pack [1]", "p"), chart("Pack [2]", "p"));
        let (a, b, c) = (md5_hex(song_a.as_bytes()), "b".repeat(32), "c".repeat(32));
        // (tables that give these charts their SHA-256 alone)
        let d = crate::chart::sha256_hex(song_d.as_bytes());
        let g = crate::chart::sha256_hex(chart("Song G", "v").as_bytes());
        let e = md5_hex(chart("Song A [EX]", "x").as_bytes());
        let (p1, p2) = (md5_hex(pack_1.as_bytes()), md5_hex(pack_2.as_bytes()));
        let data = serde_json::json!([
            {"md5": a, "title": "Song A", "artist": "x", "level": "1",
                "url": "https://a.example/a.zip", "url_diff": ""},
            {"md5": b, "title": "Song B", "artist": "y", "level": "2",
                "url": "https://b.example/b", "url_diff": ""},
            {"md5": c, "title": "Song C", "artist": "z", "level": "2", "url": "", "url_diff": ""},
            {"sha256": d, "title": "Song D", "artist": "w", "level": "2",
                "url": "https://d.example/d.zip", "url_diff": ""},
            {"md5": e, "title": "Song A [EX]", "artist": "x", "level": "2",
                "url": "https://a.example/a.zip", "url_diff": "https://a.example/dead.bms"},
            {"md5": p1, "title": "Pack [1]", "artist": "p", "level": "2",
                "url": "https://p.example/p.zip", "url_diff": ""},
            {"md5": p2, "title": "Pack [2]", "artist": "p", "level": "2",
                "url": "https://p.example/p.zip", "url_diff": ""},
            {"sha256": g, "title": "Song G", "artist": "v", "level": "2", "url": "", "url_diff": ""},
        ]);
        struct Hosts(Vec<u8>, Vec<(&'static str, Vec<u8>)>);
        impl Driver for Hosts {
            fn perform(&self, effect: Effect) -> Fed {
                match effect {
                    Effect::Http(r) => Fed::Http(match r.url() {
                        TABLE => {
                            Ok(br#"{"name":"Fixture","symbol":"F","data_url":"data.json"}"#
                                .to_vec())
                        }
                        "https://t.example/data.json" => Ok(self.0.clone()),
                        u if crate::net::host(u) == "api.bmssearch.net" => {
                            Err(NetError::Status(403))
                        }
                        _ => Err(NetError::Status(404)),
                    }),
                    Effect::Download { target, .. } => {
                        Fed::Downloaded(match self.1.iter().find(|(u, _)| *u == target.url) {
                            Some((_, body)) => Ok(Served::bytes(body.clone(), None, None)),
                            None => Err(NetError::Status(404).into()),
                        })
                    }
                    Effect::Sleep(_) => Fed::Woke,
                }
            }
        }
        let zip = crate::testkit::zip;
        let net = Hosts(
            data.to_string().into_bytes(),
            vec![
                (
                    "https://a.example/a.zip",
                    zip(&[("Song A/a.bms", song_a.as_str()), ("Song A/01.wav", "RIFF")]),
                ),
                (
                    "https://d.example/d.zip",
                    zip(&[("Song D/d.bms", song_d.as_str()), ("Song D/01.wav", "RIFF")]),
                ),
                (
                    "https://p.example/p.zip",
                    zip(&[
                        ("Pack/1.bms", pack_1.as_str()),
                        ("Pack/2.bms", pack_2.as_str()),
                        ("Pack/01.wav", "RIFF"),
                    ]),
                ),
                ("https://b.example/b", b"<html>a page</html>".to_vec()),
            ],
        );
        // the defaults, but a user agent no version changes, the sources the
        // hosts here answer, and their table
        let config = Config {
            user_agent: "BMSeeker/fixture".into(),
            sources: ["table", "bmssearch"].map(String::from).to_vec(),
            tables: vec![TABLE.into()],
            ..Config::default()
        };
        let fs: Arc<crate::fs::Mem> = Arc::default();
        // the window's rows in order, and one gone from the table since
        let chosen = [&a, &b, &c, &d, &e, &p1, &p2, &g, &"f".repeat(32)]
            .map(|h| format!("{h}\n"))
            .concat();
        let get = [
            "--json",
            "table",
            "missing",
            "--library=/lib",
            "--get",
            "--chosen=-",
            "--",
            TABLE,
        ];
        let again = format!("{a}\n");
        let runs: [(&[&str], &str); 6] = [
            (&["--json", "config", "show"], ""),
            (
                &["--json", "table", "missing", "--library=/lib", "--", TABLE],
                "",
            ),
            (&get, &chosen),
            (&get, &again),
            (
                &[
                    "--json",
                    "--third-party",
                    "--skip-host=bmssearch.net",
                    "library",
                    "pending",
                    "--library=/lib",
                ],
                "",
            ),
            (&["--json", "link", "page-script"], ""),
        ];
        let mut file = String::new();
        for (args, stdin) in runs {
            let lines: Rc<RefCell<Vec<serde_json::Value>>> = Rc::default();
            let sink = lines.clone();
            let cli = Cli::try_parse_from(std::iter::once(&"bmseeker").chain(args)).unwrap();
            let run = Run {
                out: Out::new(cli.json, move |l| {
                    sink.borrow_mut().push(match l {
                        Line::Record(r) => serde_json::json!({"record": serde_json::from_str::<serde_json::Value>(r).unwrap()}),
                        Line::Note(n) => serde_json::json!({"note": n}),
                    })
                }),
                cx: Context::new(cli.settings(config.clone())),
                fs: fs.clone(),
                net: &net,
                stdin: Rc::new(RefCell::new(std::io::Cursor::new(stdin.to_string()))),
            };
            // (as the binary ends)
            let exit = match cli.run(&run) {
                Ok(()) => 0,
                Err(e) => {
                    run.out.note(format!("bmseeker: {e}"));
                    1
                }
            };
            let mut run = serde_json::json!({"args": args});
            if !stdin.is_empty() {
                run["stdin"] = stdin.into();
            }
            run["lines"] = serde_json::json!(*lines.borrow());
            run["exit"] = exit.into();
            file.push_str(&format!("{run}\n"));
        }
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/testdata/records.jsonl");
        if std::env::var("BMSEEKER_BLESS").as_deref() == Ok("1") {
            std::fs::write(path, &file).unwrap();
        }
        let kept = std::fs::read_to_string(path).unwrap_or_default();
        for (i, (now, then)) in file.lines().zip(kept.lines()).enumerate() {
            assert_eq!(
                now,
                then,
                "testdata/records.jsonl, run {}: the protocol changed (BMSEEKER_BLESS=1 writes it)",
                i + 1
            );
        }
        assert_eq!(
            file.lines().count(),
            kept.lines().count(),
            "testdata/records.jsonl: runs"
        );
    }

    /// What reads the records gone (the window closed): the run ends, no
    /// download started after (`main` cancels it when a line cannot be
    /// written).
    #[test]
    fn a_reader_gone_ends_the_run() {
        use crate::net::job::Served;
        const TABLE: &str = "https://t.example/table.json";
        struct Hosts(Vec<u8>, RefCell<Vec<String>>);
        impl Driver for Hosts {
            fn perform(&self, effect: Effect) -> Fed {
                match effect {
                    Effect::Http(r) => Fed::Http(match r.url() {
                        TABLE => Ok(br#"{"name":"T","symbol":"t","data_url":"d.json"}"#.to_vec()),
                        "https://t.example/d.json" => Ok(self.0.clone()),
                        _ => Err(NetError::Status(404)),
                    }),
                    Effect::Download { target, .. } => {
                        self.1.borrow_mut().push(target.url);
                        Fed::Downloaded(Ok(Served::bytes(b"<html>".to_vec(), None, None)))
                    }
                    Effect::Sleep(_) => Fed::Woke,
                }
            }
        }
        /// A pipe whose reader goes once a download's record comes.
        struct Closing;
        impl std::io::Write for Closing {
            fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
                match String::from_utf8_lossy(b).contains("\"state\"") {
                    true => Err(std::io::ErrorKind::BrokenPipe.into()),
                    false => Ok(b.len()),
                }
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        let data = serde_json::json!([
            {"md5": "a".repeat(32), "title": "A", "level": "1", "url": "https://h.example/a.zip"},
            {"md5": "b".repeat(32), "title": "B", "level": "1", "url": "https://h.example/b.zip"},
        ]);
        let net = Hosts(data.to_string().into_bytes(), RefCell::default());
        let progress = crate::progress::Progress::new();
        let p = progress.clone();
        let run = Run {
            out: Out::writing(true, || Closing, std::io::sink, move || p.cancel()),
            cx: Context::default().with_progress(progress),
            fs: Arc::new(crate::fs::Mem::default()),
            net: &net,
            stdin: Rc::new(RefCell::new(std::io::empty())),
        };
        let args = [
            "bmseeker",
            "--json",
            "table",
            "missing",
            "-L",
            "/lib",
            "--get",
            "--own-links",
            TABLE,
        ];
        let e = Cli::try_parse_from(args).unwrap().run(&run).unwrap_err();
        assert_eq!(e, NetError::Cancelled.to_string());
        assert_eq!(*net.1.borrow(), ["https://h.example/a.zip"]);
    }

    /// The window's command lines (`gui/app.go`): an option's value
    /// attached to it and the links after `--` are values and links,
    /// whatever they start with.
    #[test]
    fn values_that_look_like_options() {
        let cli = Cli::try_parse_from([
            "bmseeker",
            "--json",
            "--skip-host=-x.evil.example",
            "library",
            "install",
            "--library=/lib",
            "--title=-ERROR",
            "--artist=--reinstall",
            "--md5=",
            "--diff",
            "--",
            "--config=/x.json",
            "--third-party",
        ])
        .unwrap();
        assert!(cli.config.is_none() && !cli.third_party);
        assert_eq!(cli.skip_hosts, ["-x.evil.example"]);
        let Command::Library(library::Library::Install { links, song, .. }) = cli.command else {
            panic!("not library install");
        };
        assert_eq!(links, ["--config=/x.json", "--third-party"]);
        assert_eq!(
            (song.title.as_str(), song.artist.as_str()),
            ("-ERROR", "--reinstall")
        );
        for line in [
            &["table", "missing", "--library=/lib", "--", "--help"][..],
            &["library", "pending", "--library=-lib"],
            &["library", "import", "--library=/lib", "--", "-x.zip"],
            &[
                "library",
                "import",
                "--library=/lib",
                "--name=--reinstall",
                "--",
                "x",
            ],
        ] {
            let args = ["bmseeker", "--json"].iter().chain(line);
            assert!(Cli::try_parse_from(args).is_ok(), "{line:?}");
        }
    }

    /// `link page-script`: the script a window showing a link's page
    /// runs is the library's (the window has none of its own).
    #[test]
    fn the_page_script_is_the_librarys() {
        struct Offline;
        impl Driver for Offline {
            fn perform(&self, effect: Effect) -> Fed {
                panic!("{effect:?}: nothing is asked for");
            }
        }
        let lines: Rc<RefCell<Vec<serde_json::Value>>> = Rc::default();
        let sink = lines.clone();
        let run = Run {
            out: Out::new(true, move |l| {
                if let Line::Record(r) = l {
                    sink.borrow_mut().push(serde_json::from_str(r).unwrap());
                }
            }),
            cx: Context::default(),
            fs: Arc::new(crate::fs::Mem::default()),
            net: &Offline,
            stdin: Rc::new(RefCell::new(std::io::empty())),
        };
        let cli = Cli::try_parse_from(["bmseeker", "--json", "link", "page-script"]).unwrap();
        cli.run(&run).unwrap();
        let lines = lines.borrow();
        assert_eq!(lines.len(), 1, "{lines:?}");
        assert_eq!(lines[0]["script"], crate::hosts::PRESS_DOWNLOAD_JS);
        assert!(crate::hosts::PRESS_DOWNLOAD_JS.contains("location.hostname"));
    }

    /// `link get` into a folder: the file is put right in it, under one
    /// file name, whatever its host named it (a Drive folder's listing).
    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn link_get_keeps_to_its_folder() {
        const ZIP: &[u8] = b"PK\x05\x06\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0";
        const LISTING: &[u8] = br#"<div class="flip-entry"><a href="https://drive.google.com/file/d/A/view"><div class="flip-entry-title">../../evil.zip</div></a></div>"#;
        struct Drive;
        impl Driver for Drive {
            fn perform(&self, effect: Effect) -> Fed {
                match effect {
                    Effect::Http(_) => Fed::Http(Ok(LISTING.to_vec())),
                    Effect::Download { slot, .. } => {
                        std::fs::write(&slot, ZIP).unwrap();
                        Fed::Downloaded(Ok(crate::net::job::Served {
                            blob: crate::fs::Blob::File(slot),
                            name: None,
                            at: None,
                            head: ZIP.to_vec(),
                        }))
                    }
                    Effect::Sleep(_) => Fed::Woke,
                }
            }
        }
        let tmp = crate::testkit::tmp("link-get");
        let out = tmp.join("a/b/out");
        std::fs::create_dir_all(&out).unwrap();
        let run = Run {
            out: Out::new(true, |_| {}),
            cx: Context::default(),
            fs: Arc::new(crate::fs::Disk),
            net: &Drive,
            stdin: Rc::new(RefCell::new(std::io::empty())),
        };
        let link = "https://drive.google.com/drive/folders/F";
        let args = ["bmseeker", "link", "get", link, "-o", out.to_str().unwrap()];
        Cli::try_parse_from(args).unwrap().run(&run).unwrap();
        let names: Vec<_> = std::fs::read_dir(&out)
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(names, ["_.._evil.zip"]);
        assert!(!tmp.join("a/evil.zip").exists());
    }

    /// `link get -o` what is no file (a device, a pipe, a socket): not
    /// written to, nor removed.
    #[cfg(unix)]
    #[test]
    fn link_get_into_no_file() {
        use std::os::unix::fs::FileTypeExt;
        let tmp = crate::testkit::tmp("link-get-socket");
        let socket = tmp.join("socket");
        let _listening = std::os::unix::net::UnixListener::bind(&socket).unwrap();
        struct Offline;
        impl Driver for Offline {
            fn perform(&self, effect: Effect) -> Fed {
                panic!("{effect:?}: nothing is asked for");
            }
        }
        let run = Run {
            out: Out::new(true, |_| {}),
            cx: Context::default(),
            fs: Arc::new(crate::fs::Disk),
            net: &Offline,
            stdin: Rc::new(RefCell::new(std::io::empty())),
        };
        let args = ["bmseeker", "link", "get", "https://h.example/a.zip", "-o"];
        let args = args.into_iter().chain([socket.to_str().unwrap()]);
        let e = Cli::try_parse_from(args).unwrap().run(&run).unwrap_err();
        assert!(e.contains("not a file"), "{e}");
        assert!(std::fs::symlink_metadata(&socket)
            .unwrap()
            .file_type()
            .is_socket());
    }
}
