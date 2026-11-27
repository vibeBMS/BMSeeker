//! Other links for a table's songs, by the chart's MD5: where a table's
//! own link is missing, dead, or does not download (a page, a host that
//! needs a browser), the same chart may have a link elsewhere: in another
//! table the player reads (発狂BMS and Satellite list some of the same
//! charts), or in the Stella uploader's list ([`Config::stella_list`]).
//!
//! [`Config::stella_list`]: crate::config::Config::stella_list
//!
//! [`Links::fill`] fills a link only where the table has none, or one on a
//! host known to be gone (`is_dead`); [`Links::song_links`] /
//! [`Links::chart_links`] give every link known for a chart, the table's own
//! first (one to a dead uploader last: the uploader's mirrors may have its
//! file), to try one after another.

use crate::chart::{is_chart, md5_key};
use crate::table::{DifficultyTable, TableEntry};
use std::collections::HashMap;

/// The links kept for one chart: the sources give a few, and a list that
/// gives one MD5 a link in each of its thousands of rows is looked through
/// no further.
const PER_CHART: usize = 64;

/// Hosts that are gone (their links are filled as if empty).
const DEAD: &[&str] = &["gnqg.rosx.net", "absolute.pv.land.to"];

/// A link to a host known to be gone (also through web.archive.org).
pub(crate) fn is_dead(url: &str) -> bool {
    let u = url.to_ascii_lowercase();
    DEAD.iter().any(|d| u.contains(d))
}

fn usable(url: &str) -> bool {
    let u = url.trim();
    (u.starts_with("https://") || u.starts_with("http://")) && !is_dead(u)
}

/// A link straight to a dead uploader (not a copy of it elsewhere, as on
/// web.archive.org).
fn to_dead_uploader(url: &str) -> bool {
    let u = url.to_ascii_lowercase();
    let Some(rest) = u.strip_prefix("http://").or(u.strip_prefix("https://")) else {
        return false;
    };
    let host = rest.split(['/', '?', '#', ':']).next().unwrap_or("");
    DEAD.iter().any(|d| {
        host.strip_suffix(d)
            .is_some_and(|r| r.is_empty() || r.ends_with('.'))
    })
}

/// MD5 → the (song's link, chart's link) pairs known for it, in the order
/// the sources were given.
#[derive(Clone, Debug, Default)]
pub struct Links {
    map: HashMap<String, Vec<(String, String)>>,
}

impl Links {
    /// How many charts have links.
    pub fn len(&self) -> usize {
        self.map.len()
    }

    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    fn add(&mut self, md5: &str, url: &str, url_diff: &str) {
        let Some(k) = md5_key(md5) else { return };
        let keep = |u: &str| {
            if usable(u) {
                u.trim().to_string()
            } else {
                String::new()
            }
        };
        let pair = (keep(url), keep(url_diff));
        if pair.0.is_empty() && pair.1.is_empty() {
            return;
        }
        let known = self.map.entry(k).or_default();
        if known.len() < PER_CHART && !known.contains(&pair) {
            known.push(pair);
        }
    }

