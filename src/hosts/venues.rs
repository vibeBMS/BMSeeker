//! A work's page on a BMS event venue names the author's download links:
//! the one the event mirror would take is followed, its mirrors after it.
//! For a manbow work the event index (vibeBMS/BMS,
//! [`Config::mirror_catalog`]) knows first: the mirror's file, else the
//! work's links as the index's checks order them (those gone left out).

use super::{get_then, Host, Link, Resolved, Target, Want};
use crate::config::Config;
use crate::net::{Answer, BoxTask, Next, Request, Task};

/// A work on manbow (`event.cgi?action=More_def&num=N&event=E`).
pub(crate) struct Manbow;

impl Host for Manbow {
    fn name(&self) -> &'static str {
        "manbow"
    }

    fn accepts(&self, link: &Link) -> bool {
        link.host == "manbow.nothing.sh" && link.lower.contains("action=more_def") && link.hops < 2
    }

    fn open(&self, link: &Link, _: Option<&Want>, config: &Config) -> BoxTask<Resolved> {
        let index = crate::manbow::work_page(&link.url)
            .filter(|_| link.hops == 0 && !config.mirror_catalog.is_empty());
        Box::new(ManbowWork {
            link: link.url.clone(),
            catalog: config.mirror_catalog.clone(),
            state: match index {
                Some((event, num)) => State::Catalog {
                    event: event.into(),
                    num: num.into(),
                },
                None => State::Venue,
            },
        })
    }
}

enum State {
    /// The mirror's list of events, for work `num` of manbow event `event`.
    Catalog {
        event: u64,
        num: u64,
    },
    /// An event's index, for its work `num`.
    Index {
        num: u64,
    },
    /// The work's page on the venue.
    Venue,
    Done,
}

struct ManbowWork {
    link: String,
    catalog: String,
    state: State,
}

impl ManbowWork {
    /// The work's page: its own links.
    fn venue(&mut self) -> Next<Resolved> {
        self.state = State::Venue;
        Next::Ask(Request::Get(self.link.clone()))
    }
}

impl Task for ManbowWork {
    type Output = Resolved;

    fn start(&mut self) -> Next<Resolved> {
        match self.state {
            State::Catalog { .. } => Next::Ask(Request::Get(self.catalog.clone())),
            _ => self.venue(),
        }
    }

    fn feed(&mut self, answer: Answer) -> Next<Resolved> {
        let json = |a: &Answer| -> Option<serde_json::Value> {
            serde_json::from_slice(a.as_ref().ok()?).ok()
        };
        match std::mem::replace(&mut self.state, State::Done) {
            State::Catalog { event, num } => {
                let v = json(&answer);
                let index = v
                    .as_ref()
                    .and_then(|v| v.as_array())
                    .into_iter()
                    .flatten()
                    .find_map(|e| {
                        (e.get("venue").and_then(|n| n.as_u64()) == Some(event))
                            .then(|| e.get("index")?.as_str())
                            .flatten()
                            .map(str::to_string)
                    });
                match index {
                    Some(index) => {
                        self.state = State::Index { num };
                        Next::Ask(Request::Get(index))
                    }
                    None => self.venue(),
                }
            }
            State::Index { num } => {
                let v = json(&answer);
                let work = v
                    .as_ref()
                    .and_then(|v| v.as_array())
                    .into_iter()
                    .flatten()
                    .find(|w| w.get("num").and_then(|n| n.as_u64()) == Some(num));
                // the main archive: "042.zip" (an add-on is "042+1.rar")
                let main = work
                    .and_then(|w| w.get("files")?.as_array())
                    .into_iter()
                    .flatten()
                    .find(|f| {
                        f.get("name")
                            .and_then(|n| n.as_str())
                            .is_some_and(|n| !n.contains('+'))
                    });
                let s = |k: &str| main.and_then(|f| f.get(k)?.as_str()).map(str::to_string);
                if let Some(url) = s("url") {
                    return Next::Done(Resolved::File(Target::named(url, s("original"))));
                }
                // not mirrored: its links as checked, the work's page last
                let mut links: Vec<String> = work
                    .and_then(|w| w.get("links")?.as_array())
                    .into_iter()
                    .flatten()
                    .filter_map(|l| l.as_str().map(str::to_string))
                    .collect();
                if links.is_empty() {
                    return self.venue();
                }
                links.push(self.link.clone());
                Next::Done(Resolved::Follow(links))
            }
            State::Venue => Next::Done(match answer {
                Ok(body) => {
                    let page = crate::text::decode_text(&body);
                    match pick(&venue_links(&page)) {
                        links if links.is_empty() => Resolved::Page,
                        links => Resolved::Follow(links),
                    }
                }
                Err(e) => Resolved::Failed(e),
            }),
            State::Done => Next::Done(Resolved::Failed("nothing asked".into())),
        }
    }
}

