//! `library import`, `library place`, `library pending`, `library install`.

use super::{Run, SongArgs};
use crate::import::Imported;
use crate::install::{self, Done};
use crate::placement::{self, Located, Rules};
use crate::sources::{Query, Sources};
use clap::Subcommand;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

#[derive(Subcommand)]
pub(super) enum Library {
    /// A downloaded file into the library: an archive unpacked, a chart
    /// into its song's folder.
    Import {
        file: PathBuf,
        #[arg(short = 'L', long)]
        library: PathBuf,
        #[command(flatten)]
        again: Reinstall,
    },
    /// Which song's folder a chart of its own (差分) belongs in; --to moves
    /// it there.
    Place {
        chart: PathBuf,
        #[arg(short = 'L', long)]
        library: PathBuf,
        /// Move it into this folder.
        #[arg(long)]
        to: Option<PathBuf>,
        /// How much of its sounds a folder must hold (percent).
        #[arg(long)]
        min: Option<f32>,
    },
    /// Charts that wait for their song.
    Pending {
        #[arg(short = 'L', long)]
        library: PathBuf,
    },
    /// A song (or its separate chart) from its links into the library: the
    /// links in turn, then the sources'.
    Install {
        links: Vec<String>,
        #[arg(short = 'L', long)]
        library: PathBuf,
        /// The separate chart (差分), not the song.
        #[arg(long)]
        diff: bool,
        #[command(flatten)]
        song: SongArgs,
        /// The given links only.
        #[arg(long)]
        own_links: bool,
        #[command(flatten)]
        again: Reinstall,
    },
}

/// What becomes of a song whose charts the library has.
#[derive(clap::Args, Clone, Copy, Default)]
pub(super) struct Reinstall {
    /// Bring its files in again, newer ones over those there (else only
    /// those its folder lacks).
    #[arg(long)]
    reinstall: bool,
}

/// `r`, songs whose charts the library has brought in again with
/// `reinstall` (else as the settings say).
pub(super) fn reinstalling<'a>(r: &Run<'a>, reinstall: bool) -> Run<'a> {
    let mut r = r.clone();
    if reinstall {
        let config = crate::config::Config {
            reinstall: true,
            ..(*r.cx.config).clone()
        };
        r.cx.config = std::sync::Arc::new(config);
    }
    r
}

