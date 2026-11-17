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

    /// Run the command.
    pub fn run(self, r: &Run) -> Result<(), String> {
        match self.command {
            Command::Link(c) => link::link(r, c),
            Command::Song(c) => link::song(r, c),
            Command::Table(c) => table::table(r, c),
            Command::Library(c) => library::library(r, c),
            #[cfg(not(target_arch = "wasm32"))]
            Command::Event(c) => event::event(r, c),
        }
    }
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
