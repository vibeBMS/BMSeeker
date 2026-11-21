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

use crate::net::{Answer, Kind, Next, Request, Task};
use serde::{Deserialize, Serialize};
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

/// A header JSON read straight from its text: a value of any JSON type
/// where text is expected (a symbol as a number), `course` a list of
/// courses, or of lists of them, or one course.
pub(crate) fn parse_header(text: &str) -> Result<TableHeader, TableError> {
    crate::json::last_wins! {
        struct Header {
            name: Text,
            symbol: Text,
            tag: Text,
            data_url: Text,
            level_order: Texts,
            course: Courses,
        }
    }
    let h = serde_json::from_str::<crate::json::Object<Header>>(text)
        .map_err(|e| TableError::Format(format!("header: {e}")))?
        .0
        .ok_or_else(|| TableError::Format("header is not an object".into()))?;
    let header = TableHeader {
        name: h.name.0,
        symbol: h.symbol.0,
        tag: h.tag.0,
        data_url: h.data_url.0,
        level_order: h.level_order.0,
        courses: h.course.0,
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

/// The entries a table may have: some eighty times the largest tables'.
const MAX_ENTRIES: usize = 200_000;

/// A data JSON's entries (those with a hash), read straight from its
/// text: a value of any JSON type where text is expected (a level as a
/// number), anything but an object in the list passed by, and what has no
/// hash dropped as it is read (a list of millions of empty entries costs
/// nothing); more than [`MAX_ENTRIES`] is no table.
pub(crate) fn parse_data(text: &str) -> Result<Vec<TableEntry>, TableError> {
    struct Entries(Vec<TableEntry>);
    impl<'de> Deserialize<'de> for Entries {
        fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Entries, D::Error> {
            struct V;
            impl<'de> Visitor<'de> for V {
                type Value = Entries;
                fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                    f.write_str("a list of entries")
                }
                fn visit_seq<A: SeqAccess<'de>>(self, mut s: A) -> Result<Entries, A::Error> {
                    let mut out = Vec::new();
                    while let Some(Row(row)) = s.next_element()? {
                        let Some(e) = row.filter(|e| !e.md5.is_empty() || !e.sha256.is_empty())
                        else {
                            continue;
                        };
                        if out.len() == MAX_ENTRIES {
                            return Err(serde::de::Error::custom(format_args!(
                                "more than {MAX_ENTRIES} entries"
                            )));
                        }
                        out.push(e);
                    }
                    Ok(Entries(out))
                }
            }
            d.deserialize_seq(V)
        }
    }
    serde_json::from_str::<Entries>(text)
        .map(|e| e.0)
        .map_err(|e| TableError::Format(format!("data: {e}")))
}

use serde::de::value::MapAccessDeserializer;
use serde::de::{IgnoredAny, MapAccess, SeqAccess, Visitor};

/// A visitor's methods for the JSON values that are text, taken as `$v`:
/// a string, a number, a boolean.
macro_rules! one {
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
    };
}

/// [`one`], and null and a list (read through) as no text.
macro_rules! scalars {
    ($t:ty, $v:expr) => {
        one!($t, $v);
        fn visit_unit<E>(self) -> Result<$t, E> {
            Ok($v(String::new()))
        }
        fn visit_seq<A: SeqAccess<'de>>(self, mut s: A) -> Result<$t, A::Error> {
            while s.next_element::<IgnoredAny>()?.is_some() {}
            Ok($v(String::new()))
        }
    };
}

/// A JSON value as text (a string, a number, a boolean; else nothing).
#[derive(Default)]
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

/// A JSON value as a list of texts: a list's values that are text (the
/// others, and empty ones, passed by), null as none, any other value as
/// the one text it is.
#[derive(Default)]
struct Texts(Vec<String>);

impl<'de> Deserialize<'de> for Texts {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Texts, D::Error> {
        struct V;
        impl<'de> Visitor<'de> for V {
            type Value = Texts;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("texts")
            }
            fn visit_unit<E>(self) -> Result<Texts, E> {
                Ok(Texts(Vec::new()))
            }
            fn visit_seq<A: SeqAccess<'de>>(self, mut s: A) -> Result<Texts, A::Error> {
                let mut out = Vec::new();
                while let Some(Text(t)) = s.next_element()? {
                    if !t.is_empty() {
                        out.push(t);
                    }
                }
                Ok(Texts(out))
            }
            fn visit_map<A: MapAccess<'de>>(self, mut m: A) -> Result<Texts, A::Error> {
                while m.next_entry::<IgnoredAny, IgnoredAny>()?.is_some() {}
                Ok(Texts(vec![String::new()]))
            }
            one!(Texts, |t| Texts(vec![t]));
        }
        d.deserialize_any(V)
    }
}

