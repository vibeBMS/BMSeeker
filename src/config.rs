//! Every place bmseeker reads from, and how it behaves, in one value. The
//! defaults are the public services BMS players use; a JSON file (or the
//! fields a program sets) changes any of them.

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// Sent with requests (hosts answer browsers better).
    pub user_agent: String,
    /// The BMS event mirror's list of events (vibeBMS/BMS `events.json`):
    /// a manbow work it holds is downloaded from there. Empty: none.
    pub mirror_catalog: String,
    /// manbow's event pages (`event.cgi`).
    pub manbow: String,
    /// The BMS SEARCH API.
    pub bmssearch_api: String,
    /// The Stella uploader's list of charts and their links.
    pub stella_list: String,
    /// The MEGA API.
    pub mega_api: String,
    /// Where else a song's links are looked for, in order: `table` (the
    /// difficulty tables given), `stella`, `tsv` (link lists given),
    /// `bmssearch`, `mirrors` (a separate chart in [`Config::chart_mirrors`]),
    /// `bmslibrary` (the community archive's songs: [`Config::bms_library`]).
    /// The last two only with [`Config::third_party`].
    pub sources: Vec<String>,
    /// Whether third parties' copies are used: other people's mirrors of
    /// dead sites and uploaders, and the community archive
    /// ([`Config::uploaders`], the `mirrors` and `bmslibrary` sources). Not
    /// the authors' own links, nor the event mirror of manbow works.
    pub third_party: bool,
    /// Dead uploaders' files, numbered, kept elsewhere: a link to one goes
    /// to its mirrors (with [`Config::third_party`]).
    pub uploaders: Vec<Uploader>,
    /// Folders where separate charts of dead sites are kept under their
    /// file names (BMS SEARCH knows a chart's): the `mirrors` source.
    pub chart_mirrors: Vec<String>,
    /// The community archive's "BMS Library" Google Drive folder (a folder
    /// per first letter, of `Title (by Artist).rar`): the `bmslibrary`
    /// source.
    pub bms_library: String,
    /// Difficulty tables to use when none is given.
    pub tables: Vec<String>,
    /// The folder (in a library) downloads are kept in until imported.
    pub downloads: String,
    /// How much of a chart's sounds a song folder must hold to take it
    /// (0..1).
    pub min_coverage: f32,
    /// Whether a download whose charts are all in the library already
    /// brings its files in again: those of a name the song's folder has
    /// replace them when newer. Else only the files the folder lacks come
    /// in.
    pub reinstall: bool,
    /// Whether a song new to the library names its folder after its charts
    /// (as the BeMusicSeeker fork does: [`Config::folder_name_format`]; a
    /// folder the archive wraps everything in left out). Else after the
    /// song's title or the archive, as it has been.
    pub folder_naming: bool,
    /// How such a folder is named: `%ARTIST%` and `%TITLE%`, what the
    /// charts' artists and titles share.
    pub folder_name_format: String,
    /// Such a folder's name has only characters Shift_JIS has (LR2 on
    /// Windows).
    pub sjis_only: bool,
}

/// The [`Config::sources`] that are third parties' copies.
pub const THIRD_PARTY_SOURCES: &[&str] = &["mirrors", "bmslibrary"];

/// A dead uploader whose files are kept elsewhere by number.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Uploader {
    /// Its host name: a link there names a file by number (the last digits
    /// in it, as `upload.cgi?get=03691`).
    pub site: String,
    /// The number's width, zeros before.
    pub digits: usize,
    /// Where its files are, tried in order: `{n}` is the number, `{h}` its
    /// hundreds (`03691`: `36`), both with their zeros; `{f}` the link's
    /// file name (`up0650.zip`), `{d}` its first folder (`uploader` of
    /// `/uploader/src/up0650.zip`: an uploader of several boards). A Google Drive
    /// folder link may end in `#sub/folder/file` names, followed from it.
    pub mirrors: Vec<String>,
}

impl Default for Uploader {
    fn default() -> Uploader {
        Uploader {
            site: String::new(),
            digits: 5,
            mirrors: Vec::new(),
        }
    }
}

impl Uploader {
    /// The file number in `url` (a link to this uploader).
    pub fn number(&self, url: &str) -> Option<u64> {
        let lower = url.to_ascii_lowercase();
        let host = lower.split('/').nth(2)?;
        if self.site.is_empty()
            || !(host == self.site || host.ends_with(&format!(".{}", self.site)))
        {
            return None;
        }
        let rest = &lower[lower.find(host)? + host.len()..];
        let digits: String = rest
            .rsplit(|c: char| !c.is_ascii_digit())
            .find(|d| !d.is_empty())?
            .to_string();
        digits.parse().ok()
    }

    /// `url`'s file on each mirror.
    pub fn mirrored(&self, url: &str) -> Vec<String> {
        let Some(n) = self.number(url) else {
            return Vec::new();
        };
        let w = self.digits;
        let num = format!("{n:0w$}");
        let hundreds = format!("{:0h$}", n / 100, h = w.saturating_sub(3).max(1));
        let path = url
            .split(['?', '#'])
            .next()
            .unwrap_or("")
            .splitn(4, '/')
            .nth(3)
            .unwrap_or("");
        let folder = path.split('/').next().unwrap_or("");
        let file = path.rsplit('/').next().unwrap_or("");
        self.mirrors
            .iter()
            .filter(|m| {
                !(m.contains("{f}") && file.is_empty() || m.contains("{d}") && folder.is_empty())
            })
            .map(|m| {
                m.replace("{n}", &num)
                    .replace("{h}", &hundreds)
                    .replace("{f}", file)
                    .replace("{d}", folder)
            })
            .collect()
    }
}

