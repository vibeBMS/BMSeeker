//! Difficulty tables in the common "bmstable" format.
//!
//! This is the format used by the LR2-era tables (通常/発狂BMS難易度表,
//! overjoy, LN tables, ...) and by beatoraja-era tables such as Satellite
//! and Stella:
//!
//! 1. an HTML page containing `<meta name="bmstable" content="header.json">`
//!    (or the header JSON URL directly),
//! 2. a header JSON with `name`, `symbol`, `data_url`, optional
//!    `level_order` and `course`,
//! 3. a data JSON: an array of entries with `md5` / `sha256` and `level`.

use crate::net::{Answer, Http, Next, Request, Task};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use url::Url;

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct CourseTrophy {
    pub name: String,
    pub missrate: f64,
    pub scorerate: f64,
}

/// A course (段位 / dan) defined by a table.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct Course {
    pub name: String,
    pub md5: Vec<String>,
    pub sha256: Vec<String>,
    /// e.g. "grade_mirror", "gauge_lr2", "ln", "cn", "hcn", "no_speed"...
    pub constraint: Vec<String>,
    pub trophy: Vec<CourseTrophy>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct TableHeader {
    pub name: String,
    pub symbol: String,
    pub tag: String,
    pub data_url: String,
    pub level_order: Vec<String>,
    pub courses: Vec<Course>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct TableEntry {
    pub md5: String,
    pub sha256: String,
    pub level: String,
    pub title: String,
    pub artist: String,
    pub url: String,
    pub url_diff: String,
    pub comment: String,
    /// The MD5s of the song's own charts (`org_md5`, `org_md5s`): for a
    /// separate chart, the song it belongs to.
    #[serde(default)]
    pub org_md5: Vec<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct DifficultyTable {
    /// URL the user registered.
    pub source_url: String,
    pub header: TableHeader,
    pub entries: Vec<TableEntry>,
    /// Unix seconds of the last successful fetch.
    pub fetched_at: u64,
}

#[derive(Debug)]
pub enum TableError {
    Http(String),
    Format(String),
}

impl std::fmt::Display for TableError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TableError::Http(e) => write!(f, "network error: {e}"),
            TableError::Format(e) => write!(f, "invalid table: {e}"),
        }
    }
}
impl std::error::Error for TableError {}

/// Well-known tables offered by default.
pub const DEFAULT_TABLES: &[(&str, &str)] = &[
    // ribbit.xyz is gone; darksabun.club keeps their copies
    ("発狂BMS難易度表 (★)", INSANE),
    ("通常難易度表 (☆)", NORMAL),
    ("Satellite (sl)", "https://stellabms.xyz/sl/table.html"),
    ("Stella (st)", "https://stellabms.xyz/st/table.html"),
];

fn s(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        Value::Bool(b) => b.to_string(),
        _ => String::new(),
    }
}

fn str_list(v: &Value) -> Vec<String> {
    match v {
        Value::Array(a) => a.iter().map(s).filter(|x| !x.is_empty()).collect(),
        Value::Null => Vec::new(),
        other => vec![s(other)],
    }
}

/// Find `<meta name="bmstable" content="...">` in an HTML page.
pub fn find_bmstable_meta(html: &str) -> Option<String> {
    let lower = html.to_ascii_lowercase();
    let mut from = 0;
    while let Some(i) = lower[from..].find("<meta") {
        let start = from + i;
        let end = lower[start..]
            .find('>')
            .map(|e| start + e)
            .unwrap_or(lower.len());
        let tag = &html[start..end];
        let ltag = &lower[start..end];
        if ltag.contains("bmstable") {
            if let Some(c) = attr(tag, ltag, "content") {
                return Some(c);
            }
        }
        from = end;
    }
    None
}

fn attr(tag: &str, ltag: &str, name: &str) -> Option<String> {
    // `ltag` is the ASCII-lowercased `tag`, so byte offsets are shared.
    let b = ltag.as_bytes();
    let mut idx = 0;
    while let Some(i) = ltag[idx..].find(name) {
        let mut p = idx + i + name.len();
        while p < b.len() && b[p].is_ascii_whitespace() {
            p += 1;
        }
        if p < b.len() && b[p] == b'=' {
            p += 1;
            while p < b.len() && b[p].is_ascii_whitespace() {
                p += 1;
            }
            let q = *b.get(p)?;
            if q == b'"' || q == b'\'' {
                let rest = &tag[p + 1..];
                let e = rest.find(q as char)?;
                return Some(rest[..e].trim().to_string());
            }
            let rest = &tag[p..];
            let e = rest.find(char::is_whitespace).unwrap_or(rest.len());
            return Some(rest[..e].trim_end_matches('/').to_string());
        }
        idx = p;
    }
    None
}

