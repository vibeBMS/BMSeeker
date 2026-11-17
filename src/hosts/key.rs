//! Which file a link downloads, as a key: two links to one file (a Drive
//! file's `view` and `uc?id=` links, a Dropbox link with `dl=0` and with
//! `dl=1`) have the same key, so a batch of downloads (a table's missing
//! charts) gets each archive once.

use super::Want;

/// A download's key: `gdrive:ID`, `mediafire:ID`, `mega:HANDLE`,
/// `dropbox:PATH`, else the URL without its fragment. A folder's link
/// (Drive, MEGA) brings the archive of the song looked for in it: its key
/// names that song too.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct DownloadKey(String);

impl DownloadKey {
    /// The key of `url`, for the song `want` (a folder's link: the song's
    /// archive in it).
    pub(crate) fn of(url: &str, want: Option<&Want>) -> DownloadKey {
        let url = url.trim();
        let (key, folder) = match super::Link::parse(url, 0) {
            Some(l) => known(&l).unwrap_or_else(|| (plain(url), false)),
            None => (plain(url), false),
        };
        match want.filter(|_| folder) {
            Some(w) => DownloadKey(format!(
                "{key}|{}|{}",
                w.title.trim().to_lowercase(),
                w.artist.trim().to_lowercase()
            )),
            None => DownloadKey(key),
        }
    }
}

/// The URL without its fragment, its scheme and host in lower case.
fn plain(url: &str) -> String {
    let url = url.split('#').next().unwrap_or(url);
    match url.find("://") {
        Some(i) => {
            let rest = &url[i + 3..];
            let end = rest.find(['/', '?']).unwrap_or(rest.len());
            format!(
                "{}://{}{}",
                url[..i].to_ascii_lowercase(),
                rest[..end].to_ascii_lowercase(),
                &rest[end..]
            )
        }
        None => url.to_string(),
    }
}

/// A host's own key for `l`, and whether it is a folder's.
fn known(l: &super::Link) -> Option<(String, bool)> {
    if l.on("google.com") || l.on("googleusercontent.com") {
        let id = super::drive::drive_id(&l.url)?;
        let folder = l.lower.contains("/folders/") || l.lower.contains("folderview");
        let kind = if folder { "gdrive-folder" } else { "gdrive" };
        return Some((format!("{kind}:{id}"), folder));
    }
    if l.on("mediafire.com") {
        // `/file/ID/name/file`, `/download/ID/...`, `/?ID`, `/view/ID`
        let path = l.url.split_once("mediafire.com").map(|(_, p)| p)?;
        let id = ["/file/", "/download/", "/view/", "/?", "?"]
            .iter()
            .find_map(|k| path.find(k).map(|i| &path[i + k.len()..]))?;
        let id: String = id
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric())
            .collect();
        return (!id.is_empty()).then(|| (format!("mediafire:{}", id.to_lowercase()), false));
    }
    if l.on("mega.nz") || l.on("mega.co.nz") {
        if let Some((h, _)) = super::mega::mega_link(&l.url) {
            return Some((format!("mega:{h}"), false));
        }
        let (h, node) = super::mega::folder_handle(&l.url)?;
        let node = node.map(|n| format!("/{n}")).unwrap_or_default();
        return Some((format!("mega-folder:{h}{node}"), true));
    }
    if l.on("dropbox.com") || l.on("dropboxusercontent.com") {
        // the path, and the share's key (`rlkey`); not `dl=`
        let rest = &l.url[l.url.find(&l.host)? + l.host.len()..];
        let rest = rest.split('#').next().unwrap_or(rest);
        let (path, query) = rest.split_once('?').unwrap_or((rest, ""));
        let rlkey = query
            .split('&')
            .find(|p| p.starts_with("rlkey="))
            .map(|p| format!("?{p}"))
            .unwrap_or_default();
        return Some((format!("dropbox:{path}{rlkey}"), false));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(u: &str) -> String {
        DownloadKey::of(u, None).0
    }

    #[test]
    fn keys() {
        let drive = "gdrive:1AbC-d_E";
        assert_eq!(
            key("https://drive.google.com/file/d/1AbC-d_E/view?usp=sharing"),
            drive
        );
        assert_eq!(
            key("https://drive.google.com/uc?id=1AbC-d_E&export=download"),
            drive
        );
        assert_eq!(key("https://drive.google.com/open?id=1AbC-d_E"), drive);
        assert_eq!(
            key("https://www.mediafire.com/file/AbC123x/song.zip/file"),
            "mediafire:abc123x"
        );
        assert_eq!(
            key("http://www.mediafire.com/?abc123x"),
            "mediafire:abc123x"
        );
        assert_eq!(
            key("https://www.dropbox.com/scl/fi/xyz/song.zip?rlkey=k1&dl=0"),
            key("https://www.dropbox.com/scl/fi/xyz/song.zip?dl=1&rlkey=k1")
        );
        assert_ne!(
            key("https://www.dropbox.com/scl/fi/xyz/song.zip?rlkey=k1"),
            key("https://www.dropbox.com/scl/fi/xyz/song.zip?rlkey=k2")
        );
        // a file's key does not depend on the song looked for
        let k = "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";
        let mega = format!("https://mega.nz/file/H1#{k}");
        assert_eq!(key(&mega), "mega:H1");
        assert_eq!(key(&format!("https://mega.nz/#!H1!{k}")), "mega:H1");
        let want = Want {
            title: "Song".into(),
            artist: "A".into(),
        };
        assert_eq!(DownloadKey::of(&mega, Some(&want)).0, "mega:H1");
        // a folder's does
        let folder = "https://drive.google.com/drive/folders/F1?usp=sharing";
        assert_eq!(key(folder), "gdrive-folder:F1");
        assert_eq!(
            DownloadKey::of(folder, Some(&want)).0,
            "gdrive-folder:F1|song|a"
        );
        let mf = "https://mega.nz/folder/H2#AAAAAAAAAAAAAAAAAAAAAA";
        assert_eq!(key(mf), "mega-folder:H2");
        assert_eq!(
            key("https://mega.nz/folder/H2#AAAAAAAAAAAAAAAAAAAAAA/file/N3"),
            "mega-folder:H2/N3"
        );
        // anything else: the URL without its fragment
        assert_eq!(
            key("HTTP://Example.COM/a/B.zip?x=1#top"),
            "http://example.com/a/B.zip?x=1"
        );
        assert_eq!(key("not a link"), "not a link");
    }
}