impl Default for Config {
    fn default() -> Config {
        Config {
            user_agent: "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126 Safari/537.36".into(),
            mirror_catalog: "https://raw.githubusercontent.com/vibeBMS/BMS/main/events.json".into(),
            manbow: "https://manbow.nothing.sh/event/event.cgi".into(),
            bmssearch_api: "https://api.bmssearch.net/v1".into(),
            stella_list: "https://stellabms.xyz/score_upload_full.json".into(),
            mega_api: "https://g.api.mega.co.nz/cs?id=0".into(),
            sources: [
                "table",
                "stella",
                "tsv",
                "bmssearch",
                "mirrors",
                "bmslibrary",
            ]
            .map(String::from)
            .to_vec(),
            third_party: false,
            uploaders: vec![
                Uploader {
                site: "gnqg.rosx.net".into(),
                digits: 5,
                mirrors: [
                    "https://bms.hexlataia.xyz/mirror/gnqg-upload/{n}.zip",
                    "https://darksabun.club/mirror/gnqg-upload/{n}.zip",
                    // the community archive
                    "https://drive.google.com/drive/folders/1ASZJPUALe1-gxQ034wSUlFuAGWlVUULz#{h}xx/{n}.zip",
                ]
                .map(String::from)
                .to_vec(),
            },
                // its boards' files (/uploader/src/up0650.zip, /bof/src/...)
                Uploader {
                    site: "absolute.pv.land.to".into(),
                    digits: 4,
                    mirrors: vec!["https://darksabun.club/mirror/absolute/{d}/{f}".into()],
                },
            ],
            chart_mirrors: vec!["https://darksabun.club/mirror/absolute/bms/".into()],
            bms_library:
                "https://drive.google.com/drive/folders/1mTcWY9jYMsnEzbVfvGkjfepBpN4Tj786".into(),
            tables: [
                "https://darksabun.club/table/archive/insane1/",
                "https://darksabun.club/table/archive/normal1/",
            ]
            .map(String::from)
            .to_vec(),
            downloads: ".bmseeker".into(),
            min_coverage: 0.99,
            reinstall: false,
            folder_naming: false,
            folder_name_format: crate::naming::DEFAULT_FORMAT.into(),
            sjis_only: false,
        }
    }
}

impl Config {
    /// A JSON file's settings over the defaults (fields left out keep
    /// theirs).
    pub fn from_json(text: &str) -> Result<Config, String> {
        serde_json::from_str(text).map_err(|e| e.to_string())
    }

    /// Whether `name` is one of the [`Config::sources`] (a third party's
    /// only with [`Config::third_party`]).
    pub fn uses(&self, name: &str) -> bool {
        (self.third_party || !THIRD_PARTY_SOURCES.contains(&name))
            && self.sources.iter().any(|s| s == name)
    }

    /// The settings file: `path`, else `$BMSEEKER_CONFIG`, else
    /// `<config folder>/bmseeker/config.json` if there is one, else the
    /// defaults.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn load(path: Option<&std::path::Path>) -> Result<Config, String> {
        let file = path
            .map(std::path::Path::to_path_buf)
            .or_else(|| std::env::var_os("BMSEEKER_CONFIG").map(Into::into))
            .or_else(|| {
                let dir = std::env::var_os("XDG_CONFIG_HOME")
                    .map(std::path::PathBuf::from)
                    .or_else(|| std::env::var_os("APPDATA").map(Into::into))
                    .or_else(|| {
                        std::env::var_os("HOME").map(|h| std::path::Path::new(&h).join(".config"))
                    })?;
                Some(dir.join("bmseeker").join("config.json")).filter(|f| f.exists())
            });
        match file {
            None => Ok(Config::default()),
            Some(f) => {
                let text =
                    std::fs::read_to_string(&f).map_err(|e| format!("{}: {e}", f.display()))?;
                Config::from_json(&text).map_err(|e| format!("{}: {e}", f.display()))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn partial_files() {
        let c = Config::from_json(r#"{"mirror_catalog": "", "sources": ["bmssearch"]}"#).unwrap();
        assert_eq!(c.mirror_catalog, "");
        assert!(c.uses("bmssearch") && !c.uses("stella"));
        // third parties' only when allowed
        let c = Config::default();
        assert!(!c.uses("mirrors") && !c.uses("bmslibrary"));
        let c = Config {
            third_party: true,
            ..c
        };
        assert!(c.uses("mirrors") && c.uses("bmslibrary"));
        assert_eq!(c.manbow, Config::default().manbow);
        assert!(Config::from_json("{").is_err());
    }

    #[test]
    fn uploader_numbers() {
        let u = Uploader {
            site: "gnqg.rosx.net".into(),
            digits: 5,
            mirrors: vec![
                "https://m/{n}.zip".into(),
                "https://d/f#{h}xx/{n}.zip".into(),
            ],
        };
        assert_eq!(
            u.mirrored("http://gnqg.rosx.net/upload/upload.cgi?get=03691"),
            ["https://m/03691.zip", "https://d/f#36xx/03691.zip"]
        );
        assert_eq!(
            u.number("http://gnqg.rosx.net/upload/src/up12.zip"),
            Some(12)
        );
        assert_eq!(u.number("http://gnqg.rosx.net/upload/"), None);
        // by its board and file name
        let a = &Config::default().uploaders[1];
        assert_eq!(
            a.mirrored("http://absolute.pv.land.to/uploader/src/up3833.rar"),
            ["https://darksabun.club/mirror/absolute/uploader/up3833.rar"]
        );
        assert_eq!(u.number("http://example.com/upload.cgi?get=03691"), None);
    }
}
