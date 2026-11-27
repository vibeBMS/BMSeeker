//! The BMS event mirror's files (vibeBMS/BMS): its list of events
//! (`events.json`, [`Config::mirror_catalog`]) and each event's index of
//! its works (`index.json`), which hold the mirrored archives or, for an
//! event that is not mirrored, the authors' links as checked. The hosts
//! resolve a manbow work through them; a player shows them.
//!
//! Both are read as their schema says: an event, a work and a file are
//! each an object whose fields are of their types and given once (a
//! field may be missing; one not named here is passed by). A list keeps
//! its other events when one does not read; an index is one file made at
//! once, so one work that does not read refuses it.
//!
//! [`Config::mirror_catalog`]: crate::config::Config::mirror_catalog

use serde::de::value::MapAccessDeserializer;
use serde::de::{DeserializeOwned, Deserializer, MapAccess, SeqAccess, Visitor};
use serde::{Deserialize, Serialize};
use std::marker::PhantomData;

/// The bytes a list or an index may have: an API's answer's
/// ([`Kind::Api`](crate::net::Kind::Api)), as the hosts fetch them. An
/// index of six hundred works is a quarter of a megabyte.
pub const MAX_BYTES: usize = crate::net::Kind::Api.limit();

/// An event in the list (`events.json`).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Event {
    /// Its release's tag ("boftt"), in its index's URL.
    pub tag: String,
    /// The name to show ("BOF:TT").
    pub name: String,
    /// "THE BMS OF FIGHTERS : TT - …"
    pub title: String,
    /// manbow's number for it (`event=` in a work's page), if it was held
    /// there.
    pub venue: Option<u32>,
    /// Its archives are mirrored (else its works give their authors' links).
    pub mirror: bool,
    /// Its index's URL ("": none yet, nothing to read of it).
    pub index: String,
}

/// A work in an event's index (`index.json`).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Work {
    /// Its number on the venue (`num=` in its page): the one field a work
    /// must have.
    pub num: u32,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub artist: String,
    #[serde(default)]
    pub team: String,
    /// Its genre on the venue ("Original").
    #[serde(default)]
    pub kind: String,
    /// Its page on the venue.
    #[serde(default)]
    pub page: String,
    /// The mirrored archives: the main one first ("042.zip"), then
    /// add-ons ("042+1.rar").
    #[serde(default, deserialize_with = "objects")]
    pub files: Vec<File>,
    /// Its authors' download links (a work the mirror has no copy of), in
    /// the order to try them: those found to bring the song first.
    #[serde(default)]
    pub links: Vec<String>,
    /// Links found to hold add-ons for the song (charts without their
    /// sounds: 差分), for its folder.
    #[serde(default)]
    pub addons: Vec<String>,
}

/// A mirrored archive of a work.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct File {
    /// Its name in the mirror ("042.zip").
    pub name: String,
    pub url: String,
    /// Its size in bytes (0: not said).
    pub size: u64,
    /// The file name its author published ("song_ogg.zip"; another
    /// party's text, to be made a plain name before it names a file).
    pub original: String,
}

/// The events of a list (`events.json`): a JSON array of at most
/// [`MAX_BYTES`], else why not. An entry that does not read as an
/// [`Event`] is left out, not the whole list; one without an index is
/// kept.
pub fn parse_catalog(bytes: &[u8]) -> Result<Vec<Event>, String> {
    let read: Readable<Event> =
        serde_json::from_slice(within(bytes)?).map_err(|e| e.to_string())?;
    Ok(read.0)
}

/// The works of an event's index (`index.json`): a JSON array of at most
/// [`MAX_BYTES`] whose every element reads as a [`Work`], else why not.
pub fn parse_index(bytes: &[u8]) -> Result<Vec<Work>, String> {
    let mut d = serde_json::Deserializer::from_slice(within(bytes)?);
    let works = objects(&mut d).and_then(|w| d.end().map(|()| w));
    works.map_err(|e| e.to_string())
}

fn within(bytes: &[u8]) -> Result<&[u8], String> {
    match bytes.len() {
        n if n > MAX_BYTES => Err(format!("{n} bytes, more than the {MAX_BYTES} it may have")),
        _ => Ok(bytes),
    }
}

/// A list's elements that read as `T`, each read on its own from its text
/// (as `T` reads a file: a key given twice is no `T`).
struct Readable<T>(Vec<T>);

impl<'de, T: DeserializeOwned> Deserialize<'de> for Readable<T> {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Readable<T>, D::Error> {
        struct V<T>(PhantomData<T>);
        impl<'de, T: DeserializeOwned> Visitor<'de> for V<T> {
            type Value = Readable<T>;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("a list")
            }
            fn visit_seq<A: SeqAccess<'de>>(self, mut a: A) -> Result<Readable<T>, A::Error> {
                let mut out = Vec::new();
                while let Some(raw) = a.next_element::<&'de serde_json::value::RawValue>()? {
                    out.extend(serde_json::from_str::<Obj<T>>(raw.get()).map(|o| o.0).ok());
                }
                Ok(Readable(out))
            }
        }
        d.deserialize_seq(V(PhantomData))
    }
}

/// A `T` given as an object (serde's derive would take a list of its
/// fields' values too).
struct Obj<T>(T);