crate::json::last_wins! {
    /// A course of a header's `course`.
    struct CourseRow {
        name: Text,
        md5: Texts,
        sha256: Texts,
        constraint: Texts,
        trophy: Vec<TrophyRow> = crate::json::objects,
    }
}

crate::json::last_wins! {
    struct TrophyRow {
        name: Text,
        missrate: f64 = rate,
        scorerate: f64 = rate,
    }
}

impl From<CourseRow> for Course {
    fn from(c: CourseRow) -> Course {
        let trophy = |t: TrophyRow| CourseTrophy {
            name: t.name.0,
            missrate: t.missrate,
            scorerate: t.scorerate,
        };
        Course {
            name: c.name.0,
            md5: c.md5.0,
            sha256: c.sha256.0,
            constraint: c.constraint.0,
            trophy: c.trophy.into_iter().map(trophy).collect(),
        }
    }
}

/// A visitor's methods for a header's `course`, or for a list in it:
/// one course (an object) alone, null and other values as none; a list's
/// elements are `$inner`s.
macro_rules! courses {
    ($t:ident, $inner:ty) => {
        impl<'de> Deserialize<'de> for $t {
            fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<$t, D::Error> {
                struct V;
                impl<'de> Visitor<'de> for V {
                    type Value = $t;
                    fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                        f.write_str("courses")
                    }
                    one!($t, |_| $t(Vec::new()));
                    fn visit_unit<E>(self) -> Result<$t, E> {
                        Ok($t(Vec::new()))
                    }
                    fn visit_seq<A: SeqAccess<'de>>(self, mut s: A) -> Result<$t, A::Error> {
                        let mut out = Vec::new();
                        while let Some(inner) = s.next_element::<$inner>()? {
                            out.extend(inner.0.into_iter().map(Course::from));
                        }
                        Ok($t(out))
                    }
                    fn visit_map<A: MapAccess<'de>>(self, m: A) -> Result<$t, A::Error> {
                        let c = CourseRow::deserialize(MapAccessDeserializer::new(m))?;
                        Ok($t(vec![c.into()]))
                    }
                }
                d.deserialize_any(V)
            }
        }
    };
}

/// A header's `course`: its courses (objects) in a list, or in lists in
/// the list ([`CourseList`]), or one alone; anything else passed by.
#[derive(Default)]
struct Courses(Vec<Course>);
courses!(Courses, CourseList);

/// A list of courses in a header's `course`, or a course in it (what is
/// in a list in such a list is none).
struct CourseList(Vec<Course>);
courses!(CourseList, crate::json::Object<CourseRow>);

