//! Google Drive: a file downloads from its id (`confirm=t` skips the virus
//! scan page of big files); a folder's listing (`embeddedfolderview`)
//! names its archives, the best of which is taken. A folder link ending in
//! `#sub/folder/file` names goes down to that file.

use super::{archive_name, between, get_then, pick_file, Host, Link, Resolved, Target, Want};
use crate::config::Config;
use crate::net::{Answer, BoxTask, Next, Ready, Request, Task};

pub struct Drive;

impl Host for Drive {
    fn name(&self) -> &'static str {
        "drive"
    }

    fn accepts(&self, link: &Link) -> bool {
        link.on("drive.google.com") || link.host == "docs.google.com"
    }

    fn open(&self, link: &Link, _: Option<&Want>, _: &Config) -> BoxTask<Resolved> {
        let Some(id) = drive_id(&link.url) else {
            return Ready::boxed(Resolved::Page);
        };
        if !link.lower.contains("/folders/") {
            return Ready::boxed(Resolved::File(Target::url(file_url(&id))));
        }
        if let Some((_, path)) = link.url.split_once('#') {
            let names: Vec<String> = path
                .split('/')
                .filter(|n| !n.is_empty())
                .map(crate::net::percent_decode)
                .collect();
            if !names.is_empty() {
                return Box::new(Walk { folder: id, names });
            }
        }
        get_then(
            format!("https://drive.google.com/embeddedfolderview?id={id}"),
            |body| {
                let page = String::from_utf8_lossy(&body);
                let files = folder_entries(&page);
                let best = pick_file(
                    files
                        .iter()
                        .filter(|(name, _)| archive_name(name))
                        .map(|(n, i)| (n.as_str(), (n, i))),
                );
                match best {
                    Some((name, id)) => {
                        Resolved::File(Target::named(file_url(id), Some(name.clone())))
                    }
                    None => Resolved::Page,
                }
            },
        )
    }
}

/// The file or folder id of a Google Drive link (`/file/d/ID`,
/// `/folders/ID`, `?id=ID`).
pub fn drive_id(u: &str) -> Option<String> {
    let rest = ["/d/", "/folders/", "id="]
        .iter()
        .find_map(|k| u.find(k).map(|i| &u[i + k.len()..]))?;
    let id: String = rest
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
        .collect();
    (!id.is_empty()).then_some(id)
}

fn file_url(id: &str) -> String {
    format!("https://drive.usercontent.google.com/download?id={id}&export=download&confirm=t")
}

/// (name, file id) of the files in an `embeddedfolderview` page.
fn folder_entries(page: &str) -> Vec<(String, String)> {
    listing(page)
        .into_iter()
        .filter(|e| !e.folder)
        .map(|e| (e.name, e.id))
        .collect()
}

/// A file or folder in a folder's listing.
#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    pub name: String,
    pub id: String,
    pub folder: bool,
}

/// What an `embeddedfolderview` page lists.
pub fn listing(page: &str) -> Vec<Entry> {
    let mut out = Vec::new();
    for part in page.split("class=\"flip-entry\"").skip(1) {
        let (id, folder) = match between(part, "/file/d/", "/") {
            Some(id) => (id, false),
            None => match between(part, "/folders/", "\"") {
                Some(id) => (id.split(['?', '&']).next().unwrap_or(id), true),
                None => continue,
            },
        };
        if let Some(name) = between(part, "flip-entry-title\">", "<") {
            out.push(Entry {
                name: crate::text::unescape_attr(name),
                id: id.to_string(),
                folder,
            });
        }
    }
    out
}

/// A folder's listing page.
pub fn listing_url(id: &str) -> String {
    format!("https://drive.google.com/embeddedfolderview?id={id}")
}

/// A Drive file's link.
pub fn file_link(id: &str) -> String {
    format!("https://drive.google.com/file/d/{id}/view")
}

/// Down a folder's sub-folders by name to a file.
struct Walk {
    folder: String,
    names: Vec<String>,
}

impl Task for Walk {
    type Output = Resolved;

    fn start(&mut self) -> Next<Resolved> {
        Next::Ask(Request::Get(listing_url(&self.folder)))
    }

    fn feed(&mut self, answer: Answer) -> Next<Resolved> {
        let body = match answer {
            Ok(b) => b,
            Err(e) => return Next::Done(Resolved::Failed(e)),
        };
        let page = String::from_utf8_lossy(&body);
        let name = self.names.remove(0);
        let last = self.names.is_empty();
        match listing(&page)
            .into_iter()
            .find(|e| e.name == name && e.folder != last)
        {
            Some(e) if last => {
                Next::Done(Resolved::File(Target::named(file_url(&e.id), Some(e.name))))
            }
            Some(e) => {
                self.folder = e.id;
                self.start()
            }
            None => Next::Done(Resolved::Failed(
                format!("{name}: not in the folder").into(),
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hosts::tests::resolve;

    #[test]
    fn down_the_folders() {
        let root = r#"<div class="flip-entry" id="entry-F1"><a href="https://drive.google.com/drive/folders/F36"><div class="flip-entry-title">36xx</div></a></div>"#;
        let sub = r#"<div class="flip-entry"><a href="https://drive.google.com/file/d/Z1/view?usp=drive_web"><div class="flip-entry-title">03691.zip</div></a></div>"#;
        assert_eq!(
            resolve(
                "https://drive.google.com/drive/folders/ROOT#36xx/03691.zip",
                &[
                    (
                        "https://drive.google.com/embeddedfolderview?id=ROOT",
                        root.as_bytes()
                    ),
                    (
                        "https://drive.google.com/embeddedfolderview?id=F36",
                        sub.as_bytes()
                    ),
                ]
            ),
            Ok(Target::named(file_url("Z1"), Some("03691.zip".into())))
        );
    }

    #[test]
    fn ids() {
        assert_eq!(
            drive_id("https://drive.google.com/file/d/1AbC-d_9/view?usp=sharing").as_deref(),
            Some("1AbC-d_9")
        );
        assert_eq!(
            drive_id("https://drive.google.com/drive/folders/1UX?usp=x").as_deref(),
            Some("1UX")
        );
        assert_eq!(
            drive_id("https://drive.google.com/open?id=XYZ").as_deref(),
            Some("XYZ")
        );
    }

    #[test]
    fn files_and_folders() {
        let t = resolve("https://drive.google.com/file/d/X/view", &[]).unwrap();
        assert!(t.url.contains("id=X"));
        let page = br#"<div class="flip-entry" id="entry-A"><a href="https://drive.google.com/file/d/A/view?usp=drive_web"><div class="flip-entry-title">song.zip</div></a></div>
            <div class="flip-entry" id="entry-S"><a href="https://drive.google.com/drive/folders/S"><div class="flip-entry-title">sub</div></a></div>
            <div class="flip-entry" id="entry-B"><a href="https://drive.google.com/file/d/B/view?usp=drive_web"><div class="flip-entry-title">song_ogg.zip</div></a></div>
            <div class="flip-entry" id="entry-C"><a href="https://drive.google.com/file/d/C/view?usp=drive_web"><div class="flip-entry-title">song_wav.zip</div></a></div>"#;
        assert_eq!(folder_entries(&String::from_utf8_lossy(page)).len(), 3);
        let t = resolve(
            "https://drive.google.com/drive/folders/F",
            &[("https://drive.google.com/embeddedfolderview?id=F", page)],
        )
        .unwrap();
        assert!(t.url.contains("id=B"));
        assert_eq!(t.name.as_deref(), Some("song_ogg.zip"));
    }
}
