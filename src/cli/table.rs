//! `table show`, `table missing`, `table export`.

use super::library::{done_record, scan};
use super::link::fetch_list;
use super::Run;
use crate::chart::md5_key;
use crate::export;
use crate::install::{self, Batch, Entry, EntryJob};
use crate::links::Links;
use crate::net::job::run_job;
use crate::sources::Sources;
use crate::table::{DifficultyTable, TableEntry, TableLoader};
use clap::{Args, Subcommand, ValueEnum};
use serde_json::json;
use std::path::{Path, PathBuf};

#[derive(Subcommand)]
pub(super) enum Table {
    /// A table's name and its levels, with how many charts each.
    Show { url: String },
    /// The table's charts a library lacks (-l, --md5: of a level, of
    /// those charts only); --get downloads them.
    Missing {
        url: String,
        #[arg(short = 'L', long)]
        library: PathBuf,
        /// One level only.
        #[arg(short, long)]
        level: Option<String>,
        /// These charts only (their MD5; more than once for more).
        #[arg(long = "md5", value_name = "MD5")]
        md5s: Vec<String>,
        /// Download them (each song, then its separate chart; with the song
        /// in the library, the chart first; each file once).
        #[arg(long)]
        get: bool,
        /// A download whose charts the library has: its files again, newer
        /// ones over those there (else only those its folder lacks).
        #[arg(long, requires = "get")]
        reinstall: bool,
        #[command(flatten)]
        links: LinkArgs,
    },
    /// The table for LR2 (custom folders) or beatoraja (a table cache).
    Export {
        #[arg(value_enum)]
        to: ExportTo,
        url: String,
        /// LR2: a folder to add to LR2; beatoraja: its table folder.
        out: PathBuf,
        #[command(flatten)]
        links: LinkArgs,
    },
}

#[derive(Clone, Copy, ValueEnum)]
pub(super) enum ExportTo {
    Lr2,
    Bmt,
}

/// Where else charts' links are looked for, by MD5.
#[derive(Args, Clone, Default)]
pub(super) struct LinkArgs {
    /// Other tables whose links count.
    #[arg(long = "also", value_name = "URL")]
    also: Vec<String>,
    /// TSV lists of links (md5, url_diff, url; a file or URL).
    #[arg(long = "list", value_name = "FILE|URL")]
    lists: Vec<String>,
    /// The table's own links only.
    #[arg(long)]
    own_links: bool,
}

pub(super) fn table(r: &Run, c: Table) -> Result<(), String> {
    match c {
        Table::Show { url } => {
            let t = load(r, &url)?;
            r.out.record(
                json!({"table": t.header.name, "symbol": t.header.symbol, "charts": t.entries.len()}),
                || format!("{} [{}]: {} charts", t.header.name, t.header.symbol, t.entries.len()),
            );
            for l in t.levels() {
                let n = t.entries.iter().filter(|e| e.level == l).count();
                r.out.record(
                    json!({"level": l, "label": t.label(&l), "charts": n}),
                    || format!("  {}  {n}", t.label(&l)),
                );
            }
            Ok(())
        }
        Table::Missing {
            url,
            library,
            level,
            md5s,
            get,
            reinstall,
            links,
        } => missing(
            &super::library::reinstalling(r, reinstall),
            &url,
            &library,
            &Chosen { level, md5s },
            get,
            &links,
        ),
        Table::Export {
            to,
            url,
            out,
            links,
        } => {
            let (t, _) = load_filled(r, &url, &links)?;
            match to {
                ExportTo::Lr2 => {
                    let dir = out.join(install::safe_name(&t.header.name));
                    let folders = export::write_lr2(&*r.fs, &t, &dir)?;
                    r.out
                        .record(json!({"folder": dir, "folders": folders}), || {
                            format!("{}: {folders} folders", dir.display())
                        });
                }
                ExportTo::Bmt => {
                    let file = out.join(export::bmt_file_name(&t.source_url));
                    export::write(&*r.fs, &file, &export::beatoraja_bmt(&t))?;
                    r.out
                        .record(json!({"file": file}), || file.display().to_string());
                }
            }
            Ok(())
        }
    }
}

/// The entries of a table a command keeps: of one level, of these charts.
struct Chosen {
    level: Option<String>,
    md5s: Vec<String>,
}

impl Chosen {
    fn keeps(&self, e: &TableEntry) -> bool {
        self.level.as_ref().is_none_or(|l| e.level == *l)
            && (self.md5s.is_empty() || {
                let md5 = md5_key(&e.md5);
                md5.is_some() && self.md5s.iter().any(|m| md5_key(m) == md5)
            })
    }
}

