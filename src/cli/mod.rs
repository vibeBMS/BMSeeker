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
use crate::install::job::{run_job, Driver, TaskJob};
use crate::net::Task;
use crate::refusal::{Guarded, Refusal};
use clap::{Args, Parser, Subcommand};
use std::path::PathBuf;
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
            r.out.note(THIRD_PARTY_NOTICE);
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
        };
        // (one a task kept from the last answer: MEGA's -4)
        guarded.tell();
        done
    }
}

/// What the tool says once a run when third parties' copies are used
/// (`--third-party`, the setting).
pub const THIRD_PARTY_NOTICE: &str = "bmseeker: third-party sources are on (other people's \
copies: mirrors, Ginger, Konmai, the community archive); keeping to their sites' terms and to \
the copyright holders' rights is up to you";

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
/// hosts, the files libraries are in, and what does its requests.
#[derive(Clone)]
pub struct Run<'a> {
    pub out: Out,
    pub cx: Context,
    pub fs: Arc<dyn Store + Send + Sync>,
    pub net: &'a dyn Driver,
}

impl Run<'_> {
    fn config(&self) -> &Config {
        &self.cx.config
    }

    /// `task` run to its end.
    fn task<T: Task + ?Sized>(&self, task: &mut T) -> T::Output {
        run_job(&mut TaskJob::new(task), self.net)
    }

    /// `url`'s body.
    fn get(&self, url: &str) -> crate::net::Answer {
        self.task(&mut crate::net::get_then(url.to_string(), |a| a))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::install::job::{Effect, Fed};
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
                        Fed::Downloaded(Ok(crate::install::job::Served {
                            blob: crate::unpack::Blob::File(slot),
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
}
