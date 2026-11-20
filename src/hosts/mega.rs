//! MEGA: a file link's download URL comes from the API, a folder link's
//! files (and their names) from its listing, decrypted with the link's key;
//! a folder of many songs' archives gives the one for the song looked for.
//! The bytes come encrypted ([`MegaCipher`]).

use super::{pick_file, Host, Link, Resolved, Target, Want};
use crate::config::Config;
use crate::net::{Answer, BoxTask, Kind, NetError, Next, Ready, Request, Task};
use crate::unpack::is_archive;
use std::path::Path;

pub(crate) struct Mega;

impl Host for Mega {
    fn name(&self) -> &'static str {
        "mega"
    }

    fn accepts(&self, link: &Link) -> bool {
        link.host == "mega.nz" || link.host == "mega.co.nz" || link.host.ends_with(".mega.nz")
    }

    fn open(&self, link: &Link, want: Option<&Want>, config: &Config) -> BoxTask<Resolved> {
        let api = config.mega_api.clone();
        if let Some((handle, key, node)) = mega_folder(&link.url) {
            return Box::new(MegaTask {
                api,
                state: State::Folder {
                    handle,
                    key,
                    node,
                    want: want.cloned(),
                },
            });
        }
        match mega_link(&link.url) {
            Some((handle, key)) => Box::new(MegaTask {
                api,
                state: State::File { handle, key },
            }),
            None => Ready::boxed(Resolved::Page),
        }
    }
}

enum State {
    /// A file link: its download URL.
    File {
        handle: String,
        key: Vec<u8>,
    },
    /// A folder link: its files.
    Folder {
        handle: String,
        key: Vec<u8>,
        node: Option<String>,
        want: Option<Want>,
    },
    /// The download URL of the file picked in a folder.
    Picked {
        key: Vec<u8>,
        name: String,
    },
    Done,
}

struct MegaTask {
    api: String,
    state: State,
}

impl Task for MegaTask {
    type Output = Resolved;

    fn start(&mut self) -> Next<Resolved> {
        match &self.state {
            State::File { handle, .. } => Next::Ask(Request::Post {
                url: self.api.clone(),
                body: mega_request(handle),
                kind: Kind::Api,
            }),
            State::Folder { handle, .. } => Next::Ask(Request::Post {
                url: format!("{}&n={handle}", self.api),
                body: br#"[{"a":"f","c":1,"r":1,"ca":1}]"#.to_vec(),
                kind: Kind::Listing,
            }),
            _ => Next::Done(Resolved::Failed("MEGA: started twice".into())),
        }
    }

    fn feed(&mut self, answer: Answer) -> Next<Resolved> {
        let body = match answer {
            Ok(b) => b,
            Err(e) => return Next::Done(Resolved::Failed(e)),
        };
        match std::mem::replace(&mut self.state, State::Done) {
            State::File { key, .. } => Next::Done(match mega_answer(&body) {
                Ok((url, at)) => {
                    let name = b64url(&at).and_then(|at| mega_attr_name(&at, &key));
                    Resolved::File(Target {
                        mega_key: Some(key),
                        ..Target::named(url, name)
                    })
                }
                Err(e) => Resolved::Failed(e),
            }),
            // the folder's archive, then its download URL
            State::Folder {
                handle,
                key,
                node,
                want,
            } => {
                let files = match mega_nodes(&body, &key) {
                    Ok(f) => f,
                    Err(e) => return Next::Done(Resolved::Failed(e)),
                };
                let Some(f) = pick_mega(&files, node.as_deref(), want.as_ref()) else {
                    return Next::Done(Resolved::Page);
                };
                self.state = State::Picked {
                    key: f.key.clone(),
                    name: f.name.clone(),
                };
                Next::Ask(Request::Post {
                    url: format!("{}&n={handle}", self.api),
                    body: format!(r#"[{{"a":"g","g":1,"ssl":1,"n":"{}"}}]"#, f.handle).into_bytes(),
                    kind: Kind::Api,
                })
            }
            State::Picked { key, name } => Next::Done(match mega_answer(&body) {
                Ok((url, _)) => Resolved::File(Target {
                    mega_key: Some(key),
                    ..Target::named(url, Some(name))
                }),
                Err(e) => Resolved::Failed(e),
            }),
            State::Done => Next::Done(Resolved::Failed("MEGA: nothing asked".into())),
        }
    }
}

/// MEGA's AES-128-CTR with the 32-byte key of a file link, and the check
/// the key carries of what the file is: the file in chunks (128 KiB, then
/// each 128 KiB longer up to 1 MiB), each chunk's CBC-MAC, those MACs'
/// CBC-MAC, folded to the key's last 8 bytes. Without it, whoever serves
/// or carries the bytes can change the file (CTR hides it, no more).
pub struct MegaCipher {
    aes: aes::Aes128,
    ctr: u128,
    /// The key's nonce twice: where a chunk's MAC starts.
    start: [u8; 16],
    /// The key's check.
    check: [u8; 8],
    /// The chunk being read: its MAC so far, the bytes of it read, its
    /// size.
    chunk: ([u8; 16], usize, usize),
    /// The MAC of the chunks' MACs so far.
    mac: [u8; 16],
}

/// A chunk's size at first, and by how much the next is longer (until it
/// is [`CHUNK_MAX`]).
const CHUNK: usize = 128 << 10;
const CHUNK_MAX: usize = 1 << 20;

impl MegaCipher {
    pub fn new(k: &[u8]) -> MegaCipher {
        use aes::cipher::KeyInit;
        let mut key = [0u8; 16];
        for i in 0..16 {
            key[i] = k[i] ^ k[i + 16];
        }
        // the counter starts at the key's nonce (bytes 16..24), then 0
        let mut nonce = [0u8; 16];
        nonce[..8].copy_from_slice(&k[16..24]);
        let mut start = [0u8; 16];
        start[..8].copy_from_slice(&k[16..24]);
        start[8..].copy_from_slice(&k[16..24]);
        let mut check = [0u8; 8];
        check.copy_from_slice(&k[24..32]);
        MegaCipher {
            aes: aes::Aes128::new(&key.into()),
            ctr: u128::from_be_bytes(nonce),
            start,
            check,
            chunk: (start, 0, CHUNK),
            mac: [0; 16],
        }
    }

