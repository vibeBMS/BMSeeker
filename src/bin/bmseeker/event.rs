//! `event works`, `event check`: a BMS event's works and how their links
//! fare, as records (JSON Lines) a script keeps however it likes.
//!
//! ```text
//! work   {"num","title","artist","team","kind","size_kb","page",
//!         "links":[{"label","url"}],"main":[url],"extra":[[url]],"review"?, ...}
//! check  {"url","state":"archive|chart|page|gone|error","kind"?,"format"?,
//!         "size","charts","title"?,"error"?,"at",
//!         "num"?,"source"?,   (a link a source found for work "num")
//!         "mirrored"?}        (a dead uploader's, checked through its mirrors)
//! ```

use crate::io::{read_text, records, s};
use crate::Run;
use bmseeker::hosts::Want;
use bmseeker::manbow;
use bmseeker::net::{Client, Http, Request};
use clap::Subcommand;
use serde_json::{json, Map, Value};
use std::path::PathBuf;

#[derive(Subcommand)]
pub enum Event {
    /// An event's works (manbow's venue number), each with its links and
    /// which to take: the main archive, add-ons, mirrors.
    Works {
        venue: u32,
        /// Records (by "num") whose fields replace the guess: choices made
        /// by hand.
        #[arg(long, value_name = "FILE|-")]
        overrides: Option<PathBuf>,
    },
    /// Each work's links downloaded and looked into: whether they still
    /// work and what they hold (the song, an add-on, media).
    Check {
        /// Work records (`event works`'). Default: stdin.
        #[arg(default_value = "-")]
        works: PathBuf,
        /// Earlier checks: those fresh enough are kept, not checked again.
        #[arg(long, value_name = "FILE")]
        known: Option<PathBuf>,
        /// How long a check stays fresh (days; a failure that may pass: one).
        #[arg(long, default_value_t = 30)]
        days: u64,
        /// Stop checking after this long (the rest keep their earlier checks).
        #[arg(long)]
        minutes: Option<u64>,
        /// A work none of whose links gives an archive: the links the
        /// sources know for it (by its title and artist: BMS SEARCH; with
        /// --third-party the community archive too), checked as well.
        #[arg(long)]
        search: bool,
    },
}

pub fn event(r: &Run, c: Event) -> Result<(), String> {
    match c {
        Event::Works { venue, overrides } => works(r, venue, overrides),
        Event::Check {
            works,
            known,
            days,
            minutes,
            search,
        } => check(r, works, known, days, minutes, search),
    }
}

fn works(r: &Run, venue: u32, overrides: Option<PathBuf>) -> Result<(), String> {
    let http = Client::new(r.config());
    let base = &r.config().manbow;
    let page = |u: String| http.send(&Request::Get(u)).map(|b| manbow::decode(&b));
    let works = manbow::works(
        base,
        venue,
        &page(manbow::url_list_url(base, venue))?,
        &page(manbow::work_list_url(base, venue))?,
    );
    let by_hand = match overrides {
        Some(p) => records(&read_text(&p)?, "num")?,
        None => Vec::new(),
    };
    let (mut review, mut disq) = (0, 0);
    for w in &works {
        let mut o = Map::new();
        o.insert("num".into(), json!(w.num));
        o.insert("title".into(), json!(w.title));
        o.insert("artist".into(), json!(w.artist));
        o.insert("team".into(), json!(w.team));
        o.insert("kind".into(), json!(w.kind));
        o.insert("size_kb".into(), json!(w.size_kb));
        o.insert("page".into(), json!(w.page));
        let links: Vec<Value> = w
            .links
            .iter()
            .map(|(l, u)| json!({"label": l, "url": u}))
            .collect();
        o.insert("links".into(), Value::Array(links));
        o.insert("main".into(), json!(w.pick.main));
        o.insert("extra".into(), json!(w.pick.extra));
        if !w.pick.review.is_empty() {
            o.insert("review".into(), json!(w.pick.review));
        }
        if let Some(h) = by_hand
            .iter()
            .find(|h| h.get("num").and_then(|n| n.as_u64()) == Some(w.num.into()))
        {
            for (k, v) in h {
                o.insert(k.clone(), v.clone());
            }
            o.shift_remove("review");
        }
        review += o.contains_key("review") as usize;
        disq += (w.kind == "Disq") as usize;
        r.out.json(&Value::Object(o));
    }
    r.out.note(format!(
        "{} works, {review} to review, {disq} disqualified",
        works.len()
    ));
    Ok(())
}

