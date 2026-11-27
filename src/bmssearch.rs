//! Links for a chart from BMS SEARCH (bmssearch.net), its public API: the
//! chart (by MD5) → the BMS work it belongs to → the work's download links
//! as its author gave them (and its page on an event venue). Where the
//! chart is not tied to a work (most 差分 charts), the work is searched by
//! title and artist.
//!
//! [`Lookup`] is a [`Task`] (`api.bmssearch.net` sends CORS headers, so a
//! web page asks it itself) that ends with the links.

use crate::hosts::Want;
use crate::net::{Answer, Kind, Next, Request, Task};
use serde::Deserialize;

type Step = Next<Vec<String>>;

fn get(url: String) -> Step {
    Next::Ask(Request::Get(url, Kind::Api))
}

pub(crate) struct Lookup {
    api: String,
    md5: Option<String>,
    want: Want,
    wait: Wait,
}

enum Wait {
    Pattern,
    Bms,
    /// (title, artist) searched for
    Search(String, String),
}

/// Event venues whose work pages lead to the download (the resolver reads
/// them); other related links (a video, an author's profile) are left out.
const VENUES: &[&str] = &["manbow.nothing.sh", "venue.bmssearch.net"];

/// A BMS work as the API gives it: what is looked at.
#[derive(Deserialize, Default)]
#[serde(default)]
pub(crate) struct Work {
    #[serde(deserialize_with = "crate::json::text")]
    title: String,
    #[serde(deserialize_with = "crate::json::text")]
    artist: String,
    #[serde(deserialize_with = "crate::json::objects")]
    downloads: Vec<Link>,
    #[serde(rename = "relatedLinks", deserialize_with = "crate::json::objects")]
    related_links: Vec<Link>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct Link {
    #[serde(deserialize_with = "crate::json::text")]
    url: String,
}

/// A chart (`/patterns/<md5>`) as the API gives it: the work it belongs
/// to, its own title and artist.
#[derive(Deserialize, Default)]
#[serde(default)]
pub(crate) struct Pattern {
    #[serde(deserialize_with = "crate::json::object")]
    pub(crate) bms: Bms,
    #[serde(deserialize_with = "crate::json::text")]
    pub(crate) title: String,
    #[serde(deserialize_with = "crate::json::text")]
    pub(crate) artist: String,
    #[serde(deserialize_with = "crate::json::object")]
    pub(crate) file: File,
}

/// A chart's work.
#[derive(Deserialize, Default)]
#[serde(default)]
pub(crate) struct Bms {
    #[serde(deserialize_with = "crate::json::text")]
    pub(crate) id: String,
    #[serde(deserialize_with = "crate::json::text")]
    pub(crate) title: String,
}

/// A chart's file.
#[derive(Deserialize, Default)]
#[serde(default)]
pub(crate) struct File {
    #[serde(deserialize_with = "crate::json::text")]
    pub(crate) name: String,
}

impl Work {
    /// Its links: its downloads, then its venue pages.
    pub(crate) fn links(&self) -> Vec<String> {
        let urls = |links: &[Link]| -> Vec<String> {
            links
                .iter()
                .map(|d| d.url.trim().to_string())
                .filter(|u| u.starts_with("http"))
                .collect()
        };
        let mut out = urls(&self.downloads);
        for u in urls(&self.related_links) {
            let host = u.split('/').nth(2).unwrap_or("").to_ascii_lowercase();
            if VENUES.contains(&host.as_str()) && !out.contains(&u) {
                out.push(u);
            }
        }
        out
    }
}

/// The title to search for: the table's without its chart's name
/// (`Song [ANOTHER]`, `Song -Remix-`, `Song (7KEYS)`).
pub(crate) fn base_title(t: &str) -> String {
    let cut = t
        .char_indices()
        .find(|&(i, c)| {
            i > 0
                && (matches!(c, '[' | '(' | '（' | '【' | '<' | '～' | '〜')
                    || t[i..].starts_with(" -"))
        })
        .map_or(t.len(), |(i, _)| i);
    t[..cut].trim().to_string()
}

use crate::net::url_encode;

impl Lookup {
    /// From the chart `md5` (of the song `want`; without an MD5, the song
    /// by its title), asking the API at `api` ([`Config::bmssearch_api`]).
    ///
    /// [`Config::bmssearch_api`]: crate::config::Config::bmssearch_api
    pub(crate) fn new(md5: &str, want: &Want, api: &str) -> Lookup {
        Lookup {
            api: api.trim_end_matches('/').to_string(),
            md5: crate::chart::md5_key(md5),
            want: want.clone(),
            wait: Wait::Pattern,
        }
    }

    fn search(&mut self, title: &str, artist: &str) -> Step {
        let t = base_title(title);
        if t.chars().count() < 2 {
            return Next::Done(Vec::new());
        }
        let step = get(format!(
            "{}/bmses/search?title={}",
            self.api,
            url_encode(&t)
        ));
        self.wait = Wait::Search(t, artist.to_string());
        step
    }
}

impl Task for Lookup {
    type Output = Vec<String>;

    fn start(&mut self) -> Step {
        self.wait = Wait::Pattern;
        if let Some(m) = &self.md5 {
            get(format!("{}/patterns/{m}", self.api))
        } else {
            let (t, a) = (self.want.title.clone(), self.want.artist.clone());
            self.search(&t, &a)
        }
    }

