//! Where else a song's (or its separate chart's) links are looked for when
//! its own do not bring it: each place is a `Source` (link lists by MD5:
//! difficulty tables, the Stella uploader's list, TSV lists; BMS SEARCH by
//! MD5, else by title and artist; mirrors of dead sites' charts by the file
//! name BMS SEARCH knows; Ginger's and Konmai's packages by MD5; the
//! community archive's songs by title and artist), and [`Sources`] asks
//! them in the order
//! [`Config::sources`] gives. A source answers with a [`Task`], so one that
//! reads the network runs anywhere a task does; the sources of one
//! [`Sources`] share their answers ([`Memo`]: BMS SEARCH is asked of a
//! chart once, the community archive's folder of a letter listed once).
//!
//! [`Config::sources`]: crate::config::Config::sources

use crate::config::Config;
use crate::hosts::Want;
use crate::links::Links;
use crate::net::{Answer, BoxTask, Memo, Next, Ready, Task};
use std::sync::Arc;

/// What is looked for: a chart (by its MD5, if known) of the song `want`;
/// `diff`: the chart's own link (a 差分 posted apart from its song), else
/// the song's; `org_md5`: the MD5s of the song's own charts a table names
/// (where a separate chart goes).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Query {
    pub md5: String,
    pub want: Want,
    pub diff: bool,
    pub org_md5: Vec<String>,
}

/// A place a song's links are looked for.
pub(crate) trait Source: Send + Sync {
    /// Its name in [`Config::sources`] ("table", "stella", "tsv",
    /// "bmssearch").
    fn name(&self) -> &'static str;
    /// The links it knows for `q`, in its order.
    fn find(&self, q: &Query) -> BoxTask<Vec<String>>;
    /// Whether what its links bring must be checked to hold the chart (by
    /// its MD5) before it goes into the library: it found them by name.
    fn checked(&self) -> bool {
        false
    }
}

/// A list of links by MD5 ([`Links`]): difficulty tables, the Stella
/// uploader's list, a TSV list.
pub(crate) struct List {
    name: &'static str,
    links: Links,
}

impl List {
    pub(crate) fn new(name: &'static str, links: Links) -> List {
        List { name, links }
    }
}

impl Source for List {
    fn name(&self) -> &'static str {
        self.name
    }

    fn find(&self, q: &Query) -> BoxTask<Vec<String>> {
        let e = crate::table::TableEntry {
            md5: q.md5.clone(),
            ..Default::default()
        };
        Ready::boxed(if q.diff {
            self.links.chart_links(&e)
        } else {
            self.links.song_links(&e)
        })
    }
}

/// BMS SEARCH: the work a chart belongs to and its download links (a song
/// only: BMS SEARCH lists works, not separate charts).
pub(crate) struct BmsSearch {
    api: String,
    memo: Arc<Memo>,
}

impl BmsSearch {
    pub(crate) fn new(config: &Config, memo: Arc<Memo>) -> BmsSearch {
        BmsSearch {
            api: config.bmssearch_api.clone(),
            memo,
        }
    }
}

impl Source for BmsSearch {
    fn name(&self) -> &'static str {
        "bmssearch"
    }

    fn find(&self, q: &Query) -> BoxTask<Vec<String>> {
        if q.diff {
            return Ready::boxed(Vec::new());
        }
        self.memo
            .wrap(crate::bmssearch::Lookup::new(&q.md5, &q.want, &self.api))
    }
}

/// Separate charts of dead sites kept in mirror folders under their file
/// names ([`Config::chart_mirrors`]): the name is BMS SEARCH's for the MD5.
///
/// [`Config::chart_mirrors`]: crate::config::Config::chart_mirrors
pub(crate) struct ChartMirrors {
    api: String,
    folders: Vec<String>,
    memo: Arc<Memo>,
}

impl ChartMirrors {
    pub(crate) fn new(config: &Config, memo: Arc<Memo>) -> ChartMirrors {
        ChartMirrors {
            api: config.bmssearch_api.trim_end_matches('/').to_string(),
            folders: config.chart_mirrors.clone(),
            memo,
        }
    }
}