    /// Decrypt the next part of the file (whole 16-byte blocks but at the
    /// end), taking it into the check.
    pub fn apply(&mut self, data: &mut [u8]) {
        use aes::cipher::BlockCipherEncrypt;
        for part in data.chunks_mut(16) {
            let mut block = self.ctr.to_be_bytes().into();
            self.aes.encrypt_block(&mut block);
            for (b, s) in part.iter_mut().zip(block.iter()) {
                *b ^= s;
            }
            self.ctr = self.ctr.wrapping_add(1);
            // (the last block as if zeros followed)
            let (mac, read, size) = &mut self.chunk;
            mix(&self.aes, mac, part);
            *read += part.len();
            if *read == *size {
                let next = (*size + CHUNK).min(CHUNK_MAX);
                mix(&self.aes, &mut self.mac, &self.chunk.0);
                self.chunk = (self.start, 0, next);
            }
        }
    }

    /// Whether what was decrypted, now whole, is the file the key was
    /// made for.
    pub fn verified(mut self) -> bool {
        if self.chunk.1 > 0 {
            mix(&self.aes, &mut self.mac, &self.chunk.0);
        }
        let m = self.mac;
        let folded: Vec<u8> = (0..4)
            .map(|i| m[i] ^ m[i + 4])
            .chain((8..12).map(|i| m[i] ^ m[i + 4]))
            .collect();
        folded == self.check
    }
}

/// A CBC-MAC's next step: `mac` with `block` (16 bytes at most) mixed in.
fn mix(aes: &aes::Aes128, mac: &mut [u8; 16], block: &[u8]) {
    use aes::cipher::BlockCipherEncrypt;
    for (m, b) in mac.iter_mut().zip(block) {
        *m ^= b;
    }
    let mut out = (*mac).into();
    aes.encrypt_block(&mut out);
    mac.copy_from_slice(&out);
}

/// What a MEGA file that failed its check ([`MegaCipher::verified`]) comes
/// to.
pub(crate) const MEGA_CHANGED: &str =
    "MEGA: the file is not the one its link was made for (its check failed)";

/// A MEGA file link (`mega.nz/file/H#K`, `mega.nz/#!H!K`): (handle,
/// 32-byte key).
pub(crate) fn mega_link(u: &str) -> Option<(String, Vec<u8>)> {
    if u.contains("/folder/") || u.contains("#F!") {
        return None;
    }
    let (h, k) = if let Some(i) = u.find("/file/") {
        u[i + 6..].split_once('#')?
    } else {
        u[u.find("#!")? + 2..].split_once('!')?
    };
    let key = b64url(k).filter(|k| k.len() == 32)?;
    Some((handle(h)?, key))
}

/// A file's, folder's or node's handle as MEGA writes them (letters,
/// digits, `-`, `_`): it goes into the API's requests as it is.
fn handle(h: &str) -> Option<String> {
    let ok = |b: u8| b.is_ascii_alphanumeric() || b == b'-' || b == b'_';
    (!h.is_empty() && h.bytes().all(ok)).then(|| h.to_string())
}

/// A MEGA folder link (`mega.nz/folder/H#K`, with `/file/N` or
/// `/folder/N` for a node in it, also written `!N` or `?N`;
/// `mega.nz/#F!H!K!N`): (handle, 16-byte key, the node).
fn mega_folder(u: &str) -> Option<(String, Vec<u8>, Option<String>)> {
    let (h, rest) = if let Some(i) = u.find("/folder/") {
        u[i + 8..].split_once('#')?
    } else {
        u[u.find("#F!")? + 3..].split_once('!')?
    };
    let mut parts = rest.split(['/', '!', '?']);
    let key = b64url(parts.next()?).filter(|k| k.len() == 16)?;
    let node = parts
        .rfind(|p| !p.is_empty() && *p != "file" && *p != "folder")
        .map(str::to_string);
    Some((handle(h)?, key, node))
}

/// A MEGA folder link's handle and the node in it (as [`mega_folder`]).
pub(crate) fn folder_handle(u: &str) -> Option<(String, Option<String>)> {
    mega_folder(u).map(|(h, _, n)| (h, n))
}

/// A file in a MEGA folder.
#[derive(Clone)]
struct MegaFile {
    handle: String,
    /// The folders it is in, innermost first.
    under: Vec<String>,
    name: String,
    /// Its 32-byte key (as a file link's).
    key: Vec<u8>,
}

/// The nodes a folder's listing may have (a pack of a table's songs has a
/// few thousand).
const MAX_NODES: usize = 100_000;

/// A node of a folder's listing as the API sends it (one without a handle
/// is none).
#[derive(serde::Deserialize, Default)]
#[serde(default)]
struct Node {
    #[serde(deserialize_with = "crate::json::text")]
    h: String,
    #[serde(deserialize_with = "crate::json::text")]
    p: String,
    #[serde(deserialize_with = "crate::json::number")]
    t: Option<u64>,
    #[serde(deserialize_with = "crate::json::text")]
    a: String,
    #[serde(deserialize_with = "crate::json::text")]
    k: String,
}

/// A listing's nodes, those without a handle dropped as they are read;
/// more than [`MAX_NODES`] is an error.
fn nodes<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<Vec<Node>>, D::Error> {
    use serde::de::{Error, IgnoredAny, MapAccess, SeqAccess, Visitor};
    struct V;
    impl<'de> Visitor<'de> for V {
        type Value = Option<Vec<Node>>;
        fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
            f.write_str("nodes")
        }
        fn visit_seq<A: SeqAccess<'de>>(self, mut s: A) -> Result<Self::Value, A::Error> {
            let mut out = Vec::new();
            while let Some(n) = s.next_element::<crate::json::Object<Node>>()? {
                let Some(n) = n.0.filter(|n| !n.h.is_empty()) else {
                    continue;
                };
                if out.len() == MAX_NODES {
                    return Err(A::Error::custom(format_args!(
                        "more than {MAX_NODES} files and folders"
                    )));
                }
                out.push(n);
            }
            Ok(Some(out))
        }
        fn visit_map<A: MapAccess<'de>>(self, mut m: A) -> Result<Self::Value, A::Error> {
            while m.next_entry::<IgnoredAny, IgnoredAny>()?.is_some() {}
            Ok(None)
        }
        fn visit_str<E>(self, _: &str) -> Result<Self::Value, E> {
            Ok(None)
        }
        fn visit_u64<E>(self, _: u64) -> Result<Self::Value, E> {
            Ok(None)
        }
        fn visit_i64<E>(self, _: i64) -> Result<Self::Value, E> {
            Ok(None)
        }
        fn visit_f64<E>(self, _: f64) -> Result<Self::Value, E> {
            Ok(None)
        }
        fn visit_bool<E>(self, _: bool) -> Result<Self::Value, E> {
            Ok(None)
        }
        fn visit_unit<E>(self) -> Result<Self::Value, E> {
            Ok(None)
        }
    }
    d.deserialize_any(V)
}