/// A trophy's rate: a number (else 0).
fn rate<'de, D: serde::Deserializer<'de>>(d: D) -> Result<f64, D::Error> {
    struct V;
    impl<'de> Visitor<'de> for V {
        type Value = f64;
        fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
            f.write_str("a number")
        }
        fn visit_i64<E>(self, v: i64) -> Result<f64, E> {
            Ok(v as f64)
        }
        fn visit_u64<E>(self, v: u64) -> Result<f64, E> {
            Ok(v as f64)
        }
        fn visit_f64<E>(self, v: f64) -> Result<f64, E> {
            Ok(v)
        }
        fn visit_str<E>(self, _: &str) -> Result<f64, E> {
            Ok(0.0)
        }
        fn visit_bool<E>(self, _: bool) -> Result<f64, E> {
            Ok(0.0)
        }
        fn visit_unit<E>(self) -> Result<f64, E> {
            Ok(0.0)
        }
        fn visit_seq<A: SeqAccess<'de>>(self, mut s: A) -> Result<f64, A::Error> {
            while s.next_element::<IgnoredAny>()?.is_some() {}
            Ok(0.0)
        }
        fn visit_map<A: MapAccess<'de>>(self, mut m: A) -> Result<f64, A::Error> {
            while m.next_entry::<IgnoredAny, IgnoredAny>()?.is_some() {}
            Ok(0.0)
        }
    }
    d.deserialize_any(V)
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
                            for md5 in m.next_value::<Texts>()?.0 {
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
    Get(String, Kind),
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
                    Ok(Step::Get(mirror.to_string(), Kind::Page))
                }
                None => Err(e),
            },
        }
    }

    fn advance(&mut self, body: Answer) -> Result<Step, TableError> {
        let body = body.map_err(|e| TableError::Http(e.to_string()))?;
        // (a driver reads no more; one that did is not parsed)
        let limit = match self.stage {
            Stage::Page(_) => Kind::Page,
            Stage::Header(_) => Kind::Header,
            Stage::Data(_) => Kind::TableData,
        }
        .limit();
        if body.len() > limit {
            return Err(TableError::Http(crate::net::too_large(limit).to_string()));
        }
        let text = decode_body(&body);
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
                Ok(Step::Get(hu, Kind::Header))
            }
            Stage::Header(url) => {
                let header = parse_header(text.trim_start_matches('\u{feff}'))?;
                let du = join(&url, &header.data_url)?;
                self.stage = Stage::Data(header);
                Ok(Step::Get(du, Kind::TableData))
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
        Next::Ask(Request::Get(self.source.clone(), Kind::Page))
    }

    fn feed(&mut self, answer: Answer) -> Next<Self::Output> {
        match self.step(answer) {
            Ok(Step::Get(u, kind)) => Next::Ask(Request::Get(u, kind)),
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
        // (a table sets how many of each there are)
        let mut known = std::collections::HashSet::new();
        let mut seen: Vec<String> = Vec::new();
        for l in &self.header.level_order {
            if known.insert(l.as_str()) {
                seen.push(l.clone());
            }
        }
        let mut rest: Vec<String> = self
            .entries
            .iter()
            .filter(|e| !known.contains(e.level.as_str()))
            .map(|e| e.level.clone())
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

    /// A header's values of any JSON type, its courses however they are
    /// listed.
    #[test]
    fn headers_of_any_shape() {
        let h = parse_header(
            r#"{"name":1,"symbol":true,"tag":null,"data_url":"d.json","level_order":"x",
                "x":{"y":[1,{"z":[]}]},
                "course":{"name":"c","md5":"a","sha256":null,"constraint":["g",1,"",[2],{}],
                    "trophy":[{"name":"t","missrate":5,"scorerate":"x"},3,{"scorerate":1.5}]}}"#,
        )
        .unwrap();
        assert_eq!((&*h.name, &*h.symbol, &*h.tag), ("1", "true", ""));
        assert_eq!(h.level_order, ["x"]);
        let c = &h.courses[0];
        assert_eq!(
            (h.courses.len(), &*c.name, &c.md5[..]),
            (1, "c", &["a".to_string()][..])
        );
        assert!(c.sha256.is_empty());
        assert_eq!(c.constraint, ["g", "1"]);
        let rates: Vec<(f64, f64)> = c.trophy.iter().map(|t| (t.missrate, t.scorerate)).collect();
        assert_eq!(rates, [(5.0, 0.0), (0.0, 1.5)]);
        // courses in a list, in lists in the list; what else is there is none
        let h = parse_header(
            r#"{"data_url":"d","level_order":[0,"1",null,2.5],
                "course":[{"name":"a"},[{"name":"b"},7,[{"name":"deep"}],{"name":"c"}],"x",null]}"#,
        )
        .unwrap();
        assert_eq!(h.level_order, ["0", "1", "2.5"]);
        let names: Vec<&str> = h.courses.iter().map(|c| &*c.name).collect();
        assert_eq!(names, ["a", "b", "c"]);
        for (odd, why) in [
            ("[]", "not an object"),
            (r#"["n","s","t","d.json"]"#, "not an object"),
            ("{", "header: "),
            (r#"{"name":"x"}"#, "no data_url"),
            (r#"{"data_url":{}}"#, "no data_url"),
        ] {
            let e = parse_header(odd).unwrap_err().to_string();
            assert!(e.contains(why), "{odd}: {e}");
        }
    }

    /// A key given twice in a header, a course or a trophy: its last value,
    /// as in a data row.
    #[test]
    fn a_key_twice() {
        let h = parse_header(
            r#"{"name":"a","data_url":"d.json","name":"b",
                "course":[{"name":"c","md5":["x"],"name":"d","trophy":[{"missrate":1,"missrate":2}]}]}"#,
        )
        .unwrap();
        assert_eq!(&*h.name, "b");
        let c = &h.courses[0];
        assert_eq!((&*c.name, &c.md5[..]), ("d", &["x".to_string()][..]));
        assert_eq!(c.trophy[0].missrate, 2.0);
        let d =
            parse_data(r#"[{"md5":"0123456789abcdef0123456789abcdef","level":"1","level":"2"}]"#);
        assert_eq!(d.unwrap()[0].level, "2");
    }

    /// A data JSON made to be large to hold: what is no entry is dropped
    /// as it is read, and entries beyond counting are no table.
    #[test]
    fn data_made_to_be_large() {
        let none = format!("[{}{{}}]", "1,{},[],{\"md5\":\"x\"},".repeat(1_000_000));
        assert!(parse_data(&none).unwrap().is_empty());
        let row = r#"{"md5":"0123456789abcdef0123456789abcdef"},"#;
        let full = format!("[{}1]", row.repeat(MAX_ENTRIES));
        assert_eq!(parse_data(&full).unwrap().len(), MAX_ENTRIES);
        let more = format!("[{}1]", row.repeat(MAX_ENTRIES + 1));
        let e = parse_data(&more).unwrap_err().to_string();
        assert!(e.contains("more than 200000 entries"), "{e}");
        // a list where one text is expected is read past, not kept
        let listed = format!(
            r#"[{{"md5":"0123456789abcdef0123456789abcdef","org_md5":[{}"A"],"title":[{}1]}}]"#,
            "0,[],".repeat(500_000),
            "0,".repeat(500_000)
        );
        let e = parse_data(&listed).unwrap();
        assert_eq!((e[0].org_md5.len(), &*e[0].title), (2, ""));
    }

    /// Each of a table's answers is taken no larger than its kind may be,
    /// whoever read it, and asked as its kind.
    #[test]
    fn answers_of_their_kinds() {
        let mut l = TableLoader::new("https://ex.com/t/");
        let page = "https://ex.com/t/".to_string();
        assert!(matches!(l.start(), Next::Ask(r) if r == Request::Get(page, Kind::Page)));
        let meta = br#"<meta name="bmstable" content="h.json">"#.to_vec();
        let header = "https://ex.com/t/h.json".to_string();
        let asked = l.feed(Ok(meta));
        assert!(matches!(asked, Next::Ask(r) if r == Request::Get(header, Kind::Header)));
        let data = "https://ex.com/t/d.json".to_string();
        let asked = l.feed(Ok(br#"{"data_url":"d.json"}"#.to_vec()));
        assert!(matches!(asked, Next::Ask(r) if r == Request::Get(data, Kind::TableData)));
        let mut l = TableLoader::new("https://ex.com/t/");
        l.start();
        l.feed(Ok(br#"<meta name="bmstable" content="h.json">"#.to_vec()));
        let mut long = br#"{"data_url":"d.json","x":""#.to_vec();
        long.resize(Kind::Header.limit(), b'x');
        long.extend(b"\"}");
        let Next::Done(Err(e)) = l.feed(Ok(long)) else {
            panic!("a header over its limit was read")
        };
        assert!(e.to_string().contains("larger than 1 MiB"), "{e}");
    }

    /// A table of levels beyond counting is put in order in its size's
    /// time.
    #[test]
    fn many_levels() {
        let n = 100_000;
        let t = DifficultyTable {
            header: TableHeader {
                level_order: (0..n)
                    .map(|i| format!("o{i}"))
                    .chain(["o1".into()])
                    .collect(),
                ..Default::default()
            },
            entries: (0..n)
                .map(|i| TableEntry {
                    level: format!("{}", i % (n / 2)),
                    ..Default::default()
                })
                .collect(),
            ..Default::default()
        };
        let since = std::time::Instant::now();
        let levels = t.levels();
        assert!(since.elapsed() < std::time::Duration::from_secs(10));
        assert_eq!(levels.len(), n + n / 2);
        assert_eq!(
            (&*levels[1], &*levels[n], &*levels[n + 10]),
            ("o1", "0", "10")
        );
    }
}