impl Source for ChartMirrors {
    fn name(&self) -> &'static str {
        "mirrors"
    }

    fn checked(&self) -> bool {
        true
    }

    fn find(&self, q: &Query) -> BoxTask<Vec<String>> {
        let md5 = crate::chart::md5_key(&q.md5).filter(|_| q.diff && !self.folders.is_empty());
        let Some(md5) = md5 else {
            return Ready::boxed(Vec::new());
        };
        let folders = self.folders.clone();
        let find = crate::net::get_then(format!("{}/patterns/{md5}", self.api), move |answer| {
            // (nothing when it fails)
            let Ok(body) = answer else {
                return Vec::new();
            };
            let v: serde_json::Value = serde_json::from_slice(&body).unwrap_or_default();
            let name = v["file"]["name"].as_str().unwrap_or("");
            // a file name, nothing of a path
            if name.is_empty() || name.contains(['/', '\\']) || name.starts_with('.') {
                return Vec::new();
            }
            folders
                .iter()
                .map(|f| {
                    format!(
                        "{}/{}",
                        f.trim_end_matches('/'),
                        crate::net::url_encode(name)
                    )
                })
                .collect()
        });
        self.memo.wrap(find)
    }
}

/// A service that names the package (a song's archive) holding a chart,
/// by the chart's MD5: Ginger ([`Config::ginger`]: `downloadURL`), Konmai
/// ([`Config::konmai`]: `data.song_url` of a `"result": "success"`). Third
/// parties' copies, so what one brings is checked for the chart. A URL is
/// taken only when its path names an archive or a chart.
///
/// [`Config::ginger`]: crate::config::Config::ginger
/// [`Config::konmai`]: crate::config::Config::konmai
pub(crate) struct ByMd5 {
    name: &'static str,
    /// The lookup's URL, the MD5 after it.
    api: String,
    url: fn(&serde_json::Value) -> Option<&str>,
}

impl ByMd5 {
    pub(crate) fn ginger(config: &Config) -> ByMd5 {
        ByMd5 {
            name: "ginger",
            api: config.ginger.clone(),
            url: |v| v["downloadURL"].as_str(),
        }
    }

    pub(crate) fn konmai(config: &Config) -> ByMd5 {
        ByMd5 {
            name: "konmai",
            api: config.konmai.clone(),
            url: |v| {
                let ok = v["result"].as_str()?.eq_ignore_ascii_case("success");
                v["data"]["song_url"].as_str().filter(|_| ok)
            },
        }
    }
}

/// Whether `url` is an http(s) link to an archive or a chart (by its path).
fn file_link(url: &str) -> bool {
    let lower = url.to_ascii_lowercase();
    let path = lower.split(['?', '#']).next().unwrap_or("");
    (lower.starts_with("https://") || lower.starts_with("http://"))
        && (crate::install::chart_link(path)
            || crate::unpack::is_archive(std::path::Path::new(path)))
}

impl Source for ByMd5 {
    fn name(&self) -> &'static str {
        self.name
    }

    fn checked(&self) -> bool {
        true
    }

    fn find(&self, q: &Query) -> BoxTask<Vec<String>> {
        let Some(md5) = crate::chart::md5_key(&q.md5).filter(|_| !self.api.is_empty()) else {
            return Ready::boxed(Vec::new());
        };
        let url = self.url;
        crate::net::get_then(format!("{}{md5}", self.api), move |answer| {
            // (nothing when it fails: not found is a 404)
            let v: serde_json::Value = answer
                .ok()
                .and_then(|b| serde_json::from_slice(&b).ok())
                .unwrap_or_default();
            url(&v)
                .map(str::trim)
                .filter(|u| file_link(u))
                .map(String::from)
                .into_iter()
                .collect()
        })
    }
}

/// The community archive's songs ([`Config::bms_library`]): a Google Drive
/// folder of a folder per first letter, each of `Title (by Artist).rar`.
/// Found by the song's title (BMS SEARCH's for the chart's MD5, else the
/// table's) and artist, so what one brings is checked for the chart.
///
/// [`Config::bms_library`]: crate::config::Config::bms_library
pub(crate) struct BmsLibrary {
    folder: String,
    api: String,
    memo: Arc<Memo>,
}

impl BmsLibrary {
    pub(crate) fn new(config: &Config, memo: Arc<Memo>) -> BmsLibrary {
        BmsLibrary {
            folder: crate::hosts::drive::drive_id(&config.bms_library).unwrap_or_default(),
            api: config.bmssearch_api.trim_end_matches('/').to_string(),
            memo,
        }
    }
}