/// The API's answer to one command: an error's number (alone, or in a
/// list), or the first of a list's objects.
enum Reply {
    Code(i64),
    Got(Got),
    Other,
}

/// What a command's answer holds of what is looked at: a file's download
/// URL and its attributes, a folder's nodes.
#[derive(serde::Deserialize, Default)]
#[serde(default)]
struct Got {
    #[serde(deserialize_with = "crate::json::text")]
    g: String,
    #[serde(deserialize_with = "crate::json::text")]
    at: String,
    #[serde(deserialize_with = "nodes")]
    f: Option<Vec<Node>>,
}

impl<'de> serde::Deserialize<'de> for Reply {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Reply, D::Error> {
        use serde::de::value::MapAccessDeserializer;
        use serde::de::{IgnoredAny, MapAccess, SeqAccess, Visitor};
        struct V;
        impl<'de> Visitor<'de> for V {
            type Value = Reply;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("MEGA's answer")
            }
            fn visit_i64<E>(self, n: i64) -> Result<Reply, E> {
                Ok(Reply::Code(n))
            }
            fn visit_u64<E>(self, n: u64) -> Result<Reply, E> {
                Ok(Reply::Code(n as i64))
            }
            fn visit_seq<A: SeqAccess<'de>>(self, mut s: A) -> Result<Reply, A::Error> {
                let first = s.next_element::<Reply>()?;
                while s.next_element::<IgnoredAny>()?.is_some() {}
                Ok(first.unwrap_or(Reply::Other))
            }
            fn visit_map<A: MapAccess<'de>>(self, m: A) -> Result<Reply, A::Error> {
                serde::Deserialize::deserialize(MapAccessDeserializer::new(m)).map(Reply::Got)
            }
            fn visit_str<E>(self, _: &str) -> Result<Reply, E> {
                Ok(Reply::Other)
            }
            fn visit_f64<E>(self, _: f64) -> Result<Reply, E> {
                Ok(Reply::Other)
            }
            fn visit_bool<E>(self, _: bool) -> Result<Reply, E> {
                Ok(Reply::Other)
            }
            fn visit_unit<E>(self) -> Result<Reply, E> {
                Ok(Reply::Other)
            }
        }
        d.deserialize_any(V)
    }
}