pub fn parse_header(v: &Value) -> Result<TableHeader, TableError> {
    let o = v
        .as_object()
        .ok_or_else(|| TableError::Format("header is not an object".into()))?;
    let get = |k: &str| o.get(k).map(s).unwrap_or_default();
    let mut courses = Vec::new();
    if let Some(c) = o.get("course") {
        let mut flat = Vec::new();
        match c {
            Value::Array(a) => {
                for x in a {
                    match x {
                        Value::Array(inner) => flat.extend(inner.iter().cloned()),
                        other => flat.push(other.clone()),
                    }
                }
            }
            Value::Object(_) => flat.push(c.clone()),
            _ => {}
        }
        for c in flat {
            let Some(co) = c.as_object() else { continue };
            courses.push(Course {
                name: co.get("name").map(s).unwrap_or_default(),
                md5: co.get("md5").map(str_list).unwrap_or_default(),
                sha256: co.get("sha256").map(str_list).unwrap_or_default(),
                constraint: co.get("constraint").map(str_list).unwrap_or_default(),
                trophy: co
                    .get("trophy")
                    .and_then(Value::as_array)
                    .map(|a| {
                        a.iter()
                            .filter_map(Value::as_object)
                            .map(|t| CourseTrophy {
                                name: t.get("name").map(s).unwrap_or_default(),
                                missrate: t.get("missrate").and_then(Value::as_f64).unwrap_or(0.0),
                                scorerate: t
                                    .get("scorerate")
                                    .and_then(Value::as_f64)
                                    .unwrap_or(0.0),
                            })
                            .collect()
                    })
                    .unwrap_or_default(),
            });
        }
    }
    let header = TableHeader {
        name: get("name"),
        symbol: get("symbol"),
        tag: get("tag"),
        data_url: get("data_url"),
        level_order: o.get("level_order").map(str_list).unwrap_or_default(),
        courses,
    };
    if header.data_url.is_empty() {
        return Err(TableError::Format("header has no data_url".into()));
    }
    Ok(header)
}

/// A chart hash of `len` hex digits as tables write it (spaces around it,
/// either case), lower case; empty when it is not one.
fn hash(v: &str, len: usize) -> String {
    let v = v.trim();
    if v.len() == len && v.bytes().all(|b| b.is_ascii_hexdigit()) {
        v.to_ascii_lowercase()
    } else {
        String::new()
    }
}

pub fn parse_data(v: &Value) -> Result<Vec<TableEntry>, TableError> {
    let a = v
        .as_array()
        .ok_or_else(|| TableError::Format("data is not an array".into()))?;
    Ok(a.iter()
        .filter_map(Value::as_object)
        .map(|o| {
            let get = |k: &str| o.get(k).map(s).unwrap_or_default();
            TableEntry {
                md5: hash(&get("md5"), 32),
                sha256: hash(&get("sha256"), 64),
                level: get("level"),
                title: get("title"),
                artist: get("artist"),
                url: get("url"),
                url_diff: get("url_diff"),
                comment: get("comment"),
                org_md5: org_md5(o),
            }
        })
        .filter(|e| !e.md5.is_empty() || !e.sha256.is_empty())
        .collect())
}

/// An entry's `org_md5` and `org_md5s` (a string or a list each), lower
/// case, each once.
fn org_md5(o: &serde_json::Map<String, Value>) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for k in ["org_md5", "org_md5s"] {
        for m in o.get(k).map(str_list).unwrap_or_default() {
            let m = m.trim().to_ascii_lowercase();
            if !m.is_empty() && !out.contains(&m) {
                out.push(m);
            }
        }
    }
    out
}

fn join(base: &str, rel: &str) -> Result<String, TableError> {
    let rel = rel.trim();
    if let Ok(u) = Url::parse(rel) {
        return Ok(u.to_string());
    }
    let b = Url::parse(base).map_err(|e| TableError::Format(format!("bad url {base}: {e}")))?;
    b.join(rel)
        .map(|u| u.to_string())
        .map_err(|e| TableError::Format(e.to_string()))
}

/// Decode a response body (UTF-8 with BOM, or Shift_JIS for old tables).
pub fn decode_body(bytes: &[u8]) -> String {
    let bytes = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(bytes);
    match std::str::from_utf8(bytes) {
        Ok(s) => s.to_owned(),
        Err(_) => encoding_rs::SHIFT_JIS.decode(bytes).0.into_owned(),
    }
}