/// Whether the archive's title `t` is the song `want` (a chart's title,
/// maybe with its chart's name after the song's: "AltMirrorBell Another").
fn titled(t: &str, want: &str) -> bool {
    use crate::placement::same_name;
    let (t, want) = (&plain(t), &plain(want));
    if same_name(t, want) {
        return true;
    }
    let (t, want) = (t.trim().to_lowercase(), want.trim().to_lowercase());
    t.chars().count() >= 3 && want.starts_with(&format!("{t} "))
}

/// `s` as the archive names files: full-width letters as ASCII ones, and
/// without what a Windows file name cannot hold (`Re: X` is `Re： X`
/// there).
fn plain(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            '\u{FF01}'..='\u{FF5E}' => char::from_u32(c as u32 - 0xFEE0).unwrap_or(c),
            '\u{3000}' => ' ',
            c => c,
        })
        .filter(|c| !matches!(c, '\\' | '/' | ':' | '*' | '?' | '"' | '<' | '>' | '|'))
        .collect()
}

/// The archive's folder for titles starting with `c`.
fn letter_folder(c: char) -> &'static str {
    const LETTERS: [&str; 26] = [
        "A", "B", "C", "D", "E", "F", "G", "H", "I", "J", "K", "L", "M", "N", "O", "P", "Q", "R",
        "S", "T", "U", "V", "W", "X", "Y", "Z",
    ];
    // full width as half
    let c = match c {
        'Ａ'..='Ｚ' | 'ａ'..='ｚ' | '０'..='９' => {
            char::from_u32(c as u32 - 0xFEE0).unwrap_or(c)
        }
        _ => c,
    };
    match c {
        'a'..='z' | 'A'..='Z' => LETTERS[(c.to_ascii_uppercase() as u8 - b'A') as usize],
        '0'..='9' => "0-9",
        '\u{3040}'..='\u{30FF}' | '\u{31F0}'..='\u{31FF}' | '\u{FF66}'..='\u{FF9F}' => {
            "ひらがな・カタカナ"
        }
        '\u{0370}'..='\u{03FF}' => "ギリシャ文字",
        '\u{3400}'..='\u{4DBF}' | '\u{4E00}'..='\u{9FFF}' | '\u{F900}'..='\u{FAFF}' => "漢字",
        _ => "シンボル",
    }
}

/// `Title (by Artist).rar`: (title, artist).
fn by_artist(name: &str) -> Option<(&str, &str)> {
    let stem = name.rsplit_once('.').map_or(name, |(s, _)| s);
    let (title, rest) = stem.rsplit_once(" (by ")?;
    Some((title, rest.strip_suffix(')')?))
}

impl Source for BmsLibrary {
    fn name(&self) -> &'static str {
        "bmslibrary"
    }

    fn checked(&self) -> bool {
        true
    }

    fn find(&self, q: &Query) -> BoxTask<Vec<String>> {
        let title = q.want.title.trim().to_string();
        let md5 = crate::chart::md5_key(&q.md5);
        // a separate chart too: a song's archive there may hold it (checked)
        if self.folder.is_empty() || (title.is_empty() && md5.is_none()) {
            return Ready::boxed(Vec::new());
        }
        self.memo.wrap(ArchiveSearch {
            root: self.folder.clone(),
            pattern: md5.map(|m| format!("{}/patterns/{m}", self.api)),
            table_title: title,
            titles: Vec::new(),
            artist: q.want.artist.clone(),
            wait: Wait::Pattern,
        })
    }
}

/// BMS SEARCH's title of the chart's song (when its MD5 is known), the
/// archive's root, the letter's folder, then the song's archives.
struct ArchiveSearch {
    root: String,
    pattern: Option<String>,
    table_title: String,
    /// The titles looked for: BMS SEARCH's song's, the table's.
    titles: Vec<String>,
    artist: String,
    wait: Wait,
}

#[derive(Clone, Copy)]
enum Wait {
    Pattern,
    Root,
    Letter,
}

impl ArchiveSearch {
    fn root(&mut self) -> Next<Vec<String>> {
        self.titles.retain(|t| !t.trim().is_empty());
        if self.titles.is_empty() {
            return Next::Done(Vec::new());
        }
        self.wait = Wait::Root;
        Next::Ask(crate::net::Request::Get(crate::hosts::drive::listing_url(
            &self.root,
        )))
    }
}