    /// The links the tables give their charts.
    pub fn from_tables<'a>(tables: impl IntoIterator<Item = &'a DifficultyTable>) -> Links {
        let mut out = Links::default();
        for t in tables {
            for e in &t.entries {
                out.add(&e.md5, &e.url, &e.url_diff);
            }
        }
        out
    }

    /// A list of `md5 <TAB> url_diff <TAB> url` rows after a header row
    /// (one a player keeps, or trusts).
    pub fn from_tsv(text: &str) -> Links {
        let mut out = Links::default();
        for line in text.lines().skip(1) {
            let mut col = line.split('\t');
            let md5 = col.next().unwrap_or("");
            let url_diff = col.next().unwrap_or("");
            let url = col.next().unwrap_or("");
            out.add(md5, url, url_diff);
        }
        out
    }

    /// The Stella uploader's list's format (an array of `{md5, url, url_diff, ...}`),
    /// each row taken as it is read.
    pub fn from_stella(json: &[u8]) -> Links {
        use serde::de::{Deserializer, SeqAccess, Visitor};
        crate::json::last_wins! {
            struct Row {
                md5: String = crate::json::text,
                url: String = crate::json::text,
                url_diff: String = crate::json::text,
            }
        }
        struct Rows<'a>(&'a mut Links);
        impl<'de> Visitor<'de> for Rows<'_> {
            type Value = ();
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("a list of rows")
            }
            fn visit_seq<A: SeqAccess<'de>>(self, mut s: A) -> Result<(), A::Error> {
                while let Some(row) = s.next_element::<crate::json::Object<Row>>()? {
                    if let Some(r) = row.0 {
                        self.0.add(&r.md5, &r.url, &r.url_diff);
                    }
                }
                Ok(())
            }
        }
        let mut out = Links::default();
        // (what is no list of rows, or ends as none, gives none)
        let mut d = serde_json::Deserializer::from_slice(json);
        if d.deserialize_seq(Rows(&mut out))
            .and_then(|()| d.end())
            .is_err()
        {
            return Links::default();
        }
        out
    }

    /// Both, `self`'s links first.
    pub fn and(mut self, other: Links) -> Links {
        for (k, v) in other.map {
            let known = self.map.entry(k).or_default();
            for pair in v {
                if known.len() < PER_CHART && !known.contains(&pair) {
                    known.push(pair);
                }
            }
        }
        self
    }

    fn known(&self, md5: &str) -> &[(String, String)] {
        md5_key(md5)
            .and_then(|k| self.map.get(&k))
            .map_or(&[], |v| v.as_slice())
    }

    /// `own` (if usable), then the other links in `pick` of the chart's
    /// pairs in each of `lists` in turn (a link once), then `own` if it is
    /// to a dead uploader (the uploader's mirrors may have its file:
    /// `hosts::uploader`).
    fn all(
        lists: &[&Links],
        own: &str,
        md5: &str,
        pick: fn(&(String, String)) -> &String,
    ) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        let own = own.trim();
        if usable(own) {
            out.push(own.to_string());
        }
        for p in lists.iter().flat_map(|l| l.known(md5)) {
            let u = pick(p);
            if !u.is_empty() && !out.contains(u) {
                out.push(u.clone());
            }
        }
        if to_dead_uploader(own) {
            out.push(own.to_string());
        }
        out
    }

    /// Every link known for `e`'s song, its own first.
    pub fn song_links(&self, e: &TableEntry) -> Vec<String> {
        Links::song_links_in(&[self], e)
    }

    /// Every link known for `e`'s separate chart, its own first.
    pub fn chart_links(&self, e: &TableEntry) -> Vec<String> {
        Links::chart_links_in(&[self], e)
    }

    /// Every link `lists` know for `e`'s song, its own first, then each
    /// list's in turn: what [`song_links`](Links::song_links) of the lists
    /// made one with [`and`](Links::and) gives, without making it (a
    /// player keeps the tables' links and a large list apart).
    pub fn song_links_in(lists: &[&Links], e: &TableEntry) -> Vec<String> {
        Links::all(lists, &e.url, &e.md5, |p| &p.0)
    }

    /// Every link `lists` know for `e`'s separate chart, as
    /// [`song_links_in`](Links::song_links_in).
    pub fn chart_links_in(lists: &[&Links], e: &TableEntry) -> Vec<String> {
        Links::all(lists, &e.url_diff, &e.md5, |p| &p.1)
    }

    /// `e`'s links, filled where missing or dead: (url, url_diff).
    pub(crate) fn of(&self, e: &TableEntry) -> (String, String) {
        let pick = |own: &str, other: Vec<String>| {
            if usable(own) {
                own.to_string()
            } else {
                other.into_iter().next().unwrap_or_else(|| own.to_string())
            }
        };
        (
            pick(&e.url, self.song_links(e)),
            pick(&e.url_diff, self.chart_links(e)),
        )
    }

    /// Fill `entries`' missing or dead links. Returns how many changed.
    pub fn fill(&self, entries: &mut [TableEntry]) -> usize {
        let mut n = 0;
        for e in entries {
            let (url, url_diff) = self.of(e);
            if url != e.url || url_diff != e.url_diff {
                e.url = url;
                e.url_diff = url_diff;
                n += 1;
            }
        }
        n
    }
}