/// The 発狂 / 通常 tables (ribbit.xyz's, kept by darksabun.club).
pub const INSANE: &str = "https://darksabun.club/table/archive/insane1/";
pub const NORMAL: &str = "https://darksabun.club/table/archive/normal1/";

/// Tables that moved: the old address, read from the new one (ribbit.xyz
/// is gone).
pub const MOVED: &[(&str, &str)] = &[
    ("http://www.ribbit.xyz/bms/tables/insane.html", INSANE),
    ("http://www.ribbit.xyz/bms/tables/normal.html", NORMAL),
];

/// Known mirrors used when the primary host is unreachable.
pub const MIRRORS: &[(&str, &str)] = MOVED;

/// Next action of a [`TableLoader`].
#[derive(Debug)]
enum Step {
    Get(String),
    Done(Box<DifficultyTable>),
}

#[derive(Debug)]
enum Stage {
    Page(String),
    Header(String),
    Data(TableHeader),
}

/// A table loaded (page → header.json → data.json) as a [`Task`], so any
/// HTTP client (native or browser) can run it.
#[derive(Debug)]
pub struct TableLoader {
    source: String,
    stage: Stage,
    mirror_used: bool,
}

impl TableLoader {
    pub fn new(url: &str) -> TableLoader {
        TableLoader {
            source: url.to_string(),
            stage: Stage::Page(url.to_string()),
            mirror_used: false,
        }
    }

    fn step(&mut self, body: Answer) -> Result<Step, TableError> {
        match self.advance(body) {
            Ok(s) => Ok(s),
            Err(e) => match MIRRORS
                .iter()
                .find(|(u, _)| *u == self.source)
                .filter(|_| !self.mirror_used)
            {
                Some((_, mirror)) => {
                    log::info!("{} failed ({e}); trying mirror {mirror}", self.source);
                    self.mirror_used = true;
                    self.stage = Stage::Page(mirror.to_string());
                    Ok(Step::Get(mirror.to_string()))
                }
                None => Err(e),
            },
        }
    }

    fn advance(&mut self, body: Answer) -> Result<Step, TableError> {
        let text = decode_body(&body.map_err(|e| TableError::Http(e.to_string()))?);
        let json = |t: &str| serde_json::from_str::<Value>(t.trim_start_matches('\u{feff}'));
        match std::mem::replace(&mut self.stage, Stage::Page(String::new())) {
            Stage::Page(url) => {
                if text.trim_start().starts_with('{') {
                    self.stage = Stage::Header(url);
                    return self.advance(Ok(text.into_bytes()));
                }
                let meta = find_bmstable_meta(&text).ok_or_else(|| {
                    TableError::Format("no <meta name=\"bmstable\"> found".into())
                })?;
                let hu = join(&url, &meta)?;
                self.stage = Stage::Header(hu.clone());
                Ok(Step::Get(hu))
            }
            Stage::Header(url) => {
                let header = parse_header(
                    &json(&text).map_err(|e| TableError::Format(format!("header: {e}")))?,
                )?;
                let du = join(&url, &header.data_url)?;
                self.stage = Stage::Data(header);
                Ok(Step::Get(du))
            }
            Stage::Data(header) => {
                let entries = parse_data(
                    &json(&text).map_err(|e| TableError::Format(format!("data: {e}")))?,
                )?;
                let fetched_at = web_time::SystemTime::now()
                    .duration_since(web_time::UNIX_EPOCH)
                    .map(|d| d.as_secs())
                    .unwrap_or(0);
                Ok(Step::Done(Box::new(DifficultyTable {
                    source_url: self.source.clone(),
                    header,
                    entries,
                    fetched_at,
                })))
            }
        }
    }
}

impl Task for TableLoader {
    type Output = Result<DifficultyTable, TableError>;

    fn start(&mut self) -> Next<Self::Output> {
        self.stage = Stage::Page(self.source.clone());
        self.mirror_used = false;
        Next::Ask(Request::Get(self.source.clone()))
    }

    fn feed(&mut self, answer: Answer) -> Next<Self::Output> {
        match self.step(answer) {
            Ok(Step::Get(u)) => Next::Ask(Request::Get(u)),
            Ok(Step::Done(t)) => Next::Done(Ok(*t)),
            Err(e) => Next::Done(Err(e)),
        }
    }
}

/// Load a table, blocking on `http` (tools, tests).
pub fn fetch_table(http: &dyn Http, url: &str) -> Result<DifficultyTable, TableError> {
    crate::net::run(&mut TableLoader::new(url), http)
}