pub(super) fn library(r: &Run, c: Library) -> Result<(), String> {
    match c {
        Library::Import {
            file,
            library,
            again,
        } => {
            let req = crate::install::Request {
                reinstall: again.reinstall || r.config().reinstall,
                naming: install::Naming::of(r.config(), Default::default()),
                ..Default::default()
            };
            let lib = &mut scan(r, &library);
            let (got, placed) = crate::import::import(&*r.fs, &file, lib, &req)?;
            let (v, text) = match &got {
                Imported::Unpacked(d, n) => (
                    json!({"state": "added", "path": d, "files": n}),
                    format!("added {} ({n} files)", d.display()),
                ),
                Imported::Owned(d) => (
                    json!({"state": "owned", "path": d}),
                    format!("in the library already: {}", d.display()),
                ),
                Imported::Placed(p) => (
                    json!({"state": "placed", "path": p}),
                    format!("placed {}", p.display()),
                ),
                Imported::Waiting(p) => (
                    json!({"state": "waiting", "path": p}),
                    format!("{} waits for its song", p.display()),
                ),
                Imported::Ask(p, c) => (
                    json!({"state": "ask", "path": p, "candidates": c.iter().map(|(d, l)| json!({"folder": d, "label": l})).collect::<Vec<_>>()}),
                    format!(
                        "{}: which song? {}",
                        p.display(),
                        c.iter()
                            .map(|(_, l)| l.as_str())
                            .collect::<Vec<_>>()
                            .join(" / ")
                    ),
                ),
                Imported::Other(p) => (
                    json!({"state": "other", "path": p}),
                    format!("{}: not a song", p.display()),
                ),
            };
            r.out.record(v, || text);
            for p in placed {
                r.out.record(json!({"state": "placed", "path": p}), || {
                    format!("placed {}", p.display())
                });
            }
            Ok(())
        }
        Library::Place {
            chart,
            library,
            to,
            min,
        } => {
            if let Some(dir) = to {
                return match placement::bring(&*r.fs, &chart, &dir) {
                    Ok(Some(at)) => {
                        r.out.record(json!({"state": "placed", "path": at}), || {
                            at.display().to_string()
                        });
                        Ok(())
                    }
                    Ok(None) => Err(format!("{} stays where it is", chart.display())),
                    Err(e) => Err(format!("{}: {e}", chart.display())),
                };
            }
            let mut rules = Rules {
                min_coverage: r.config().min_coverage,
                ..Default::default()
            };
            if let Some(m) = min {
                rules.min_coverage = m / 100.0;
            }
            let downloads = r.config().downloads.clone();
            let siblings = !chart.parent().is_some_and(|p| p.ends_with(&downloads));
            let (located, candidates) = placement::locate(
                &*r.fs,
                &chart,
                &crate::library::Library::new(&library),
                siblings,
                rules,
            )
            .ok_or_else(|| format!("{}: not a chart", chart.display()))?;
            r.out
                .record(located_record(&chart, &located), || format!("{located:?}"));
            for (c, label) in candidates.iter().take(10) {
                r.out.record(
                    json!({"folder": c.dir, "label": label, "coverage": c.coverage, "own": c.own, "same_song": c.same_song}),
                    || format!("  {:>5.1}% {} {label}", c.coverage * 100.0, if c.same_song { "same song " } else { "          " }),
                );
            }
            Ok(())
        }
        Library::Pending { library } => {
            let waiting = library.join(&r.config().downloads);
            let charts: Vec<PathBuf> =
                r.fs.read_dir(&waiting)
                    .unwrap_or_default()
                    .into_iter()
                    .filter(|e| !e.is_dir && placement::is_chart(&e.path))
                    .map(|e| e.path)
                    .collect();
            // the library read once for all of them
            let index = crate::library::Library::new(&library);
            for c in &charts {
                let (located, best) = placement::locate(&*r.fs, c, &index, false, Rules::default())
                    .unwrap_or((Located::NoSong, Vec::new()));
                let mut v = located_record(c, &located);
                if let Some((b, l)) = best.first() {
                    v["best"] = json!({"folder": b.dir, "label": l, "coverage": b.coverage});
                }
                r.out.record(v, || {
                    let best = best
                        .first()
                        .map(|(c, l)| format!("  best: {:.1}% {l}", c.coverage * 100.0))
                        .unwrap_or_default();
                    format!("{}  {located:?}{best}", c.display())
                });
            }
            r.out.note(format!("{} charts waiting", charts.len()));
            Ok(())
        }
        Library::Install {
            links,
            library,
            diff,
            song,
            own_links,
            again,
        } => {
            let r = &reinstalling(r, again.reinstall);
            let sources = if own_links {
                Sources::default()
            } else {
                Sources::from_config(r.config(), Vec::new())
            };
            if links.is_empty() && sources.names().is_empty() {
                return Err("no link".into());
            }
            let q = Query {
                md5: song.md5.clone(),
                want: song.want(),
                diff,
                ..Default::default()
            };
            let downloads = library.join(&r.config().downloads);
            let lib = crate::library::Library::new(&library);
            let job = install::job::Install::new(r.fs.clone(), downloads, lib, q, links, &r.cx);
            let done = install::job::run_job(&mut job.sources(sources), r.net).done;
            for d in &done {
                r.out
                    .record(done_record(&song.title, d), || done_text(&song.title, d));
            }
            if install::got(&done) {
                Ok(())
            } else {
                Err(format!("{}: not installed", song.title))
            }
        }
    }
}

fn located_record(chart: &Path, l: &Located) -> Value {
    match l {
        Located::Here => json!({"chart": chart, "state": "here"}),
        Located::NoSong => json!({"chart": chart, "state": "no-song"}),
        Located::Move(d) => json!({"chart": chart, "state": "move", "folder": d}),
        Located::Ask(c) => json!({"chart": chart, "state": "ask",
            "candidates": c.iter().map(|(d, l)| json!({"folder": d, "label": l})).collect::<Vec<_>>()}),
    }
}

/// What came of a song's download, as a record.
pub(super) fn done_record(title: &str, d: &Done) -> Value {
    match d {
        Done::Added(dir, n) => json!({"title": title, "state": "added", "path": dir, "files": n}),
        Done::Owned(dir) => json!({"title": title, "state": "owned", "path": dir}),
        Done::Placed(p) => json!({"title": title, "state": "placed", "path": p}),
        Done::Waiting(p) => json!({"title": title, "state": "waiting", "path": p}),
        Done::Page(u) => json!({"title": title, "state": "page", "link": u}),
        Done::Failed(e) => {
            json!({"title": title, "state": "failed", "code": e.code(), "error": e.to_string()})
        }
    }
}

pub(super) fn done_text(title: &str, d: &Done) -> String {
    match d {
        Done::Added(dir, n) => format!("{title}: added {} ({n} files)", dir.display()),
        Done::Owned(dir) => format!("{title}: in the library already: {}", dir.display()),
        Done::Placed(p) => format!("{title}: {}", p.display()),
        Done::Waiting(p) => format!("{title}: {} waits for its song", p.display()),
        Done::Page(u) => format!("{title}: a page, download it in a browser: {u}"),
        Done::Failed(e) => format!("{title}: failed: {e}"),
    }
}

/// The library below `dir`, every chart read (their MD5 and SHA-256), but
/// those the last scan read as they are now (`downloads/scan.json`); not
/// those waiting in its `downloads` folder.
pub(super) fn scan(r: &Run, dir: &Path) -> crate::library::Library {
    let downloads = dir.join(&r.config().downloads);
    let cache = downloads.join("scan.json");
    crate::library::Library::scan_cached(&*r.fs, dir, &downloads, &cache)
}
