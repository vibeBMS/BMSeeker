//! `library import`, `library place`, `library pending`, `library install`.

use crate::{Run, SongArgs};
use bmseeker::fs::Disk;
use bmseeker::import::Imported;
use bmseeker::install::{self, Done};
use bmseeker::placement::{self, Located, Rules};
use bmseeker::sources::{Query, Sources};
use clap::Subcommand;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

#[derive(Subcommand)]
pub enum Library {
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
pub struct Reinstall {
    /// Bring its files in again, newer ones over those there (else only
    /// those its folder lacks).
    #[arg(long)]
    reinstall: bool,
}

/// `r`, songs whose charts the library has brought in again with
/// `reinstall` (else as the settings say).
pub fn reinstalling(r: &Run, reinstall: bool) -> Run {
    let mut cx = r.cx.clone();
    if reinstall {
        let config = bmseeker::config::Config {
            reinstall: true,
            ..(*cx.config).clone()
        };
        cx.config = std::sync::Arc::new(config);
    }
    Run {
        out: r.out.clone(),
        cx,
    }
}

pub fn library(r: &Run, c: Library) -> Result<(), String> {
    match c {
        Library::Import {
            file,
            library,
            again,
        } => {
            let req = bmseeker::install::Request {
                reinstall: again.reinstall || r.config().reinstall,
                naming: install::Naming::of(r.config(), Default::default()),
                ..Default::default()
            };
            let lib = &mut scan(&library, &r.config().downloads);
            let got = bmseeker::import::import(&Disk, &file, lib, &req)?;
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
                other => (
                    json!({"state": "other", "detail": format!("{other:?}")}),
                    format!("{other:?}"),
                ),
            };
            r.out.record(v, || text);
            Ok(())
        }
        Library::Place {
            chart,
            library,
            to,
            min,
        } => {
            if let Some(dir) = to {
                return match placement::bring(&Disk, &chart, &dir) {
                    Some(at) => {
                        r.out.record(json!({"state": "placed", "path": at}), || {
                            at.display().to_string()
                        });
                        Ok(())
                    }
                    None => Err(format!("{} stays where it is", chart.display())),
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
                &Disk,
                &chart,
                &bmseeker::library::Library::new(&library),
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
            let charts: Vec<PathBuf> = std::fs::read_dir(&waiting)
                .into_iter()
                .flatten()
                .flatten()
                .map(|e| e.path())
                .filter(|p| placement::is_chart(p))
                .collect();
            // the library read once for all of them
            let index = bmseeker::library::Library::new(&library);
            for c in &charts {
                let (located, best) = placement::locate(&Disk, c, &index, false, Rules::default())
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
            let lib = &mut bmseeker::library::Library::new(&library);
            let batch = &mut Default::default();
            let done = install::run(&downloads, lib, &q, &links, &sources, batch, &r.cx);
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
pub fn done_record(title: &str, d: &Done) -> Value {
    match d {
        Done::Added(dir, n) => json!({"title": title, "state": "added", "path": dir, "files": n}),
        Done::Owned(dir) => json!({"title": title, "state": "owned", "path": dir}),
        Done::Placed(p) => json!({"title": title, "state": "placed", "path": p}),
        Done::Waiting(p) => json!({"title": title, "state": "waiting", "path": p}),
        Done::Page(u) => json!({"title": title, "state": "page", "link": u}),
        Done::Failed(e) => json!({"title": title, "state": "failed", "error": e.to_string()}),
        other => json!({"title": title, "state": "other", "detail": format!("{other:?}")}),
    }
}

pub fn done_text(title: &str, d: &Done) -> String {
    match d {
        Done::Added(dir, n) => format!("{title}: added {} ({n} files)", dir.display()),
        Done::Owned(dir) => format!("{title}: in the library already: {}", dir.display()),
        Done::Placed(p) => format!("{title}: {}", p.display()),
        Done::Waiting(p) => format!("{title}: {} waits for its song", p.display()),
        Done::Page(u) => format!("{title}: a page, download it in a browser: {u}"),
        Done::Failed(e) => format!("{title}: failed: {e}"),
        other => format!("{title}: {other:?}"),
    }
}

/// The library below `dir`, every chart read (their MD5 and SHA-256);
/// not those waiting in its `downloads` folder.
pub fn scan(dir: &Path, downloads: &str) -> bmseeker::library::Library {
    bmseeker::library::Library::scan_except(&Disk, dir, &dir.join(downloads))
}