impl Task for ArchiveSearch {
    type Output = Vec<String>;

    fn start(&mut self) -> Next<Vec<String>> {
        self.titles = vec![self.table_title.clone()];
        match self.pattern.clone() {
            Some(url) => {
                self.wait = Wait::Pattern;
                Next::Ask(crate::net::Request::Get(url))
            }
            None => self.root(),
        }
    }

    fn feed(&mut self, answer: Answer) -> Next<Vec<String>> {
        match self.wait {
            Wait::Pattern => {
                // the song's title, the first to look for
                let v: serde_json::Value = answer
                    .ok()
                    .and_then(|b| serde_json::from_slice(&b).ok())
                    .unwrap_or_default();
                if let Some(t) = v["bms"]["title"].as_str().filter(|t| !t.trim().is_empty()) {
                    self.titles.insert(0, t.trim().to_string());
                }
                if self.artist.is_empty() {
                    self.artist = v["artist"].as_str().unwrap_or("").to_string();
                }
                self.root()
            }
            Wait::Root => {
                let Ok(body) = answer else {
                    return Next::Done(Vec::new());
                };
                let entries = crate::hosts::drive::listing(&String::from_utf8_lossy(&body));
                let first = plain(&self.titles[0]).trim().chars().next().unwrap_or(' ');
                let letter = letter_folder(first);
                match entries.iter().find(|e| e.folder && e.name == letter) {
                    Some(e) => {
                        self.wait = Wait::Letter;
                        Next::Ask(crate::net::Request::Get(crate::hosts::drive::listing_url(
                            &e.id,
                        )))
                    }
                    None => Next::Done(Vec::new()),
                }
            }
            Wait::Letter => {
                let Ok(body) = answer else {
                    return Next::Done(Vec::new());
                };
                let entries = crate::hosts::drive::listing(&String::from_utf8_lossy(&body));
                let mut want = self.titles.clone();
                want.extend(self.titles.iter().map(|t| crate::bmssearch::base_title(t)));
                use crate::placement::same_artist;
                Next::Done(
                    entries
                        .iter()
                        .filter(|e| !e.folder)
                        .filter(|e| {
                            by_artist(&e.name).is_some_and(|(t, a)| {
                                want.iter().any(|w| titled(t, w))
                                    && (self.artist.is_empty()
                                        || same_artist(&plain(a), &plain(&self.artist)))
                            })
                        })
                        .map(|e| crate::hosts::drive::file_link(&e.id))
                        .collect(),
                )
            }
        }
    }
}

/// The sources, asked in order (a copy shares them).
#[derive(Clone, Default)]
pub struct Sources(Vec<Arc<dyn Source>>);

impl Sources {
    /// The sources [`Config::sources`] names, in its order: the link lists
    /// from `lists` (by name, those the caller has: "table", "stella",
    /// "tsv"), BMS SEARCH from the settings.
    pub fn from_config(config: &Config, mut lists: Vec<(&'static str, Links)>) -> Sources {
        let mut out = Sources::default();
        let memo = Arc::new(Memo::default());
        for name in &config.sources {
            if !config.uses(name) {
                continue;
            }
            if name == "bmssearch" {
                out.push(Box::new(BmsSearch::new(config, memo.clone())));
            } else if name == "mirrors" {
                out.push(Box::new(ChartMirrors::new(config, memo.clone())));
            } else if name == "ginger" {
                out.push(Box::new(ByMd5::ginger(config)));
            } else if name == "konmai" {
                out.push(Box::new(ByMd5::konmai(config)));
            } else if name == "bmslibrary" {
                out.push(Box::new(BmsLibrary::new(config, memo.clone())));
            } else if let Some(i) = lists.iter().position(|(n, _)| n == name) {
                let (n, links) = lists.remove(i);
                out.push(Box::new(List::new(n, links)));
            }
        }
        out
    }

    pub(crate) fn push(&mut self, source: Box<dyn Source>) {
        self.0.push(source.into());
    }

    pub fn names(&self) -> Vec<&'static str> {
        self.0.iter().map(|s| s.name()).collect()
    }

    pub(crate) fn iter(&self) -> impl Iterator<Item = &dyn Source> {
        self.0.iter().map(|s| s.as_ref())
    }