fn check(
    r: &Run,
    works: PathBuf,
    known: Option<PathBuf>,
    days: u64,
    minutes: Option<u64>,
    search: bool,
) -> Result<(), String> {
    let works = records(&read_text(&works)?, "num")?;
    let known = match known {
        Some(p) if p.exists() => records(&read_text(&p)?, "url")?,
        _ => Vec::new(),
    };
    let sources = bmseeker::sources::Sources::from_config(r.config(), Vec::new());
    let http = Client::new(r.config());
    let mut c = Checks {
        r,
        known: &known,
        days,
        minutes,
        start: now(),
        tmp: std::env::temp_dir().join(format!("bmseeker-check-{}", std::process::id())),
        seen: Vec::new(),
        checked: 0,
        kept: 0,
    };
    for w in &works {
        let want = Want {
            title: s(w, "title"),
            artist: s(w, "artist"),
        };
        let links = w
            .get("links")
            .and_then(|l| l.as_array())
            .cloned()
            .unwrap_or_default();
        let mut got = false;
        for l in links {
            let url = l
                .as_str()
                .or_else(|| l.get("url").and_then(|u| u.as_str()))
                .unwrap_or("")
                .trim()
                .to_string();
            got |= c.visit(&url, &want, None) == "archive";
        }
        let num = w.get("num").and_then(|n| n.as_u64()).unwrap_or(0);
        if got || !search || c.time_up() {
            continue;
        }
        // what the sources know for it: those found before, then theirs
        let mut found: Vec<(String, String)> = known
            .iter()
            .filter(|k| k.get("num").and_then(|n| n.as_u64()) == Some(num))
            .map(|k| (s(k, "url"), s(k, "source")))
            .collect();
        let q = bmseeker::sources::Query {
            md5: String::new(),
            want: want.clone(),
            diff: false,
        };
        for (source, url) in bmseeker::net::run(&mut sources.find_all(&q), &http) {
            if !found.iter().any(|(u, _)| *u == url) {
                found.push((url, source.to_string()));
            }
        }
        for (url, source) in found {
            if c.visit(&url, &want, Some((num, &source))) == "archive" {
                break;
            }
        }
    }
    // links found for works before, not looked at this time: kept
    for k in &known {
        if k.contains_key("source") && !c.seen.contains(&s(k, "url")) {
            c.kept += 1;
            r.out.json(&Value::Object(k.clone()));
        }
    }
    let _ = std::fs::remove_dir_all(&c.tmp);
    r.out.note(format!(
        "{} checked, {} kept{}",
        c.checked,
        c.kept,
        if c.time_up() {
            "; time is up, run again to go on"
        } else {
            ""
        }
    ));
    Ok(())
}

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// One check run: the links looked at, what was known of them.
struct Checks<'a> {
    r: &'a Run,
    known: &'a [Map<String, Value>],
    days: u64,
    minutes: Option<u64>,
    start: u64,
    tmp: PathBuf,
    seen: Vec<String>,
    checked: usize,
    kept: usize,
}

impl Checks<'_> {
    /// A link that failed, to a dead uploader with mirrors now (with
    /// --third-party), not checked through them yet: checked again.
    fn mirrored(&self, k: &Map<String, Value>) -> bool {
        let config = self.r.config();
        config.third_party
            && !k.contains_key("mirrored")
            && !matches!(s(k, "state").as_str(), "archive" | "chart")
            && config
                .uploaders
                .iter()
                .any(|u| u.number(&s(k, "url")).is_some())
    }

    fn time_up(&self) -> bool {
        self.minutes.is_some_and(|m| now() - self.start > m * 60)
    }

    /// Check `url` (once a run; its earlier check when fresh enough, or
    /// when time is up), writing its record (`found`: a source's link for
    /// work `.0`). Its state, "" when not known.
    fn visit(&mut self, url: &str, want: &Want, found: Option<(u64, &str)>) -> String {
        if !url.starts_with("http") || self.seen.iter().any(|u| u == url) {
            return String::new();
        }
        self.seen.push(url.to_string());
        let before = self.known.iter().find(|k| s(k, "url") == url);
        let fresh = before.is_some_and(|k| {
            let at = k.get("at").and_then(|a| a.as_u64()).unwrap_or(0);
            let keep = if s(k, "state") == "error" {
                1
            } else {
                self.days
            };
            now().saturating_sub(at) < keep * 86_400 && !self.mirrored(k)
        });
        let time_up = self.time_up();
        if fresh || (time_up && before.is_some()) {
            let mut k = before.cloned().unwrap_or_default();
            if let Some((num, source)) = found {
                k.insert("num".into(), json!(num));
                k.insert("source".into(), json!(source));
            }
            self.kept += 1;
            self.r.out.json(&Value::Object(k.clone()));
            return s(&k, "state");
        }
        if time_up {
            return String::new();
        }
        let c = bmseeker::probe::probe(url, Some(want.clone()), &self.tmp, &self.r.cx);
        self.r.out.note(format!(
            "{}{} | {:?} {:?} {} MB {} charts {} | {url}",
            want.title,
            found.map(|(_, s)| format!(" ({s})")).unwrap_or_default(),
            c.state,
            c.kind,
            c.size >> 20,
            c.charts,
            c.error
        ));
        let mut rec = Map::new();
        rec.insert("url".into(), json!(url));
        if let Ok(Value::Object(o)) = serde_json::to_value(&c) {
            rec.extend(o);
        }
        rec.insert("at".into(), json!(now()));
        let config = self.r.config();
        if config.third_party && config.uploaders.iter().any(|u| u.number(url).is_some()) {
            rec.insert("mirrored".into(), json!(true));
        }
        if let Some((num, source)) = found {
            rec.insert("num".into(), json!(num));
            rec.insert("source".into(), json!(source));
        }
        self.checked += 1;
        let state = s(&rec, "state");
        self.r.out.json(&Value::Object(rec));
        state
    }
}