/// What the API answered `body` with: what it holds ([`Got`]), or the
/// failure its error is ([`refused`]).
fn reply(body: &[u8]) -> Result<Got, NetError> {
    match serde_json::from_slice(body).map_err(|e| format!("MEGA: {e}"))? {
        Reply::Got(g) => Ok(g),
        Reply::Code(n) => Err(refused(n)),
        Reply::Other => Err(unexpected(body)),
    }
}

/// The failure of an answer that holds neither what was asked for nor an
/// error's number: its first characters, not all an API may send.
fn unexpected(body: &[u8]) -> NetError {
    let text = String::from_utf8_lossy(&body[..body.len().min(320)]);
    let start: String = text.chars().take(80).collect();
    NetError::Other(format!("MEGA refused ({start})"))
}

/// The files of a MEGA folder (the API's `"a":"f"` answer), their keys
/// and names decrypted with the folder's `key`.
fn mega_nodes(body: &[u8], key: &[u8]) -> Result<Vec<MegaFile>, NetError> {
    use aes::cipher::{BlockCipherDecrypt, KeyInit};
    let Some(nodes) = reply(body)?.f else {
        return Err(unexpected(body));
    };
    let ecb = |k: &[u8], data: &mut [u8]| {
        let Ok(aes) = aes::Aes128::new_from_slice(k) else {
            return;
        };
        for b in data.as_chunks_mut::<16>().0 {
            let mut block = (*b).into();
            aes.decrypt_block(&mut block);
            b.copy_from_slice(&block);
        }
    };
    let parent: std::collections::HashMap<&str, &str> =
        nodes.iter().map(|n| (&*n.h, &*n.p)).collect();
    let mut out = Vec::new();
    for n in &nodes {
        if n.t != Some(0) {
            continue; // a folder
        }
        let Some(attr) = b64url(&n.a) else { continue };
        // "owner:key" for each who shares it; the folder's is the one that
        // opens the attributes
        let found = n.k.split('/').find_map(|part| {
            let mut k = b64url(part.split_once(':')?.1).filter(|k| k.len() == 32)?;
            ecb(key, &mut k);
            let name = mega_attr_name(&attr, &k)?;
            Some((k, name))
        });
        if let (Some((key, name)), Some(handle)) = (found, handle(&n.h)) {
            let mut under = Vec::new();
            let mut at = &*n.p;
            while let Some(p) = parent.get(at).filter(|_| under.len() < 64) {
                under.push(at.to_string());
                at = p;
            }
            out.push(MegaFile {
                handle,
                under,
                name,
                key,
            });
        }
    }
    Ok(out)
}

/// The file a MEGA folder link names, or the song `want` in the folder it
/// names (or in the whole folder).
fn pick_mega<'a>(
    files: &'a [MegaFile],
    node: Option<&str>,
    want: Option<&Want>,
) -> Option<&'a MegaFile> {
    if let Some(n) = node {
        if let Some(f) = files.iter().find(|f| f.handle == n) {
            return Some(f);
        }
        let inside: Vec<MegaFile> = files
            .iter()
            .filter(|f| f.under.iter().any(|u| u == n))
            .cloned()
            .collect();
        if let Some(p) = pick_in_pack(&inside, want) {
            return files.iter().find(|f| f.handle == p.handle);
        }
    }
    pick_in_pack(files, want)
}

/// Full-width ASCII as ASCII: a pack's file names hold "＊", "：", "＂"
/// for what a file name may not.
fn fold(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            '\u{ff01}'..='\u{ff5e}' => char::from_u32(c as u32 - 0xfee0).unwrap_or(c),
            '\u{3000}' => ' ',
            _ => c,
        })
        .collect()
}

/// A pack file's artist (`by`) is the table's `artist`: one holds the other,
/// without the chart maker's part ("artist / obj: someone") and spaces.
pub(crate) fn pack_artist(by: &str, artist: &str) -> bool {
    let squash = |s: &str| -> String {
        s.to_lowercase()
            .chars()
            .filter(|c| !c.is_whitespace())
            .collect()
    };
    let lower = artist.to_lowercase();
    let cut = ["/", "obj", "差分", "譜面"]
        .iter()
        .filter_map(|m| lower.find(m))
        .min()
        .unwrap_or(lower.len());
    let (a, b) = (squash(by), squash(&lower[..cut]));
    a.chars().count() >= 2 && b.chars().count() >= 2 && (a.contains(&b) || b.contains(&a))
}

