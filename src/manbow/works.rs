//! An event's works, from its URL list and its work list.

use super::{links_of, pick, text, Pick};

/// A work of an event.
#[derive(Clone, Debug, PartialEq)]
pub struct Work {
    pub num: u32,
    pub title: String,
    pub artist: String,
    pub team: String,
    /// "Original", "Arrange", "Disq" (disqualified)... from the work list.
    pub kind: String,
    pub size_kb: Option<u64>,
    /// The work's page on the venue.
    pub page: String,
    pub links: Vec<(String, String)>,
    pub pick: Pick,
}

/// The event's URL list (every work and its links).
pub fn url_list_url(base: &str, event: u32) -> String {
    format!("{base}?action=URLList&event={event}&start=0&end=9999")
}

/// The event's work list (each work's kind).
pub fn work_list_url(base: &str, event: u32) -> String {
    format!("{base}?action=List_def&event={event}")
}

/// A manbow page's text (Shift_JIS).
pub fn decode(page: &[u8]) -> String {
    encoding_rs::SHIFT_JIS.decode(page).0.into_owned()
}

/// What each `<tag>…</tag>` of `s` holds, in order (`spaces`: `<tag >`
/// too).
fn elements<'a>(s: &'a str, tag: &str, spaces: bool) -> Vec<&'a str> {
    let (open, close) = (format!("<{tag}"), format!("</{tag}>"));
    let mut out = Vec::new();
    let mut rest = s;
    while let Some(at) = rest.find(&open) {
        rest = &rest[at + open.len()..];
        let inner = match spaces {
            true => rest.trim_start(),
            false => rest,
        };
        let Some(inner) = inner.strip_prefix('>') else {
            continue;
        };
        let Some(end) = inner.find(&close) else {
            break;
        };
        out.push(&inner[..end]);
        rest = &inner[end + close.len()..];
    }
    out
}

/// The size a work list gives (`123 KB`), in kilobytes.
fn kilobytes(s: &str) -> Option<u64> {
    let digits = s.find(|c: char| !c.is_ascii_digit()).unwrap_or(s.len());
    s[digits..]
        .trim_start()
        .starts_with("KB")
        .then(|| s[..digits].parse().ok())?
}

/// The works of event `event` from its URL list and work list (`base`:
/// the venue's `event.cgi`, [`Config::manbow`](crate::config::Config::manbow)).
pub fn works(base: &str, event: u32, url_list: &str, work_list: &str) -> Vec<Work> {
    // "No.12 / Original"
    let kinds: std::collections::HashMap<u32, String> = work_list
        .match_indices("No.")
        .filter_map(|(at, _)| {
            let rest = &work_list[at + 3..];
            let (num, rest) = rest.split_at(rest.find(|c: char| !c.is_ascii_digit())?);
            let rest = rest.strip_prefix(" / ")?;
            let kind = &rest[..rest
                .find(|c: char| !crate::text::pattern::word(c))
                .unwrap_or(rest.len())];
            (!kind.is_empty()).then(|| Some((num.parse().ok()?, kind.to_string())))?
        })
        .collect();
    let mut out = Vec::new();
    for row in elements(url_list, "tr", true) {
        let mut tds = elements(row, "td", false);
        if tds.len() == 5 {
            // events up to 2005 have no team column
            tds.insert(2, "");
        }
        if tds.len() < 6 {
            continue;
        }
        let Ok(num) = text(tds[0]).parse::<u32>() else {
            continue;
        };
        let links = links_of(tds[5]);
        out.push(Work {
            num,
            title: text(tds[3]),
            artist: text(tds[1]),
            team: text(tds[2]),
            kind: kinds.get(&num).cloned().unwrap_or_default(),
            size_kb: kilobytes(&text(tds[4])),
            page: format!("{base}?action=More_def&num={num}&event={event}"),
            pick: pick(&links),
            links,
        });
    }
    out.sort_by_key(|w| w.num);
    out
}