fn missing(
    r: &Run,
    url: &str,
    library: &Path,
    chosen: &Chosen,
    get: bool,
    args: &LinkArgs,
) -> Result<(), String> {
    let (t, links) = load_filled(r, url, args)?;
    // the library read once for the run (what comes in added to it)
    let mut have = scan(r, library);
    let listed: Vec<&TableEntry> = t.entries.iter().filter(|e| chosen.keeps(e)).collect();
    let missing: Vec<&TableEntry> = listed
        .iter()
        .copied()
        .filter(|e| !have.owns(&e.md5) && !have.owns(&e.sha256))
        .collect();
    for e in &missing {
        r.out.record(
            json!({"level": e.level, "label": t.label(&e.level), "title": e.title, "artist": e.artist,
                "md5": e.md5, "url": e.url, "url_diff": e.url_diff}),
            || format!("{}\t{} — {}\t{}\t{}", t.label(&e.level), e.title, e.artist, e.url, e.url_diff),
        );
    }
    r.out.note(format!(
        "{} of {} charts missing",
        missing.len(),
        listed.len()
    ));
    if !get {
        return Ok(());
    }
    // the lists' links come with the table's; BMS SEARCH after (unless
    // --own-links)
    let sources = if args.own_links {
        Sources::default()
    } else {
        Sources::from_config(r.config(), Vec::new())
    };
    let downloads = library.join(&r.config().downloads);
    // one batch: each file downloaded once; the charts it wants wait for
    // their songs
    let mut batch = Batch::new(missing.iter().map(|e| e.md5.clone()));
    for e in missing {
        // an earlier one brought it, or its song (the chart first then)
        let entry = Entry::from_table(e, &links);
        let job = EntryJob::new(
            r.fs.clone(),
            downloads.clone(),
            have,
            entry,
            sources.clone(),
            &r.cx,
        );
        let out = run_job(&mut job.batch(batch), r.net);
        (have, batch) = (out.lib, out.batch);
        for d in &out.done {
            r.out.record(done_record(&e.title, &e.md5, d), || {
                super::library::done_text(&e.title, d)
            });
        }
    }
    Ok(())
}

pub(super) fn load(r: &Run, url: &str) -> Result<DifficultyTable, String> {
    r.task(&mut TableLoader::new(url))
        .map_err(|e| e.to_string())
}

/// A table and the other links known for its charts (none with
/// `--own-links`); its missing or dead links filled.
fn load_filled(r: &Run, url: &str, args: &LinkArgs) -> Result<(DifficultyTable, Links), String> {
    let mut t = load(r, url)?;
    if args.own_links {
        return Ok((t, Links::default()));
    }
    let mut links = Links::default();
    for name in &r.config().sources {
        match name.as_str() {
            "table" => {
                links = links.and(Links::from_tables([&t]));
                for u in &args.also {
                    match load(r, u) {
                        Ok(other) => links = links.and(Links::from_tables([&other])),
                        Err(e) => r.out.note(format!("{u}: {e}")),
                    }
                }
            }
            "stella" => {
                if let Some(b) = fetch_list(&r.config().stella_list, r) {
                    links = links.and(Links::from_stella(&b));
                }
            }
            "tsv" => {
                for l in &args.lists {
                    if let Some(b) = fetch_list(l, r) {
                        links = links.and(Links::from_tsv(&String::from_utf8_lossy(&b)));
                    }
                }
            }
            _ => {}
        }
    }
    let n = links.fill(&mut t.entries);
    r.out.note(format!(
        "{n} charts' links filled; links known for {} charts",
        links.len()
    ));
    Ok((t, links))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A level and MD5s (in any case) choose; an entry without an MD5 is
    /// no MD5's.
    #[test]
    fn entries_chosen() {
        let e = |md5: &str, level: &str| TableEntry {
            md5: md5.into(),
            level: level.into(),
            ..Default::default()
        };
        let (a, b) = ("a".repeat(32), "b".repeat(32));
        let entries = [e(&a, "1"), e(&b, "1"), e(&a, "2"), e("", "1")];
        let kept = |c: Chosen| entries.iter().filter(|x| c.keeps(x)).count();
        let chosen = |level: Option<&str>, md5s: &[&str]| Chosen {
            level: level.map(String::from),
            md5s: md5s.iter().map(|m| m.to_string()).collect(),
        };
        assert_eq!(kept(chosen(None, &[])), 4);
        assert_eq!(kept(chosen(Some("1"), &[])), 3);
        assert_eq!(kept(chosen(None, &[&a.to_uppercase()])), 2);
        assert_eq!(kept(chosen(Some("1"), &[&a, &b])), 2);
        assert_eq!(kept(chosen(None, &["", "not an md5"])), 0);
    }
}
