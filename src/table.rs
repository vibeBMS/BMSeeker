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

use crate::net::{Answer, Next, Request, Task};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use url::Url;

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct CourseTrophy {
    pub(crate) name: String,
    pub(crate) missrate: f64,
    pub(crate) scorerate: f64,
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
pub(crate) fn find_bmstable_meta(html: &str) -> Option<String> {
    let b = html.as_bytes();
    let mut from = 0;
    while let Some(i) = b[from..].iter().position(|&c| c == b'<') {
        let start = from + i;
        from = start + 1;
        if !b[start..]
            .get(..5)
            .is_some_and(|t| t.eq_ignore_ascii_case(b"<meta"))
        {
            continue;
        }
        let end = html[start..].find('>').map_or(html.len(), |e| start + e);
        let tag = &html[start..end];
        let ltag = tag.to_ascii_lowercase();
        if ltag.contains("bmstable") {
            if let Some(c) = attr(tag, &ltag, "content") {
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

pub(crate) fn parse_header(v: &Value) -> Result<TableHeader, TableError> {
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

/// A data JSON's entries (those with a hash), read straight from its
/// text: a value of any JSON type where text is expected (a level as a
/// number), anything but an object in the list passed by.
pub(crate) fn parse_data(text: &str) -> Result<Vec<TableEntry>, TableError> {
    let rows: Vec<Row> =
        serde_json::from_str(text).map_err(|e| TableError::Format(format!("data: {e}")))?;
    Ok(rows
        .into_iter()
        .filter_map(|r| r.0)
        .filter(|e| !e.md5.is_empty() || !e.sha256.is_empty())
        .collect())
}

use serde::de::{IgnoredAny, MapAccess, SeqAccess, Visitor};

/// A visitor's methods for the JSON values it takes as `$v` (a string, a
/// number, a boolean as text; null, a list as nothing), the list read
/// through.
macro_rules! scalars {
    ($t:ty, $v:expr) => {
        fn visit_str<E>(self, v: &str) -> Result<$t, E> {
            Ok($v(v.to_string()))
        }
        fn visit_string<E>(self, v: String) -> Result<$t, E> {
            Ok($v(v))
        }
        fn visit_bool<E>(self, v: bool) -> Result<$t, E> {
            Ok($v(v.to_string()))
        }
        fn visit_i64<E>(self, v: i64) -> Result<$t, E> {
            Ok($v(v.to_string()))
        }
        fn visit_u64<E>(self, v: u64) -> Result<$t, E> {
            Ok($v(v.to_string()))
        }
        fn visit_f64<E>(self, v: f64) -> Result<$t, E> {
            let n = serde_json::Number::from_f64(v).map(|n| n.to_string());
            Ok($v(n.unwrap_or_default()))
        }
        fn visit_unit<E>(self) -> Result<$t, E> {
            Ok($v(String::new()))
        }
        fn visit_seq<A: SeqAccess<'de>>(self, mut s: A) -> Result<$t, A::Error> {
            while s.next_element::<IgnoredAny>()?.is_some() {}
            Ok($v(String::new()))
        }
    };
}

/// A JSON value as text ([`s`]: a string, a number, a boolean; else
/// nothing).
struct Text(String);

impl<'de> Deserialize<'de> for Text {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Text, D::Error> {
        struct V;
        impl<'de> Visitor<'de> for V {
            type Value = Text;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("text")
            }
            scalars!(Text, Text);
            fn visit_map<A: MapAccess<'de>>(self, mut m: A) -> Result<Text, A::Error> {
                while m.next_entry::<IgnoredAny, IgnoredAny>()?.is_some() {}
                Ok(Text(String::new()))
            }
        }
        d.deserialize_any(V)
    }
}

/// An entry of a data JSON (None: not an object).
struct Row(Option<TableEntry>);

impl<'de> Deserialize<'de> for Row {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Row, D::Error> {
        struct V;
        impl<'de> Visitor<'de> for V {
            type Value = Row;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("a table entry")
            }
            scalars!(Row, |_| Row(None));
            fn visit_map<A: MapAccess<'de>>(self, mut m: A) -> Result<Row, A::Error> {
                let mut e = TableEntry::default();
                while let Some(k) = m.next_key::<std::borrow::Cow<str>>()? {
                    let field = match &*k {
                        "md5" => &mut e.md5,
                        "sha256" => &mut e.sha256,
                        "level" => &mut e.level,
                        "title" => &mut e.title,
                        "artist" => &mut e.artist,
                        "url" => &mut e.url,
                        "url_diff" => &mut e.url_diff,
                        "comment" => &mut e.comment,
                        // a string or a list each, lower case, each once
                        "org_md5" | "org_md5s" => {
                            for md5 in str_list(&m.next_value::<Value>()?) {
                                let md5 = md5.trim().to_ascii_lowercase();
                                if !md5.is_empty() && !e.org_md5.contains(&md5) {
                                    e.org_md5.push(md5);
                                }
                            }
                            continue;
                        }
                        _ => {
                            m.next_value::<IgnoredAny>()?;
                            continue;
                        }
                    };
                    *field = m.next_value::<Text>()?.0;
                }
                e.md5 = hash(&e.md5, 32);
                e.sha256 = hash(&e.sha256, 64);
                Ok(Row(Some(e)))
            }
        }
        d.deserialize_any(V)
    }
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
pub(crate) fn decode_body(bytes: &[u8]) -> std::borrow::Cow<'_, str> {
    let bytes = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(bytes);
    match std::str::from_utf8(bytes) {
        Ok(s) => s.into(),
        Err(_) => encoding_rs::SHIFT_JIS.decode(bytes).0,
    }
}

/// The largest table page, and header or data JSON, read (as the original
/// BeMusicSeeker reads them).
const PAGE_CAP: usize = 10 << 20;
pub(crate) const JSON_CAP: usize = 100 << 20;

/// The 発狂 / 通常 tables (ribbit.xyz's, kept by darksabun.club).
pub const INSANE: &str = "https://darksabun.club/table/archive/insane1/";
pub(crate) const NORMAL: &str = "https://darksabun.club/table/archive/normal1/";

/// Tables that moved: the old address, read from the new one (ribbit.xyz
/// is gone).
pub const MOVED: &[(&str, &str)] = &[
    ("http://www.ribbit.xyz/bms/tables/insane.html", INSANE),
    ("http://www.ribbit.xyz/bms/tables/normal.html", NORMAL),
];

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
            Err(e) => match MOVED
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
        let body = body.map_err(|e| TableError::Http(e.to_string()))?;
        let cap = match self.stage {
            Stage::Page(_) => PAGE_CAP,
            _ => JSON_CAP,
        };
        if body.len() > cap {
            let mib = body.len() >> 20;
            return Err(TableError::Format(format!("{mib} MiB: too large")));
        }
        let text = decode_body(&body);
        let json = |t: &str| serde_json::from_str::<Value>(t.trim_start_matches('\u{feff}'));
        match std::mem::replace(&mut self.stage, Stage::Page(String::new())) {
            Stage::Page(url) => {
                if text.trim_start().starts_with('{') {
                    self.stage = Stage::Header(url);
                    return self.advance(Ok(body));
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
                let entries = parse_data(text.trim_start_matches('\u{feff}'))?;
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
    impl crate::net::Http for Mock {
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
        let t = crate::net::run(
            &mut TableLoader::new("https://ex.com/t/table.html"),
            &Mock(m),
        )
        .unwrap();
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
        let e = parse_data(
            r#"[{"md5":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","org_md5":"ABC"},{"md5":"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb","org_md5":["x","Y"],"org_md5s":["y","z"]},{"md5":"cccccccccccccccccccccccccccccccc"}]"#,
        )
        .unwrap();
        assert_eq!(e[0].org_md5, ["abc"]);
        assert_eq!(e[1].org_md5, ["x", "y", "z"]);
        assert!(e[2].org_md5.is_empty());
        // hashes as tables write them: spaces, upper case; not a hash: no entry
        let e = parse_data(
            r#"[{"md5":" 0123456789ABCDEF0123456789abcdef\n"},{"md5":"zz"},{"md5":"","sha256":"x"}]"#,
        )
        .unwrap();
        assert_eq!(e.len(), 1);
        assert_eq!(e[0].md5, "0123456789abcdef0123456789abcdef");
        // any JSON where text is expected; what is no entry passed by
        let e = parse_data(
            r#"[1, "x", null, [2], {"md5":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","level":12,"title":true,"artist":{"a":1},"url":[1],"comment":null,"x":{"y":[]},"url_diff":1.5}]"#,
        )
        .unwrap();
        assert_eq!(
            (
                e.len(),
                &*e[0].level,
                &*e[0].title,
                &*e[0].artist,
                &*e[0].url_diff
            ),
            (1, "12", "true", "", "1.5")
        );
        assert!(parse_data("{}").is_err());
        // a table kept before there were any
        let old: TableEntry = serde_json::from_str(r#"{"md5":"a","sha256":"","level":"1","title":"","artist":"","url":"","url_diff":"","comment":""}"#).unwrap();
        assert!(old.org_md5.is_empty());
    }
}