/// The archive in a folder for `want`: its name is `[artist] title.zip`
/// (or just the title). Without a match, a folder of one song's archives
/// gives its best; one of many songs gives none.
fn pick_in_pack<'a>(files: &'a [MegaFile], want: Option<&Want>) -> Option<&'a MegaFile> {
    let archives: Vec<&MegaFile> = files
        .iter()
        .filter(|f| is_archive(Path::new(&f.name)))
        .collect();
    let best = |c: &[&'a MegaFile]| {
        let i = pick_file(c.iter().enumerate().map(|(i, f)| (f.name.as_str(), i)))?;
        Some(c[i])
    };
    if let Some(w) = want {
        let (title, artist) = (fold(&w.title), fold(&w.artist));
        // (the title exactly, the artist, the file)
        let mut found: Vec<(bool, bool, &MegaFile)> = Vec::new();
        for f in &archives {
            let name = fold(&f.name);
            let stem = name.rsplit_once('.').map_or(name.as_str(), |(s, _)| s);
            let (by, named) = match stem.strip_prefix('[').and_then(|s| s.split_once(']')) {
                Some((a, t)) => (a, t.trim()),
                None => ("", stem),
            };
            let exact = crate::titles::same_name(named, &title);
            if exact || crate::titles::same_title(named, &title) {
                found.push((exact, pack_artist(by, &artist), f));
            }
        }
        let with = |exact: bool, by: bool| -> Vec<&'a MegaFile> {
            found
                .iter()
                .filter(|(e, a, _)| (*e || !exact) && (*a || !by))
                .map(|(_, _, f)| *f)
                .collect()
        };
        // by its artist; else a title no other song in the pack has
        if let Some(f) = best(&with(true, true)).or_else(|| best(&with(false, true))) {
            return Some(f);
        }
        let only = |c: Vec<&'a MegaFile>| (c.len() == 1).then(|| c[0]);
        let long = title.chars().count() >= 6;
        if let Some(f) =
            only(with(true, false)).or_else(|| only(with(false, false)).filter(|_| long))
        {
            return Some(f);
        }
    }
    // one song's versions (ogg / wav, BGA), but not a pack's other songs
    // (`[artist] title`) for the one it lacks
    let versions: Vec<&MegaFile> = archives
        .into_iter()
        .filter(|f| want.is_none() || !f.name.starts_with('['))
        .collect();
    (versions.len() <= 6).then(|| best(&versions)).flatten()
}

