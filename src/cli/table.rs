//! `table show`, `table missing`, `table export`.

use super::library::{done_record, scan};
use super::link::fetch_list;
use super::Run;
use crate::export;
use crate::install::{self, Batch, Done, Entry, EntryJob, Failure};
use crate::links::Links;
use crate::net::job::run_job;
use crate::sources::Sources;
use crate::table::{DifficultyTable, TableEntry, TableLoader};
use clap::{Args, Subcommand, ValueEnum};
use serde_json::json;
use std::collections::{BTreeSet, HashSet};
use std::path::{Path, PathBuf};

#[derive(Subcommand)]
pub(super) enum Table {
    /// A table's name and its levels, with how many charts each.
    Show { url: String },
    /// The table's charts a library lacks (-l, --md5, --sha256, --chosen:
    /// of a level, of those charts only); --get downloads them.
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
        /// These charts only (their SHA-256, for a table that gives no MD5;
        /// more than once for more).
        #[arg(long = "sha256", value_name = "SHA256")]
        sha256s: Vec<String>,
        /// These charts only, as --md5 and --sha256: their hashes one a line
        /// (an MD5 or a SHA-256), in a file or `-` (stdin; a list too long
        /// for a command line).
        #[arg(long, value_name = "FILE|-")]
        chosen: Option<PathBuf>,
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
            sha256s,
            chosen,
            get,
            reinstall,
            links,
        } => {
            let mut c = Chosen::new(level, md5s, sha256s);
            if let Some(file) = chosen {
                c.read(&r.read_text(&file)?)
                    .map_err(|e| format!("--chosen {}: {e}", file.display()))?;
            }
            missing(
                &super::library::reinstalling(r, reinstall),
                &url,
                &library,
                &c,
                get,
                &links,
            )
        }
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

/// The entries of a table a command keeps: of one level, of these charts
/// (named by either hash: an entry is kept when one given is its own; none
/// given, every chart).
struct Chosen {
    level: Option<String>,
    /// Whether charts are named (an empty name too names one: none).
    named: bool,
    md5s: BTreeSet<String>,
    sha256s: BTreeSet<String>,
}

impl Chosen {
    fn new(level: Option<String>, md5s: Vec<String>, sha256s: Vec<String>) -> Chosen {
        let set = |v: Vec<String>| v.iter().map(|h| h.trim().to_ascii_lowercase()).collect();
        Chosen {
            level,
            named: !md5s.is_empty() || !sha256s.is_empty(),
            md5s: set(md5s),
            sha256s: set(sha256s),
        }
    }

    /// Also the charts of `text`'s hashes, one a line (32 hex digits an
    /// MD5, 64 a SHA-256; blank lines aside); anything else is an error,
    /// by its line.
    fn read(&mut self, text: &str) -> Result<(), String> {
        for (i, l) in text.lines().enumerate() {
            let h = l.trim().to_ascii_lowercase();
            let set = match h.len() {
                _ if !h.bytes().all(|b| b.is_ascii_hexdigit()) => None,
                0 => continue,
                32 => Some(&mut self.md5s),
                64 => Some(&mut self.sha256s),
                _ => None,
            };
            set.ok_or_else(|| format!("line {}: not an MD5 or a SHA-256", i + 1))?
                .insert(h);
        }
        self.named = true;
        Ok(())
    }

    fn keeps(&self, e: &TableEntry) -> bool {
        let named = |hash: &str, given: &BTreeSet<String>| !hash.is_empty() && given.contains(hash);
        self.level.as_ref().is_none_or(|l| e.level == *l)
            && (!self.named || named(&e.md5, &self.md5s) || named(&e.sha256, &self.sha256s))
    }