/// A work on a BMS SEARCH event venue (`venue.bmssearch.net/<event>/<work>`):
/// its manifest lists the links.
pub(crate) struct BmsSearchVenue;

impl Host for BmsSearchVenue {
    fn name(&self) -> &'static str {
        "bmssearch-venue"
    }

    fn accepts(&self, link: &Link) -> bool {
        link.host == "venue.bmssearch.net" && link.hops < 2 && work_path(&link.url).is_some()
    }

    fn open(&self, link: &Link, _: Option<&Want>, _: &Config) -> BoxTask<Resolved> {
        let path = work_path(&link.url).unwrap_or_default();
        get_then(format!("{path}/manifest"), |body| {
            let v: serde_json::Value = serde_json::from_slice(&body).unwrap_or_default();
            let s = |r: &serde_json::Value, k: &str| {
                r.get(k).and_then(|x| x.as_str()).unwrap_or("").to_string()
            };
            let links: Vec<(String, String)> = v
                .get("resources")
                .and_then(|r| r.as_array())
                .into_iter()
                .flatten()
                .filter(|r| s(r, "type") != "additional")
                .map(|r| (s(r, "name"), s(r, "url")))
                .filter(|(_, u)| u.starts_with("http"))
                .collect();
            match pick(&links) {
                links if links.is_empty() => Resolved::Page,
                links => Resolved::Follow(links),
            }
        })
    }
}

/// `https://venue.bmssearch.net/<event>/<work>` without a query (the
/// event's own page is no work).
fn work_path(u: &str) -> Option<String> {
    let path = u.split(['?', '#']).next()?.trim_end_matches('/');
    (path.split('/').filter(|p| !p.is_empty()).count() >= 4).then(|| path.to_string())
}

/// (label, URL) of the download links on a manbow work page: its
/// "Download" box, or on an older event's page its "DownLoadAddress" cell.
fn venue_links(page: &str) -> Vec<(String, String)> {
    let Some(at) = page.find("<h3>Download</h3>") else {
        return old_venue_links(page);
    };
    let block = &page[at + "<h3>Download</h3>".len()..];
    // up to the box's footer (its size and date)
    let end = ["</blockquote>", "<footer"]
        .iter()
        .filter_map(|e| block.find(e))
        .min()
        .unwrap_or(block.len());
    crate::manbow::links_of(&block[..end])
}

/// An older event's page (2010 and before): a table row whose header cell
/// says "DownLoadAddress", the links in the next cell.
fn old_venue_links(page: &str) -> Vec<(String, String)> {
    let lower = page.to_ascii_lowercase();
    let Some(at) = lower.find("downloadaddress") else {
        return Vec::new();
    };
    let cell = (|| {
        let head_end = at + lower[at..].find("</td>")? + "</td>".len();
        let open = head_end + lower[head_end..].find("<td")?;
        let start = open + lower[open..].find('>')? + 1;
        let end = start + lower[start..].find("</td>")?;
        Some(&page[start..end])
    })();
    cell.map(crate::manbow::links_of).unwrap_or_default()
}