    fn feed(&mut self, answer: Answer) -> Step {
        let body = answer.unwrap_or_default();
        match std::mem::replace(&mut self.wait, Wait::Bms) {
            Wait::Pattern => {
                let p: Pattern = crate::json::object_of(&body);
                if !p.bms.id.is_empty() {
                    self.wait = Wait::Bms;
                    return get(format!("{}/bmses/{}", self.api, url_encode(&p.bms.id)));
                }
                // not tied to a work: search by its title (BMS SEARCH's, as
                // the chart has it, else the table's)
                let or = |own: String, wanted: &str| match own.is_empty() {
                    true => wanted.to_string(),
                    false => own,
                };
                let title = or(p.title, &self.want.title);
                let artist = or(p.artist, &self.want.artist);
                self.search(&title, &artist)
            }
            Wait::Bms => Next::Done(crate::json::object_of::<Work>(&body).links()),
            Wait::Search(title, artist) => {
                let works: Vec<Work> = crate::json::objects_of(&body);
                let same: Vec<&Work> = works
                    .iter()
                    .filter(|w| crate::titles::same_title(&w.title, &title))
                    .collect();
                let by: Vec<&Work> = same
                    .iter()
                    .copied()
                    .filter(|w| crate::hosts::pack_artist(&w.artist, &artist))
                    .collect();
                // by its artist; else the one work of that title
                let pick: Vec<&Work> = if !by.is_empty() {
                    by
                } else if same.len() == 1 && title.chars().count() >= 6 {
                    same
                } else {
                    Vec::new()
                };
                let mut out: Vec<String> = Vec::new();
                for w in pick {
                    for u in w.links() {
                        if !out.contains(&u) {
                            out.push(u);
                        }
                    }
                }
                Next::Done(out)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const API: &str = "https://api.bmssearch.net/v1";

    fn start(md5: &str, w: Want) -> (Lookup, Step) {
        let mut l = Lookup::new(md5, &w, API);
        let s = l.start();
        (l, s)
    }

    fn want(t: &str, a: &str) -> Want {
        Want {
            title: t.into(),
            artist: a.into(),
        }
    }

    #[test]
    fn titles() {
        assert_eq!(base_title("Beatrice [ANOTHER]"), "Beatrice");
        assert_eq!(base_title("AVALON -Genocide-"), "AVALON");
        assert_eq!(
            base_title("Tell The Truth (7KEYS ANOTHER)"),
            "Tell The Truth"
        );
        assert_eq!(base_title("[ASIA]"), "[ASIA]");
    }

    #[test]
    fn a_tied_chart() {
        let (mut l, s) = start("AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA", want("Song [X]", "a"));
        assert_eq!(
            s,
            get(format!("{API}/patterns/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"))
        );
        let s = l.feed(Ok(
            br#"{"bms":{"id":"W1","title":"Song"},"title":"Song"}"#.to_vec()
        ));
        assert_eq!(s, get(format!("{API}/bmses/W1")));
        let bms = br#"{"downloads":[{"url":"https://dl.example/song.zip"}],
            "relatedLinks":[{"url":"https://twitter.com/x"},{"url":"http://manbow.nothing.sh/event/event.cgi?action=More_def&num=1&event=1"}]}"#;
        assert_eq!(
            l.feed(Ok(bms.to_vec())),
            Next::Done(vec![
                "https://dl.example/song.zip".into(),
                "http://manbow.nothing.sh/event/event.cgi?action=More_def&num=1&event=1".into()
            ])
        );
    }

    #[test]
    fn an_untied_chart_by_its_title() {
        let (mut l, _) = start("0123456789abcdef0123456789abcdef", want("x", "y"));
        let s = l.feed(Ok(br#"{"title":"$trange Attraktor","artist":"schwarzekugel / You might not need Scratch"}"#.to_vec()));
        assert_eq!(
            s,
            get(format!("{API}/bmses/search?title=%24trange%20Attraktor"))
        );
        let found = br#"[{"title":"$trange Attraktor","artist":"schwarzekugel","downloads":[{"url":"https://d/s.zip"}]},
            {"title":"$trange Attraktor II","artist":"other","downloads":[{"url":"https://d/2.zip"}]}]"#;
        assert_eq!(
            l.feed(Ok(found.to_vec())),
            Next::Done(vec!["https://d/s.zip".into()])
        );
        // unknown chart: the table's title
        let (mut l, _) = start(
            "0123456789abcdef0123456789abcdef",
            want("Beatrice [ANOTHER]", "xi"),
        );
        assert_eq!(
            l.feed(Err(crate::net::NetError::Status(404))),
            get(format!("{API}/bmses/search?title=Beatrice"))
        );
        assert_eq!(l.feed(Ok(b"[]".to_vec())), Next::Done(Vec::new()));
    }

    /// `BMSEEKER_MD5=… BMSEEKER_TITLE=… cargo test -p bmseeker bmssearch_live -- --ignored`
    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    #[ignore = "network"]
    fn bmssearch_live() {
        let md5 = std::env::var("BMSEEKER_MD5").unwrap_or_default();
        let w = want(
            &std::env::var("BMSEEKER_TITLE").unwrap_or_default(),
            &std::env::var("BMSEEKER_ARTIST").unwrap_or_default(),
        );
        let cx = crate::hosts::Context::default();
        let mut job =
            crate::net::job::TaskJob::new(Lookup::new(&md5, &w, &cx.config.bmssearch_api));
        let found = crate::net::job::run_job(&mut job, &crate::native::Native::new(&cx));
        println!("{found:?}");
    }
}