    /// Every source's links for `q`, in order, each once.
    pub fn find_all(&self, q: &Query) -> BoxTask<Vec<(&'static str, String)>> {
        Box::new(All {
            tasks: self.0.iter().map(|s| (s.name(), s.find(q))).collect(),
            at: 0,
            out: Vec::new(),
        })
    }
}

/// The sources' tasks one after another.
struct All {
    tasks: Vec<(&'static str, BoxTask<Vec<String>>)>,
    at: usize,
    out: Vec<(&'static str, String)>,
}

impl All {
    fn take(&mut self, name: &'static str, links: Vec<String>) {
        for l in links {
            if !self.out.iter().any(|(_, u)| *u == l) {
                self.out.push((name, l));
            }
        }
    }

    /// Start the sources from `at` until one asks.
    fn next(&mut self) -> Next<Vec<(&'static str, String)>> {
        while self.at < self.tasks.len() {
            let (name, t) = &mut self.tasks[self.at];
            let name = *name;
            match t.start() {
                Next::Ask(r) => return Next::Ask(r),
                Next::Done(links) => {
                    self.take(name, links);
                    self.at += 1;
                }
            }
        }
        Next::Done(std::mem::take(&mut self.out))
    }
}

impl Task for All {
    type Output = Vec<(&'static str, String)>;

    fn start(&mut self) -> Next<Self::Output> {
        self.at = 0;
        self.out.clear();
        self.next()
    }

    fn feed(&mut self, answer: Answer) -> Next<Self::Output> {
        let Some((name, t)) = self.tasks.get_mut(self.at) else {
            return Next::Done(std::mem::take(&mut self.out));
        };
        let name = *name;
        match t.feed(answer) {
            Next::Ask(r) => Next::Ask(r),
            Next::Done(links) => {
                self.take(name, links);
                self.at += 1;
                self.next()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::net::{run, Canned};

    #[test]
    fn in_the_order_set() {
        let md5 = "0123456789abcdef0123456789abcdef";
        let stella = format!(
            r#"[{{"md5":"{md5}","url":"https://s.example/song.zip","url_diff":"https://s.example/diff.zip"}}]"#
        );
        let tsv = format!("md5\turl_diff\turl\n{md5}\t\thttps://t.example/song.zip\n");
        let config = Config {
            sources: ["tsv", "bmssearch", "stella"].map(String::from).to_vec(),
            ..Config::default()
        };
        let sources = Sources::from_config(
            &config,
            vec![
                ("stella", Links::from_stella(stella.as_bytes())),
                ("tsv", Links::from_tsv(&tsv)),
                ("table", Links::default()),
            ],
        );
        assert_eq!(sources.names(), ["tsv", "bmssearch", "stella"]);
        let q = Query {
            md5: md5.into(),
            want: Want::default(),
            diff: false,
            ..Default::default()
        };
        let api = &config.bmssearch_api;
        let http = Canned(vec![
            (format!("{api}/patterns/{md5}"), Ok(br#"{"bms":{"id":"W"}}"#.to_vec())),
            (
                format!("{api}/bmses/W"),
                Ok(br#"{"downloads":[{"url":"https://b.example/song.zip"},{"url":"https://t.example/song.zip"}]}"#.to_vec()),
            ),
        ]);
        let found = run(&mut sources.find_all(&q), &http);
        assert_eq!(
            found,
            [
                ("tsv", "https://t.example/song.zip".to_string()),
                ("bmssearch", "https://b.example/song.zip".to_string()),
                ("stella", "https://s.example/song.zip".to_string()),
            ]
        );
        // a separate chart: the lists' chart links, none from BMS SEARCH
        let q = Query { diff: true, ..q };
        assert_eq!(
            run(&mut sources.find_all(&q), &http),
            [("stella", "https://s.example/diff.zip".to_string())]
        );
    }

    /// Ginger's and Konmai's packages, by MD5, in their answers' shapes.
    #[test]
    fn packages_by_md5() {
        let md5 = "176c2b2db4efd66cf186caae7923d477";
        let config = Config::default();
        let (ginger, konmai) = (ByMd5::ginger(&config), ByMd5::konmai(&config));
        let (g, k) = (
            format!("https://gingerrush.com/download/package/{md5}"),
            format!("https://bms.alvorna.com/api/hash?md5={md5}"),
        );
        let found = |http: Canned, md5: &str| {
            let q = Query {
                md5: md5.into(),
                ..Default::default()
            };
            (
                run(&mut ginger.find(&q), &http),
                run(&mut konmai.find(&q), &http),
            )
        };
        let http = Canned(vec![
            (
                g.clone(),
                Ok(br#"{"id":2420,"fileName":"x.7z","downloadURL":"https://pixeldrain.net/api/filesystem/x9/%EF%BC%82F%EF%BC%82.7z","songs":[]}"#.to_vec()),
            ),
            (
                k.clone(),
                Ok(br#"{"result":"success","msg":"","data":{"song_name":"F","song_url":"https://bms.alvorna.com/bms/zipped/F.7z"}}"#.to_vec()),
            ),
        ]);
        assert_eq!(
            found(http, &md5.to_uppercase()),
            (
                vec!["https://pixeldrain.net/api/filesystem/x9/%EF%BC%82F%EF%BC%82.7z".to_string()],
                vec!["https://bms.alvorna.com/bms/zipped/F.7z".to_string()]
            )
        );
        // not found (a 404; Konmai's says so too), a page, no MD5
        let http = Canned(vec![
            (g.clone(), Err(crate::net::NetError::Status(404))),
            (
                k.clone(),
                Ok(br#"{"result":"fail","msg":"No chart found in database","data":{"song_url":"https://x/a.zip"}}"#.to_vec()),
            ),
        ]);
        assert_eq!(found(http, md5), (vec![], vec![]));
        let http = Canned(vec![(
            g,
            Ok(br#"{"downloadURL":"https://example.com/share/page"}"#.to_vec()),
        )]);
        assert_eq!(found(http, md5).0, Vec::<String>::new());
        assert_eq!(found(Canned(vec![]), "").0, Vec::<String>::new());
        assert!(ginger.checked() && konmai.checked());
        let all = Config {
            third_party: true,
            ..Config::default()
        };
        let names = Sources::from_config(&all, Vec::new()).names();
        assert_eq!(
            names,
            ["bmssearch", "mirrors", "ginger", "konmai", "bmslibrary"]
        );
    }

    #[test]
    fn chart_mirrors_by_file_name() {
        let md5 = "5cf55487a3cb0dce8d0a40a791955e13";
        let config = Config::default();
        let api = &config.bmssearch_api;
        let http = Canned(vec![(
            format!("{api}/patterns/{md5}"),
            Ok(br#"{"file":{"name":"airAA.bme"}}"#.to_vec()),
        )]);
        let m = ChartMirrors::new(&config, Arc::default());
        let q = Query {
            md5: md5.into(),
            want: Want::default(),
            diff: true,
            ..Default::default()
        };
        assert_eq!(
            run(&mut m.find(&q), &http),
            ["https://darksabun.club/mirror/absolute/bms/airAA.bme"]
        );
        assert!(m.checked());
        // a song: not theirs
        let q = Query { diff: false, ..q };
        assert!(run(&mut m.find(&q), &http).is_empty());
    }

    #[test]
    fn the_community_library() {
        assert_eq!(letter_folder('a'), "A");
        assert_eq!(letter_folder('７'), "0-9");
        assert_eq!(letter_folder('ア'), "ひらがな・カタカナ");
        assert_eq!(letter_folder('Ω'), "ギリシャ文字");
        assert_eq!(letter_folder('雨'), "漢字");
        assert_eq!(letter_folder('['), "シンボル");
        assert_eq!(by_artist("Air (by SHIKI).rar"), Some(("Air", "SHIKI")));
        assert_eq!(by_artist("A (b) (by C (D)).rar"), Some(("A (b)", "C (D)")));
        let config = Config::default();
        let lib = BmsLibrary::new(&config, Arc::default());
        let entry = |href: &str, name: &str| {
            format!(
                r#"<div class="flip-entry"><a href="{href}"><div class="flip-entry-title">{name}</div></a></div>"#
            )
        };
        let root = entry("https://drive.google.com/drive/folders/FA", "A");
        let a = [
            entry(
                "https://drive.google.com/file/d/X1/view",
                "Airship (by SHIKI).rar",
            ),
            entry(
                "https://drive.google.com/file/d/X2/view",
                "Air (by SHIKI).rar",
            ),
            entry(
                "https://drive.google.com/file/d/X3/view",
                "Air (by someone).rar",
            ),
        ]
        .concat();
        let http = Canned(vec![
            (
                "https://drive.google.com/embeddedfolderview?id=1mTcWY9jYMsnEzbVfvGkjfepBpN4Tj786"
                    .into(),
                Ok(root.into_bytes()),
            ),
            (
                "https://drive.google.com/embeddedfolderview?id=FA".into(),
                Ok(a.into_bytes()),
            ),
        ]);
        let q = Query {
            md5: String::new(),
            want: Want {
                title: "Air [ANOTHER+]".into(),
                artist: "SHIKI / black train".into(),
            },
            diff: false,
            ..Default::default()
        };
        assert_eq!(
            run(&mut lib.find(&q), &http),
            ["https://drive.google.com/file/d/X2/view"]
        );
        // a chart's title: the song's, then its chart's name
        assert!(titled("AltMirrorBell", "AltMirrorBell Another"));
        assert!(!titled("Air", "Airship"));
        assert!(titled("Re： Shinin' Queen", "Re: Shinin' Queen"));
        // by the chart's MD5: BMS SEARCH's title of its song (another run:
        // the letter's listing is another)
        let lib = BmsLibrary::new(&config, Arc::default());
        let md5 = "0d1a3240230a854d05877f0d523a1409";
        let api = &config.bmssearch_api;
        let b = entry(
            "https://drive.google.com/file/d/B1/view",
            "AltMirrorBell (by Grand thaw).rar",
        );
        let http = Canned(vec![
            (
                format!("{api}/patterns/{md5}"),
                Ok(br#"{"bms":{"title":"AltMirrorBell"},"artist":"Grand thaw"}"#.to_vec()),
            ),
            (
                "https://drive.google.com/embeddedfolderview?id=1mTcWY9jYMsnEzbVfvGkjfepBpN4Tj786"
                    .into(),
                Ok(entry("https://drive.google.com/drive/folders/FA", "A").into_bytes()),
            ),
            (
                "https://drive.google.com/embeddedfolderview?id=FA".into(),
                Ok(b.into_bytes()),
            ),
        ]);
        let q = Query {
            md5: md5.into(),
            want: Want {
                title: "AltMirrorBell Another".into(),
                artist: "Grand thaw".into(),
            },
            diff: false,
            ..Default::default()
        };
        assert_eq!(
            run(&mut lib.find(&q), &http),
            ["https://drive.google.com/file/d/B1/view"]
        );
        // not asked unless named
        assert!(!Sources::from_config(&config, vec![])
            .names()
            .contains(&"bmslibrary"));
    }

    /// A run's sources share their answers: BMS SEARCH is asked of a chart
    /// once for the three that ask it, the archive's folders once a letter.
    #[test]
    fn asked_once() {
        let config = Config {
            sources: ["bmssearch", "mirrors", "bmslibrary"]
                .map(String::from)
                .to_vec(),
            third_party: true,
            ..Config::default()
        };
        let sources = Sources::from_config(&config, Vec::new());
        let api = &config.bmssearch_api;
        let (a, b) = ("a".repeat(32), "b".repeat(32));
        let pattern = |md5: &str, title: &str| {
            (
                format!("{api}/patterns/{md5}"),
                Ok(
                    format!(r#"{{"bms":{{"title":"{title}"}},"file":{{"name":"{title}.bme"}}}}"#)
                        .into_bytes(),
                ),
            )
        };
        let root =
            "https://drive.google.com/embeddedfolderview?id=1mTcWY9jYMsnEzbVfvGkjfepBpN4Tj786";
        let letter = "https://drive.google.com/embeddedfolderview?id=FA";
        let folder = r#"<div class="flip-entry"><a href="https://drive.google.com/drive/folders/FA"><div class="flip-entry-title">A</div></a></div>"#;
        let http = crate::net::Asked::new(Canned(vec![
            pattern(&a, "Air"),
            pattern(&b, "Aurora"),
            (root.into(), Ok(folder.into())),
            (letter.into(), Ok(Vec::new())),
        ]));
        for md5 in [&a, &b] {
            for diff in [false, true] {
                let q = Query {
                    md5: md5.clone(),
                    diff,
                    ..Default::default()
                };
                run(&mut sources.find_all(&q), &http);
            }
        }
        assert_eq!(http.times(&format!("{api}/patterns/{a}")), 1);
        assert_eq!(http.times(&format!("{api}/patterns/{b}")), 1);
        assert_eq!((http.times(root), http.times(letter)), (1, 1));
    }
}