/// A link to a chart file of its own (`….bms` and the like).
pub(crate) fn chart_link(u: &str) -> bool {
    let path = u.split(['?', '#']).next().unwrap_or("");
    is_chart(std::path::Path::new(path))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fills_missing_and_dead_links() {
        let tsv = "md5\turl_diff\turl\n\
            75DC1FF763E736BA307FBC39EC9A386B\thttps://m.example/00001.zip\t\n\
            997c5d2e1756d22793b287fe72a9b184\thttps://m.example/00003.zip\thttp://manbow.nothing.sh/x\n\
            bad\thttps://x\thttps://y\n";
        let stella = br#"[{"md5":"997c5d2e1756d22793b287fe72a9b184","url":"https://other/","url_diff":""},
            {"md5":"0123456789abcdef0123456789abcdef","url":"https://s.example/song.zip","url_diff":"https://stellabms.xyz/upload/1"}]"#;
        let links = Links::from_tsv(tsv).and(Links::from_stella(stella));
        assert_eq!(links.len(), 3);
        assert_eq!(links.known("997C5D2E1756D22793B287FE72A9B184").len(), 2);
        let entry = |md5: &str, url: &str, url_diff: &str| TableEntry {
            md5: md5.into(),
            url: url.into(),
            url_diff: url_diff.into(),
            ..Default::default()
        };
        // a dead 差分 link, no song link
        let e = entry(
            "75dc1ff763e736ba307fbc39ec9a386b",
            "",
            "http://gnqg.rosx.net/upload/up/1.zip",
        );
        assert_eq!(
            links.of(&e),
            (String::new(), "https://m.example/00001.zip".into())
        );
        // the table's own links stay; the TSV wins over Stella
        let e = entry("997c5d2e1756d22793b287fe72a9b184", "https://own/", "");
        assert_eq!(
            links.of(&e),
            ("https://own/".into(), "https://m.example/00003.zip".into())
        );
        let mut es = vec![
            entry("0123456789ABCDEF0123456789ABCDEF", "", ""),
            entry("ffffffffffffffffffffffffffffffff", "", ""),
        ];
        assert_eq!(links.fill(&mut es), 1);
        assert_eq!(es[0].url, "https://s.example/song.zip");
        assert!(is_dead(
            "https://web.archive.org/web/2019/http://absolute.pv.land.to/x"
        ));
    }

    /// A Stella row with a key twice: its last value.
    #[test]
    fn a_stella_key_twice() {
        let stella = br#"[{"md5":"0123456789abcdef0123456789abcdef","url":"https://a/","url":"https://b/"}]"#;
        let links = Links::from_stella(stella);
        assert_eq!(links.len(), 1);
        let e = TableEntry {
            md5: "0123456789abcdef0123456789abcdef".into(),
            ..Default::default()
        };
        assert_eq!(links.of(&e).0, "https://b/");
    }

    /// A table's link to a dead uploader is tried last (its mirrors may
    /// have the file); filling still takes another link for it.
    #[test]
    fn dead_uploaders_links_last() {
        let md5 = "75dc1ff763e736ba307fbc39ec9a386b";
        let tsv = format!("md5\turl_diff\turl\n{md5}\t\thttps://m.example/1.zip\n");
        let links = Links::from_tsv(&tsv);
        let gnqg = "http://gnqg.rosx.net/upload/upload.cgi?get=03691";
        let e = TableEntry {
            md5: md5.into(),
            url: gnqg.into(),
            ..Default::default()
        };
        assert_eq!(links.song_links(&e), ["https://m.example/1.zip", gnqg]);
        assert_eq!(Links::default().song_links(&e), [gnqg]);
        assert_eq!(links.of(&e).0, "https://m.example/1.zip");
        assert_eq!(Links::default().of(&e).0, gnqg);
        // a copy of it elsewhere is not the uploader's
        let archived = TableEntry {
            url: format!("https://web.archive.org/web/2019/{gnqg}"),
            ..e
        };
        assert!(Links::default().song_links(&archived).is_empty());
        assert!(to_dead_uploader("https://www.gnqg.rosx.net/a"));
        assert!(!to_dead_uploader("https://notgnqg.rosx.net.example/a"));
    }

    /// Looked up in several lists, the links are those of the lists made
    /// one ([`Links::and`]), the first list's first.
    #[test]
    fn links_in_several_lists() {
        let (a, b, c) = ("a".repeat(32), "b".repeat(32), "c".repeat(32));
        let e = |md5: &str, url: &str, url_diff: &str| TableEntry {
            md5: md5.into(),
            url: url.into(),
            url_diff: url_diff.into(),
            ..Default::default()
        };
        let table = DifficultyTable {
            entries: vec![
                e(&a, "https://t/a1.zip", ""),
                e(&a, "https://t/a2.zip", "https://t/a2.bms"),
                e(&b, "", "https://t/b.bms"),
            ],
            ..Default::default()
        };
        let tables = Links::from_tables([&table]);
        let stella = Links::from_stella(
            format!(
                r#"[{{"md5":"{a}","url":"https://s/a.zip","url_diff":""}},
                    {{"md5":"{a}","url":"https://t/a1.zip","url_diff":"https://s/a.bms"}},
                    {{"md5":"{c}","url":"https://s/c.zip","url_diff":"https://s/c.bms"}}]"#
            )
            .as_bytes(),
        );
        let one = tables.clone().and(stella.clone());
        let lists = [&tables, &stella];
        let dead = "http://gnqg.rosx.net/upload/upload.cgi?get=1";
        for md5 in [&a, &b, &c, &"d".repeat(32)] {
            for own in ["", "https://own/x.zip", dead, "not a link"] {
                for x in [e(md5, own, ""), e(md5, "", own)] {
                    assert_eq!(Links::song_links_in(&lists, &x), one.song_links(&x));
                    assert_eq!(Links::chart_links_in(&lists, &x), one.chart_links(&x));
                }
            }
        }
        let x = e(&a, "https://own/x.zip", "");
        assert_eq!(
            Links::song_links_in(&lists, &x),
            [
                "https://own/x.zip",
                "https://t/a1.zip",
                "https://t/a2.zip",
                "https://s/a.zip"
            ]
        );
        assert_eq!(
            Links::chart_links_in(&[&stella, &tables], &x),
            ["https://s/a.bms", "https://t/a2.bms"]
        );
        assert_eq!(Links::song_links_in(&[], &x), ["https://own/x.zip"]);
    }
}