impl<'de, T: Deserialize<'de>> Deserialize<'de> for Obj<T> {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Obj<T>, D::Error> {
        struct V<T>(PhantomData<T>);
        impl<'de, T: Deserialize<'de>> Visitor<'de> for V<T> {
            type Value = Obj<T>;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("an object")
            }
            fn visit_map<A: MapAccess<'de>>(self, a: A) -> Result<Obj<T>, A::Error> {
                T::deserialize(MapAccessDeserializer::new(a)).map(Obj)
            }
        }
        d.deserialize_map(V(PhantomData))
    }
}

/// A list of objects, each a `T`.
fn objects<'de, D: Deserializer<'de>, T: Deserialize<'de>>(d: D) -> Result<Vec<T>, D::Error> {
    let all = Vec::<Obj<T>>::deserialize(d)?;
    Ok(all.into_iter().map(|o| o.0).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Events not of BOF: names in Japanese, no venue or a null one, no
    /// index; an entry that does not read is left out, the rest kept.
    #[test]
    fn lists() {
        let j = r#"[{"tag": "mumei2026", "name": "無名戦&A-1 2026",
            "title": "自称無名BMS作家が物申す！ & A-1 ClimaX 2026", "venue": 151,
            "date": "2026-05-01", "mirror": false, "index": "https://x/mumei2026/index.json"},
            {"tag": "dpbms2014", "name": "DPBMS差分大会", "venue": null, "mirror": true,
            "index": "https://x/dpbms2014/index.json"},
            {"tag": "pabat", "title": "P", "index": "https://x/pabat/index.json"},
            {"tag": "bad", "name": null, "index": "https://x/bad/index.json"},
            {"tag": "bad2", "venue": "151", "index": "https://x/bad2/index.json"},
            {"tag": "bad3", "venue": -1}, {"tag": "bad4", "mirror": 1},
            {"tag": "twice", "tag": "again"}, 7, null, ["x"],
            {"tag": "noindex", "name": "N"}]"#;
        let events = parse_catalog(j.as_bytes()).unwrap();
        let tags: Vec<&str> = events.iter().map(|e| e.tag.as_str()).collect();
        assert_eq!(tags, ["mumei2026", "dpbms2014", "pabat", "noindex"]);
        assert_eq!(events[0].venue, Some(151));
        assert_eq!(events[0].name, "無名戦&A-1 2026");
        assert!(!events[0].mirror && events[1].mirror);
        assert_eq!(events[1].venue, None);
        assert_eq!(events[3].index, "");
        for not_a_list in [&b"{}"[..], b"", b"[{\"tag\":\"a\"}", b"[] x", b"null"] {
            assert!(parse_catalog(not_a_list).is_err());
        }
        assert_eq!(parse_catalog(b"[]"), Ok(Vec::new()));
    }

    #[test]
    fn indexes() {
        let j = r#"[{"num": 42, "title": "T", "artist": "A", "team": "X", "kind": "Original",
            "page": "p", "files": [{"name": "042.zip", "url": "u", "size": 3, "original": "t.zip"}]},
            {"num": 21, "title": "U", "files": [], "links": ["https://1drv.ms/x"],
            "addons": ["https://a/b.zip"], "other": {"x": 1}}]"#;
        let w = parse_index(j.as_bytes()).unwrap();
        assert_eq!(w[0].num, 42);
        assert_eq!(
            w[0].files,
            [File {
                name: "042.zip".into(),
                url: "u".into(),
                size: 3,
                original: "t.zip".into()
            }]
        );
        assert_eq!(w[1].links, ["https://1drv.ms/x"]);
        assert_eq!(w[1].addons, ["https://a/b.zip"]);
        assert_eq!(w[1].artist, "");
        // what a player keeps of it reads back the same
        let kept = serde_json::to_vec(&w).unwrap();
        assert_eq!(parse_index(&kept), Ok(w));
        // one work that does not read: none
        for bad in [
            r#"[{"num": 1}, {"title": "no number"}]"#,
            r#"[{"num": 1}, {"num": 4294967296}]"#,
            r#"[{"num": -1}]"#,
            r#"[{"num": "1"}]"#,
            r#"[{"num": 1, "title": null}]"#,
            r#"[{"num": 1, "files": [{"name": "a.zip", "size": -3}]}]"#,
            r#"[{"num": 1, "files": [7]}]"#,
            r#"[{"num": 1, "files": [["a.zip", "https://a/a.zip", 3, "a.zip"]]}]"#,
            r#"[{"num": 1, "files": {"name": "a.zip"}}]"#,
            r#"[[1]]"#,
            r#"[{"num": 1, "links": ["https://a", 2]}]"#,
            r#"[{"num": 1, "num": 2}]"#,
            r#"[{"num": 1}, 7]"#,
            r#"{"num": 1}"#,
        ] {
            assert!(parse_index(bad.as_bytes()).is_err(), "{bad}");
        }
    }

    #[test]
    fn sizes() {
        let mut big = b"[{\"num\":1,\"title\":\"".to_vec();
        big.resize(MAX_BYTES - 3, b'a');
        big.extend(b"\"}]");
        assert_eq!(parse_index(&big).unwrap().len(), 1);
        big.insert(1, b' ');
        assert!(parse_index(&big).is_err());
        assert!(parse_catalog(&big).is_err());
    }
}