/// A work's links in the order to try: the main archive the event mirror
/// would take, then its mirrors.
fn pick(links: &[(String, String)]) -> Vec<String> {
    crate::manbow::pick(links).main
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hosts::tests::resolve;

    const LINK: &str = "https://manbow.nothing.sh/event/event.cgi?action=More_def&num=94&event=146";
    const CATALOG: &str = "https://raw.githubusercontent.com/vibeBMS/BMS/main/events.json";
    const PAGE: &[u8] = br#"<h3>Download</h3></div><blockquote><p style="x">wav<br><a href="https://drive.google.com/file/d/W/view">https://drive.google.com/file/d/W/view</A><br>ogg<br><a href="https://drive.google.com/file/d/O/view">https://drive.google.com/file/d/O/view</A></p><footer>Total</footer></blockquote>"#;

    #[test]
    fn venue_pages() {
        let links = venue_links(&String::from_utf8_lossy(PAGE));
        assert_eq!(links.len(), 2);
        assert_eq!(links[0].0, "wav");
        assert_eq!(links[1].0, "wav ogg");
        assert_eq!(
            pick(&links).first().map(String::as_str),
            Some("https://drive.google.com/file/d/O/view")
        );
        // not mirrored: the venue page, then the ogg archive on Drive
        let catalog: &[u8] = br#"[{"tag":"x","venue":146,"mirror":false}]"#;
        let t = resolve(LINK, &[(CATALOG, catalog), (LINK, PAGE)]).unwrap();
        assert!(t.url.contains("id=O"));
    }

    #[test]
    fn mirrored_works() {
        let catalog: &[u8] = br#"[{"tag":"boftt","venue":146,"mirror":true,"index":"https://raw.example/boftt/index.json"}]"#;
        let index: &[u8] = br#"[{"num":94,"files":[{"name":"094+1.rar","url":"https://gh/094+1.rar","original":"add.rar"},{"name":"094.zip","url":"https://gh/094.zip","original":"song_ogg.zip"}]}]"#;
        let t = resolve(
            LINK,
            &[
                (CATALOG, catalog),
                ("https://raw.example/boftt/index.json", index),
            ],
        )
        .unwrap();
        assert_eq!(
            t,
            Target::named("https://gh/094.zip", Some("song_ogg.zip".into()))
        );
        // the mirror out of reach, or the work not in it: the venue page
        assert!(resolve(LINK, &[(LINK, PAGE)]).unwrap().url.contains("id=O"));
        let t = resolve(
            LINK,
            &[
                (CATALOG, catalog),
                ("https://raw.example/boftt/index.json", b"[]"),
                (LINK, PAGE),
            ],
        );
        assert!(t.unwrap().url.contains("id=O"));
    }

    #[test]
    fn older_pages() {
        let page = r#"<TD class="event" align="center"><font class="event" size=2><B>DownLoadAddress</B></td>
<td class="eventback" colspan="3"><a href="http://dead.example/ERIS.zip">http://dead.example/ERIS.zip</A><BR><a href="http://up.example/ERIS.zip">http://up.example/ERIS.zip</A><BR>（下はミラーです）</td>"#;
        let links = venue_links(page);
        assert_eq!(links.len(), 2);
        assert_eq!(
            pick(&links),
            ["http://dead.example/ERIS.zip", "http://up.example/ERIS.zip"]
        );
    }

    #[test]
    fn indexed_links_then_mirrors() {
        use crate::hosts::{Context, Resolver};
        use crate::net::{run, Canned};
        // not mirrored: the index's checked links, the work's page last
        let catalog: &[u8] =
            br#"[{"venue":146,"mirror":false,"index":"https://raw.example/x/index.json"}]"#;
        let index: &[u8] = br#"[{"num":94,"files":[],"links":["https://dead.example/a.zip","https://up.example/a.zip"]}]"#;
        let http = Canned(vec![
            (CATALOG.into(), Ok(catalog.to_vec())),
            (
                "https://raw.example/x/index.json".into(),
                Ok(index.to_vec()),
            ),
        ]);
        let mut r = Resolver::new(LINK, None, &Context::default());
        assert_eq!(
            run(&mut r, &http).unwrap().url,
            "https://dead.example/a.zip"
        );
        // its file did not come: the next
        assert!(r.try_next());
        assert_eq!(run(&mut r, &http).unwrap().url, "https://up.example/a.zip");
        // then the work's page, which gives nothing here
        assert!(r.try_next());
        assert!(run(&mut r, &http).is_err());
        assert!(!r.try_next());
    }

    #[test]
    fn bmssearch_venues() {
        let manifest = r#"{"version":1,"resources":[
            {"url":"https://drive.google.com/file/d/W/view","type":"core","name":"wav版"},
            {"url":"https://drive.google.com/file/d/O/view","type":"core","name":"ogg版"}]}"#
            .as_bytes();
        let t = resolve(
            "https://venue.bmssearch.net/bmsshuin3/75",
            &[(
                "https://venue.bmssearch.net/bmsshuin3/75/manifest",
                manifest,
            )],
        );
        assert!(t.unwrap().url.contains("id=O"));
        // the venue's list of works is no work
        assert_eq!(
            resolve("https://venue.bmssearch.net/bmsshuin3", &[]),
            Ok(Target::url("https://venue.bmssearch.net/bmsshuin3"))
        );
    }
}