/// Compare levels naturally ("2" < "10", "10" < "10+", numbers before text).
fn level_key(l: &str) -> (u8, f64, String) {
    let num: String = l
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == '.')
        .collect();
    match num.parse::<f64>() {
        Ok(n) => (0, n, l[num.len()..].to_string()),
        Err(_) => (1, 0.0, l.to_string()),
    }
}

impl DifficultyTable {
    /// Levels in display order (`level_order` if given, else natural order).
    pub fn levels(&self) -> Vec<String> {
        let mut seen: Vec<String> = Vec::new();
        for l in &self.header.level_order {
            if !seen.contains(l) {
                seen.push(l.clone());
            }
        }
        let mut rest: Vec<String> = self
            .entries
            .iter()
            .map(|e| e.level.clone())
            .filter(|l| !seen.contains(l))
            .collect();
        rest.sort_by(|a, b| {
            level_key(a)
                .partial_cmp(&level_key(b))
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        rest.dedup();
        seen.extend(rest);
        seen
    }

    /// Display label e.g. "★12".
    pub fn label(&self, level: &str) -> String {
        format!("{}{}", self.header.symbol, level)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    struct Mock(HashMap<String, String>);
    impl Http for Mock {
        fn send(&self, r: &Request) -> Answer {
            let url = r.url();
            self.0
                .get(url)
                .map(|s| s.clone().into_bytes())
                .ok_or(crate::net::NetError::Status(404))
        }
    }

    #[test]
    fn meta_parsing() {
        assert_eq!(
            find_bmstable_meta(
                r#"<html><head><META NAME="bmstable" CONTENT="header.json"></head>"#
            )
            .as_deref(),
            Some("header.json")
        );
        assert_eq!(
            find_bmstable_meta("<meta name='bmstable' content='./h.json' />").as_deref(),
            Some("./h.json")
        );
        assert_eq!(find_bmstable_meta("<meta charset=utf-8>"), None);
    }

    #[test]
    fn full_fetch_with_relative_urls() {
        let mut m = HashMap::new();
        m.insert(
            "https://ex.com/t/table.html".into(),
            r#"<meta name="bmstable" content="header.json">"#.into(),
        );
        m.insert(
            "https://ex.com/t/header.json".into(),
            r#"{"name":"Test","symbol":"sl","data_url":"data/score.json","level_order":["0","1",2],
                "course":[[{"name":"sl1","md5":["a","b"],"constraint":["grade_mirror","gauge_lr2"]}]]}"#
                .into(),
        );
        m.insert(
            "https://ex.com/t/data/score.json".into(),
            r#"[{"md5":"AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA","level":1,"title":"x"},{"sha256":"dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd","level":"10"},{"md5":"11111111111111111111111111111111","level":"2"},{"title":"no hash"}]"#.into(),
        );
        let t = fetch_table(&Mock(m), "https://ex.com/t/table.html").unwrap();
        assert_eq!(t.header.symbol, "sl");
        assert_eq!(t.entries.len(), 3);
        assert_eq!(t.entries[0].md5, "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa");
        assert_eq!(t.levels(), vec!["0", "1", "2", "10"]);
        assert_eq!(t.header.courses.len(), 1);
        assert_eq!(t.header.courses[0].md5.len(), 2);
        assert_eq!(t.label("12"), "sl12");
    }

    /// A separate chart's entry names its song's charts.
    #[test]
    fn the_songs_own_charts() {
        let v: Value = serde_json::from_str(
            r#"[{"md5":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","org_md5":"ABC"},{"md5":"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb","org_md5":["x","Y"],"org_md5s":["y","z"]},{"md5":"cccccccccccccccccccccccccccccccc"}]"#,
        )
        .unwrap();
        let e = parse_data(&v).unwrap();
        assert_eq!(e[0].org_md5, ["abc"]);
        assert_eq!(e[1].org_md5, ["x", "y", "z"]);
        assert!(e[2].org_md5.is_empty());
        // hashes as tables write them: spaces, upper case; not a hash: no entry
        let v: Value = serde_json::from_str(
            r#"[{"md5":" 0123456789ABCDEF0123456789abcdef\n"},{"md5":"zz"},{"md5":"","sha256":"x"}]"#,
        )
        .unwrap();
        let e = parse_data(&v).unwrap();
        assert_eq!(e.len(), 1);
        assert_eq!(e[0].md5, "0123456789abcdef0123456789abcdef");
        // a table kept before there were any
        let old: TableEntry = serde_json::from_str(r#"{"md5":"a","sha256":"","level":"1","title":"","artist":"","url":"","url_diff":"","comment":""}"#).unwrap();
        assert!(old.org_md5.is_empty());
    }
}