    /// The hashes given that none of `entries` has.
    fn unlisted(&self, entries: &[&TableEntry]) -> Vec<&str> {
        let md5s: HashSet<&str> = entries.iter().map(|e| e.md5.as_str()).collect();
        let sha256s: HashSet<&str> = entries.iter().map(|e| e.sha256.as_str()).collect();
        let md5 = self.md5s.iter().filter(|h| !md5s.contains(h.as_str()));
        let sha = self
            .sha256s
            .iter()
            .filter(|h| !sha256s.contains(h.as_str()));
        md5.chain(sha)
            .filter(|h| !h.is_empty())
            .map(String::as_str)
            .collect()
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
    let owned =
        |lib: &crate::library::Library, e: &TableEntry| lib.owns(&e.md5) || lib.owns(&e.sha256);
    let missing: Vec<&TableEntry> = listed
        .iter()
        .copied()
        .filter(|e| !owned(&have, e))
        .collect();
    for e in &missing {
        r.out.record(
            json!({"level": e.level, "label": t.label(&e.level), "title": e.title, "artist": e.artist,
                "md5": e.md5, "sha256": e.sha256, "url": e.url, "url_diff": e.url_diff}),
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
    // each chart named gets a last record saying whether the library has
    // it now (`have`): one the library had, one the table does not list
    if chosen.named {
        for e in listed.iter().filter(|e| owned(&have, e)) {
            closing(r, e, &in_the_library(&have, e), true);
        }
        for h in chosen.unlisted(&listed) {
            let hash = if h.len() == 32 { "md5" } else { "sha256" };
            let v = json!({hash: h, "state": "failed", "code": "not_listed", "error": NOT_LISTED,
                "have": have.owns(h)});
            r.out.record(v, || format!("{h}: {NOT_LISTED}"));
        }
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
        // (what read the records is gone)
        if r.cx.cancelled() {
            return Err(crate::net::NetError::Cancelled.to_string());
        }
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
        let got = owned(&have, e);
        let Some((last, before)) = out.done.split_last() else {
            // nothing to do: an earlier one brought it; or nothing to try
            let d = match got {
                true => in_the_library(&have, e),
                false => Done::Failed(Failure::NoLink),
            };
            closing(r, e, &d, got);
            continue;
        };
        for d in before {
            r.out
                .record(done_record(&e.title, &e.md5, &e.sha256, d), || {
                    super::library::done_text(&e.title, d)
                });
        }
        closing(r, e, last, got);
    }
    Ok(())
}

/// What a chart named that the table does not list comes to.
const NOT_LISTED: &str = "the table lists no such chart";

/// `e`'s last record: `d`, and whether the library has its chart now.
fn closing(r: &Run, e: &TableEntry, d: &Done, have: bool) {
    let mut v = done_record(&e.title, &e.md5, &e.sha256, d);
    v["have"] = have.into();
    r.out.record(v, || super::library::done_text(&e.title, d));
}

/// `e`'s chart, which `lib` has: in its song's folder.
fn in_the_library(lib: &crate::library::Library, e: &TableEntry) -> Done {
    let mut at = lib
        .folders_of(&e.md5)
        .iter()
        .chain(lib.folders_of(&e.sha256));
    Done::Owned(at.next().cloned().unwrap_or_default())
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

    /// A level and hashes (in any case) choose; an entry without an MD5
    /// is no MD5's.
    #[test]
    fn entries_chosen() {
        let e = |md5: &str, level: &str| TableEntry {
            md5: md5.into(),
            level: level.into(),
            ..Default::default()
        };
        let (a, b) = ("a".repeat(32), "b".repeat(32));
        let mut by_sha = e("", "1");
        by_sha.sha256 = "d".repeat(64);
        let entries = [e(&a, "1"), e(&b, "1"), e(&a, "2"), by_sha];
        let kept = |c: Chosen| entries.iter().filter(|x| c.keeps(x)).count();
        let list = |v: &[&str]| v.iter().map(|m| m.to_string()).collect();
        let chosen = |level: Option<&str>, md5s: &[&str], sha256s: &[&str]| {
            Chosen::new(level.map(String::from), list(md5s), list(sha256s))
        };
        let d = "D".repeat(64);
        assert_eq!(kept(chosen(None, &[], &[])), 4);
        assert_eq!(kept(chosen(Some("1"), &[], &[])), 3);
        assert_eq!(kept(chosen(None, &[&a.to_uppercase()], &[])), 2);
        assert_eq!(kept(chosen(Some("1"), &[&a, &b], &[])), 2);
        // by its SHA-256 (in any case), beside MD5s
        assert_eq!(kept(chosen(None, &[], &[&d])), 1);
        assert_eq!(kept(chosen(Some("1"), &[&b], &[&format!(" {d}")])), 2);
        // an empty hash names nothing (the entry's MD5 is empty)
        assert_eq!(kept(chosen(None, &["", "not an md5"], &[""])), 0);
        // the same from a list (stdin's: `--chosen -`), one a line
        let mut c = chosen(Some("1"), &[], &[]);
        c.read(&format!("{}\r\n\n  {d}\n", b.to_uppercase()))
            .unwrap();
        assert_eq!(kept(c), 2);
        let mut c = chosen(None, &[], &[]);
        c.read("").unwrap();
        assert_eq!(kept(c), 0, "an empty list names no chart");
        // what is no hash: an error, by its line
        for (bad, line) in [
            ("xyz", 1),
            (&*format!("{a}\n{}", "e".repeat(40)), 2),
            (&*format!("{a}\n\n{}", "g".repeat(32)), 3),
        ] {
            let e = chosen(None, &[], &[]).read(bad).unwrap_err();
            assert_eq!(e, format!("line {line}: not an MD5 or a SHA-256"));
        }
        // the hashes no entry has
        let c = Chosen::new(
            None,
            list(&[&a, &"c".repeat(32)]),
            list(&[&d, &"e".repeat(64)]),
        );
        let all: Vec<&TableEntry> = entries.iter().collect();
        assert_eq!(c.unlisted(&all), ["c".repeat(32), "e".repeat(64)]);
    }
}