/// The API request for a file's download URL.
pub(crate) fn mega_request(handle: &str) -> Vec<u8> {
    // ssl: an https URL (a page served over https may not read http)
    format!(r#"[{{"a":"g","g":1,"ssl":1,"p":"{handle}"}}]"#).into_bytes()
}

/// The download URL in the MEGA API's answer (an https one: the file's
/// bytes are asked of whatever it names), and the file's attributes.
pub(crate) fn mega_answer(body: &[u8]) -> Result<(String, String), NetError> {
    let got = reply(body)?;
    if got.g.is_empty() {
        return Err(unexpected(body));
    }
    if !got.g.starts_with("https://") {
        return Err("MEGA: the file's address is not an https one".into());
    }
    Ok((got.g, got.at))
}

/// The API's refusal by its error's number: -9 (no such file) and -2 (a
/// handle that is no more) say the file is gone; -4 that it limits
/// BMSeeker's requests (MEGA is not asked again in the run).
fn refused(code: i64) -> NetError {
    let e = format!("MEGA refused ([{code}])");
    match code {
        -4 => NetError::Refused(crate::refusal::Refusal::of(
            "mega.nz",
            crate::refusal::Why::RateLimited,
        )),
        -9 | -2 => NetError::Gone(e),
        _ => NetError::Other(e),
    }
}

/// The name in a MEGA node's attributes (`attr`, encrypted with the
/// node's 32-byte key `k`).
fn mega_attr_name(attr: &[u8], k: &[u8]) -> Option<String> {
    use aes::cipher::{BlockCipherDecrypt, KeyInit};
    let ak: Vec<u8> = (0..16).map(|i| k[i] ^ k[i + 16]).collect();
    let aes = aes::Aes128::new_from_slice(&ak).ok()?;
    // CBC with a zero IV
    let mut a = attr.to_vec();
    let mut prev = [0u8; 16];
    for b in a.as_chunks_mut::<16>().0 {
        let c = *b;
        let mut block = c.into();
        aes.decrypt_block(&mut block);
        for ((x, d), p) in b.iter_mut().zip(block.iter()).zip(prev) {
            *x = d ^ p;
        }
        prev = c;
    }
    let json = a.strip_prefix(b"MEGA")?;
    let end = json.iter().rposition(|&b| b == b'}')? + 1;
    #[derive(serde::Deserialize)]
    struct Attributes {
        n: String,
    }
    let attributes = serde_json::from_slice::<crate::json::Object<Attributes>>(&json[..end]);
    Some(attributes.ok()?.0?.n)
}

fn b64url(s: &str) -> Option<Vec<u8>> {
    let val = |c: u8| -> Option<u32> {
        Some(match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'-' | b'+' => 62,
            b'_' | b'/' => 63,
            _ => return None,
        } as u32)
    };
    let (mut out, mut acc, mut bits) = (Vec::new(), 0u32, 0);
    for c in s.trim_end_matches('=').bytes() {
        acc = (acc << 6) | val(c)?;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
            acc &= (1 << bits) - 1;
        }
    }
    Some(out)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::hosts::tests::resolve;

    #[test]
    fn refusals() {
        assert!(matches!(
            mega_answer(b"[-4]").unwrap_err(),
            NetError::Refused(crate::refusal::Refusal {
                why: crate::refusal::Why::RateLimited,
                ..
            })
        ));
        let gone = mega_answer(b"[-9]").unwrap_err();
        assert_eq!(gone.to_string(), "MEGA refused ([-9])");
        assert!(gone.gone());
        let other = mega_answer(b"[-3]").unwrap_err();
        assert_eq!(other, NetError::Other("MEGA refused ([-3])".into()));
        // alone, as the API sends an error of the whole request
        assert!(matches!(mega_answer(b"-4"), Err(NetError::Refused(_))));
        assert!(mega_answer(b"-9").unwrap_err().gone());
        // what is neither an error's number nor what was asked for: its
        // start, not all an API may send
        let long = format!(r#"[{{"x":"{}"}}]"#, "y".repeat(100_000));
        let NetError::Other(said) = mega_answer(long.as_bytes()).unwrap_err() else {
            panic!("no failure")
        };
        assert!(
            said.starts_with(r#"MEGA refused ([{"x":"yyy"#) && said.len() < 100,
            "{said}"
        );
        assert!(mega_nodes(br#"[{"g":"x"}]"#, &[0; 16]).is_err());
    }

    /// A file's bytes are asked over https only.
    #[test]
    fn the_files_address_is_https() {
        let ok = br#"[{"s":3,"g":"https://gfs.example/dl/x","at":"AA"}]"#;
        assert_eq!(
            mega_answer(ok).unwrap(),
            ("https://gfs.example/dl/x".to_string(), "AA".to_string())
        );
        for g in ["http://gfs.example/dl/x", "ftp://x/y", "//gfs.example/x"] {
            let answer = format!(r#"[{{"s":3,"g":"{g}"}}]"#);
            let e = mega_answer(answer.as_bytes()).unwrap_err();
            assert!(e.to_string().contains("not an https"), "{g}: {e}");
        }
    }

    /// The key's check of a file, as MEGA makes it (AES-CBC-MAC of each
    /// chunk from the nonce twice, of the chunks' from zero, folded).
    pub(crate) fn mega_check(aes_key: [u8; 16], nonce: [u8; 8], plain: &[u8]) -> [u8; 8] {
        use aes::cipher::{BlockCipherEncrypt, KeyInit};
        let aes = aes::Aes128::new(&aes_key.into());
        let cbc = |mac: &mut [u8; 16], data: &[u8]| {
            for block in data.chunks(16) {
                block.iter().zip(mac.iter_mut()).for_each(|(b, m)| *m ^= b);
                let mut out = (*mac).into();
                aes.encrypt_block(&mut out);
                *mac = out.into();
            }
        };
        let mut all = [0u8; 16];
        let (mut at, mut size) = (0, 0x20000);
        while at < plain.len() {
            let end = (at + size).min(plain.len());
            let mut mac = [nonce, nonce].concat().try_into().unwrap();
            cbc(&mut mac, &plain[at..end]);
            cbc(&mut all, &mac);
            at = end;
            size = (size + 0x20000).min(0x100000);
        }
        let mut check = [0u8; 8];
        for i in 0..4 {
            check[i] = all[i] ^ all[i + 4];
            check[i + 4] = all[i + 8] ^ all[i + 12];
        }
        check
    }

    /// A file link's key for `plain` under `aes_key` and `nonce`, and the
    /// bytes MEGA would send for it.
    pub(crate) fn mega_file(aes_key: [u8; 16], nonce: [u8; 8], plain: &[u8]) -> (Vec<u8>, Vec<u8>) {
        let check = mega_check(aes_key, nonce, plain);
        let mut key = [aes_key.to_vec(), nonce.to_vec(), check.to_vec()].concat();
        for i in 0..16 {
            key[i] ^= key[i + 16];
        }
        // (CTR: to decrypt is to encrypt)
        let mut sent = plain.to_vec();
        MegaCipher::new(&key).apply(&mut sent);
        (key, sent)
    }

    /// A file decrypted in parts of any size is the file its key was made
    /// for; a byte changed on the way, or a part left out, is not.
    #[test]
    fn a_files_check() {
        let (aes_key, nonce) = ([7u8; 16], [9u8; 8]);
        // (sizes around the chunks' ends: 128 KiB, then 256 KiB more)
        for len in [
            0,
            1,
            16,
            17,
            0x20000 - 1,
            0x20000,
            0x20001,
            0x60000,
            0x60010,
            2_500_000,
        ] {
            let plain: Vec<u8> = (0..len).map(|i| (i * 31 % 251) as u8).collect();
            let (key, mut sent) = mega_file(aes_key, nonce, &plain);
            for part in [16, 1 << 16, usize::MAX] {
                let mut c = MegaCipher::new(&key);
                let mut got = sent.clone();
                got.chunks_mut(part).for_each(|p| c.apply(p));
                assert_eq!(got, plain);
                assert!(c.verified(), "{len} bytes in parts of {part}");
            }
            if len == 0 {
                continue;
            }
            let mut changed = sent.clone();
            changed[len / 2] ^= 1;
            let mut c = MegaCipher::new(&key);
            c.apply(&mut changed);
            assert!(!c.verified(), "{len} bytes, one changed");
            let mut c = MegaCipher::new(&key);
            c.apply(&mut sent[..len - 1]);
            assert!(!c.verified(), "{len} bytes, the last left out");
        }
    }

    #[test]
    fn file_links() {
        let link = "https://mega.nz/file/bzQy3bYS#Og0LBoWkfbKwxUceGvzLHDl2exyI0l1DiQ3XNw79zMw";
        let t = resolve(
            link,
            &[(
                "https://g.api.mega.co.nz/cs?id=0",
                br#"[{"s":3,"g":"https://gfs.example/dl/x"}]"#,
            )],
        )
        .unwrap();
        assert_eq!(t.url, "https://gfs.example/dl/x");
        assert_eq!(t.mega_key.map(|k| k.len()), Some(32));
        assert_eq!(b64url("AQID").unwrap(), [1, 2, 3]);
        assert_eq!(b64url("-_8").unwrap(), [0xfb, 0xff]);
        // a handle is letters, digits, `-` and `_`: any other text is no
        // MEGA link, and the API is not asked
        let (key, folder_key) = (link.split_once('#').unwrap().1, "wfv-iI4UuSIaGpP0oFZdtQ");
        for h in [r#"x","a":"f"#, "x&sid=1", "x y", ""] {
            let file = format!("https://mega.nz/file/{h}#{key}");
            assert_eq!(mega_link(&file), None, "{h}");
            assert_eq!(resolve(&file, &[]), Err(crate::hosts::Fetch::Page), "{h}");
            let folder = format!("https://mega.nz/folder/{h}#{folder_key}");
            assert!(mega_folder(&folder).is_none(), "{h}");
            assert_eq!(resolve(&folder, &[]), Err(crate::hosts::Fetch::Page), "{h}");
        }
    }

    /// Which archive of a MEGA pack each song gets: `BMSEEKER_PACKS=FILE`
    /// of `title<TAB>artist<TAB>link` lines.
    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    #[ignore = "network"]
    fn mega_packs() {
        let list = std::fs::read_to_string(std::env::var("BMSEEKER_PACKS").unwrap()).unwrap();
        let mut seen = std::collections::HashMap::new();
        for line in list.lines() {
            let [title, artist, link] = line.splitn(3, '\t').collect::<Vec<_>>()[..] else {
                continue;
            };
            let (h, key, node) = mega_folder(link.trim()).unwrap();
            let files = seen.entry(h.clone()).or_insert_with(|| {
                use crate::install::job::{Driver, Effect, Fed};
                let req = crate::net::Request::Post {
                    url: format!("{}&n={h}", Config::default().mega_api),
                    body: br#"[{"a":"f","c":1,"r":1,"ca":1}]"#.to_vec(),
                    kind: Kind::Listing,
                };
                let native = crate::native::Native::new(&Default::default());
                let Fed::Http(got) = native.perform(Effect::Http(req)) else {
                    unreachable!()
                };
                let got = got.and_then(|b| mega_nodes(&b, &key));
                got.unwrap_or_else(|e| {
                    println!("{h}: {e}");
                    Vec::new()
                })
            });
            let want = Want {
                title: title.into(),
                artist: artist.into(),
            };
            let pick = pick_mega(files, node.as_deref(), Some(&want));
            println!("{title} | {artist} -> {:?}", pick.map(|f| &f.name));
        }
    }

    #[test]
    fn mega_folders() {
        let key = "wfv-iI4UuSIaGpP0oFZdtQ";
        let parse = |u: &str| mega_folder(u).map(|(h, k, n)| (h, k.len(), n));
        let at = |n: &str| Some(("FNFDCQaT".to_string(), 16, Some(n.to_string())));
        assert_eq!(
            parse(&format!("https://mega.nz/folder/FNFDCQaT#{key}")),
            Some(("FNFDCQaT".to_string(), 16, None))
        );
        assert_eq!(
            parse(&format!("https://mega.nz/folder/FNFDCQaT#{key}/file/N1")),
            at("N1")
        );
        assert_eq!(
            parse(&format!("https://mega.nz/folder/FNFDCQaT#{key}/folder/S")),
            at("S")
        );
        assert_eq!(
            parse(&format!("https://mega.nz/folder/FNFDCQaT#{key}?N1")),
            at("N1")
        );
        assert_eq!(
            parse(&format!("https://mega.nz/folder/FNFDCQaT#{key}!N1")),
            at("N1")
        );
        assert_eq!(
            parse(&format!("https://mega.nz/#F!FNFDCQaT!{key}!N1")),
            at("N1")
        );
        assert_eq!(parse("https://mega.nz/file/bzQy3bYS#Og0LBoWk"), None);

        // a node as the API sends it: its key under the folder's, its
        // attributes under its own
        use aes::cipher::{BlockCipherEncrypt, KeyInit};
        let enc = |k: &[u8], data: &mut [u8], cbc: bool| {
            let aes = aes::Aes128::new_from_slice(k).unwrap();
            let mut prev = [0u8; 16];
            for b in data.as_chunks_mut::<16>().0 {
                if cbc {
                    b.iter_mut().zip(prev).for_each(|(x, p)| *x ^= p);
                }
                let mut block = (*b).into();
                aes.encrypt_block(&mut block);
                b.copy_from_slice(&block);
                prev = *b;
            }
        };
        let b64 = |b: &[u8]| {
            const A: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
            let mut out = String::new();
            for c in b.chunks(3) {
                let n = c.iter().fold(0u32, |n, &x| n << 8 | x as u32) << (8 * (3 - c.len()));
                for i in 0..=c.len() {
                    out.push(A[(n >> (18 - 6 * i) & 63) as usize] as char);
                }
            }
            out
        };
        let folder = [7u8; 16];
        let node = |h: &str, name: &str, fk: u8| {
            let mut k: Vec<u8> = (0..32).map(|i| i as u8 ^ fk).collect();
            let ak: Vec<u8> = (0..16).map(|i| k[i] ^ k[i + 16]).collect();
            let mut a = format!(r#"MEGA{{"n":"{name}"}}"#).into_bytes();
            a.resize(a.len().div_ceil(16) * 16, 0);
            enc(&ak, &mut a, true);
            enc(&folder, &mut k, false);
            format!(
                r#"{{"h":"{h}","p":"D","t":0,"a":"{}","k":"R:{}"}}"#,
                b64(&a),
                b64(&k)
            )
        };
        let body = format!(
            r#"[{{"f":[{{"h":"R","p":"","t":1}},{{"h":"D","p":"R","t":1}},{},{},{}]}}]"#,
            node("A", "[xi] Beatrice.zip", 1),
            node("B", "[Beatrice de Fermat] Lieselotte.zip", 2),
            node("C", "[Team：SASAKURATION] AVALON.zip", 3),
        );
        let files = mega_nodes(body.as_bytes(), &folder).unwrap();
        assert_eq!(files.len(), 3);
        assert_eq!(files[0].name, "[xi] Beatrice.zip");
        assert_eq!(files[0].under, ["D", "R"]);
        assert_eq!(
            files[0].key,
            (0..32).map(|i| i as u8 ^ 1).collect::<Vec<_>>()
        );

        let pick = |title: &str, artist: &str| {
            let want = Want {
                title: title.into(),
                artist: artist.into(),
            };
            pick_in_pack(&files, Some(&want)).map(|f| f.handle.as_str())
        };
        assert_eq!(pick("Beatrice [ANOTHER]", "xi"), Some("A"));
        assert_eq!(
            pick("AVALON -Genocide-", "Team:SASAKURATION / selene."),
            Some("C")
        );
        // a title no other song has, whoever the table says made it
        assert_eq!(pick("AVALON", "someone"), Some("C"));
        assert_eq!(pick("Elina", "Squar"), None);
        // a pack of a few songs without a match: its best
        assert!(pick_in_pack(&files, None).is_some());
        assert_eq!(
            pick_mega(&files, Some("B"), None).map(|f| f.handle.as_str()),
            Some("B")
        );

        // a node whose handle is not one is left out
        let odd = format!(r#"[{{"f":[{}]}}]"#, node(r#"A\",\"a\":\"f"#, "a.zip", 1));
        assert!(mega_nodes(odd.as_bytes(), &folder).unwrap().is_empty());

        // a name is one file name, whatever the attributes say: a folder's
        // file, and a file link's
        const API: &str = "https://g.api.mega.co.nz/cs?id=0";
        let body = format!(
            r#"[{{"g":"https://gfs.example/dl/x","f":[{}]}}]"#,
            node("A", "../../evil.zip", 1)
        );
        let link = format!("https://mega.nz/folder/FNFDCQaT#{}", b64(&folder));
        let listing = format!("{API}&n=FNFDCQaT");
        let t = resolve(&link, &[(listing.as_str(), body.as_bytes())]).unwrap();
        assert_eq!(t.name.as_deref(), Some("_.._evil.zip"));
        let key = "Og0LBoWkfbKwxUceGvzLHDl2exyI0l1DiQ3XNw79zMw";
        let k = b64url(key).unwrap();
        let ak: Vec<u8> = (0..16).map(|i| k[i] ^ k[i + 16]).collect();
        let mut a = br#"MEGA{"n":"/home/u/.bashrc"}"#.to_vec();
        a.resize(a.len().div_ceil(16) * 16, 0);
        enc(&ak, &mut a, true);
        let body = format!(r#"[{{"g":"https://gfs.example/dl/x","at":"{}"}}]"#, b64(&a));
        let link = format!("https://mega.nz/file/bzQy3bYS#{key}");
        let t = resolve(&link, &[(API, body.as_bytes())]).unwrap();
        assert_eq!(t.name.as_deref(), Some("_home_u_.bashrc"));
    }
}
