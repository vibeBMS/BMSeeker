//! What bmseeker needs of a chart: its title, artist and the files it uses
//! (sounds, BGA images and movies, its stage file, banner and back image,
//! preview), to tell which song's folder it belongs in. BMS text (`#TITLE`,
//! `#ARTIST`, `#WAVxx`, `#BMPxx`, ..., every `#RANDOM` branch's) and bmson.

use std::path::Path;

/// The title, artist and files of a chart file (names as written in it).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ChartInfo {
    pub title: String,
    pub artist: String,
    /// Sound files it uses (`#WAVxx`).
    pub sounds: Vec<String>,
    /// BGA images it uses (`#BMPxx` / bmson `bga_header` that are images).
    pub images: Vec<String>,
    /// BGA movies it uses (`#BMPxx` / `bga_header` that are movies).
    pub movies: Vec<String>,
    /// `#STAGEFILE` (bmson `eyecatch_image`).
    pub stagefile: String,
    /// `#BANNER` (bmson `banner_image`).
    pub banner: String,
    /// `#BACKBMP` (bmson `back_image`).
    pub backbmp: String,
    /// `#PREVIEW` (bmson `preview_music`).
    pub preview: String,
}

/// The kinds of file a chart uses: a name of one stands for any of its
/// extensions (BMS often names `.wav` files that are shipped as `.ogg`,
/// `.bmp` as `.png`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Class {
    Audio,
    Image,
    Movie,
}

const AUDIO: [&str; 4] = ["wav", "ogg", "mp3", "flac"];
const IMAGE: [&str; 4] = ["png", "bmp", "jpg", "jpeg"];
const MOVIE: [&str; 15] = [
    "mpg", "mpeg", "mp4", "m4v", "mp4v", "avi", "wmv", "mov", "webm", "mkv", "m1v", "m2v", "3gp",
    "flv", "rm",
];

/// A chart's MD5 as it is compared: 32 hex digits, lower case (None: not
/// one).
pub(crate) fn md5_key(m: &str) -> Option<String> {
    let m = m.trim();
    (m.len() == 32 && m.bytes().all(|b| b.is_ascii_hexdigit())).then(|| m.to_ascii_lowercase())
}

/// The class of a file by its extension (None: not one a chart uses).
pub(crate) fn class_of(name: &str) -> Option<Class> {
    let name = name.rsplit(['/', '\\']).next().unwrap_or(name);
    let (_, ext) = name.rsplit_once('.')?;
    let ext = ext.to_ascii_lowercase();
    let is = |list: &[&str]| list.contains(&ext.as_str());
    if is(&AUDIO) {
        Some(Class::Audio)
    } else if is(&IMAGE) {
        Some(Class::Image)
    } else if is(&MOVIE) {
        Some(Class::Movie)
    } else {
        None
    }
}

/// `list` with `name` added (once; none empty).
fn push(list: &mut Vec<String>, name: &str) {
    if !name.is_empty() && !list.iter().any(|x| x == name) {
        list.push(name.to_string());
    }
}

impl ChartInfo {
    /// A BGA file (`#BMPxx`, `bga_header`): an image or a movie by its
    /// extension (anything else is left out).
    fn add_bga(&mut self, name: &str) {
        match class_of(name) {
            Some(Class::Image) => push(&mut self.images, name),
            Some(Class::Movie) => push(&mut self.movies, name),
            _ => {}
        }
    }

    /// Its stage file, banner and back image (those it names).
    pub(crate) fn extra_images(&self) -> Vec<&str> {
        [&self.stagefile, &self.banner, &self.backbmp]
            .into_iter()
            .map(|s| s.as_str())
            .filter(|s| !s.is_empty())
            .collect()
    }
}

/// `bytes` of the chart file `path` (bmson by its extension, else BMS).
pub(crate) fn info(path: &Path, bytes: &[u8]) -> Option<ChartInfo> {
    let bmson = path
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("bmson"));
    if bmson {
        return bmson_info(bytes);
    }
    let text = crate::text::decode_text(bytes);
    let mut out = ChartInfo::default();
    // #WAVxx / #BMPxx by id (case aside; in id order), the last of an id
    let mut wavs = std::collections::BTreeMap::new();
    let mut bmps = std::collections::BTreeMap::new();
    let first = |s: &mut String, arg: &str| {
        if s.is_empty() {
            *s = arg.to_string();
        }
    };
    for line in text.lines() {
        let Some(l) = line.trim_start().strip_prefix('#') else {
            continue;
        };
        // (note data: `#00111:…`, most of a chart)
        if l.as_bytes().first().is_none_or(u8::is_ascii_digit) {
            continue;
        }
        let (cmd, arg) = l
            .split_once(|c: char| c.is_whitespace())
            .map_or((l, ""), |(c, a)| (c, a.trim()));
        let is = |name: &str| cmd.eq_ignore_ascii_case(name);
        if is("TITLE") {
            out.title = arg.to_string();
        } else if is("ARTIST") {
            out.artist = arg.to_string();
        } else if is("STAGEFILE") {
            first(&mut out.stagefile, arg);
        } else if is("BANNER") {
            first(&mut out.banner, arg);
        } else if is("BACKBMP") {
            first(&mut out.backbmp, arg);
        } else if is("PREVIEW") {
            first(&mut out.preview, arg);
        } else if let [a, b, c, x, y] = *cmd.as_bytes() {
            let defs = match [a, b, c].map(|c| c.to_ascii_uppercase()) {
                [b'W', b'A', b'V'] => &mut wavs,
                [b'B', b'M', b'P'] => &mut bmps,
                _ => continue,
            };
            if !arg.is_empty() && x.is_ascii_alphanumeric() && y.is_ascii_alphanumeric() {
                defs.insert([x, y].map(|c| c.to_ascii_uppercase()), arg);
            }
        }
    }
    let mut seen = std::collections::HashSet::new();
    out.sounds = wavs
        .into_values()
        .filter(|f| seen.insert(*f))
        .map(String::from)
        .collect();
    for f in bmps.into_values() {
        out.add_bga(f);
    }
    Some(out)
}

fn bmson_info(bytes: &[u8]) -> Option<ChartInfo> {
    let v: serde_json::Value =
        serde_json::from_slice(bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes)).ok()?;
    let info = v.get("info")?;
    let s = |k: &str| {
        info.get(k)
            .and_then(|x| x.as_str())
            .unwrap_or("")
            .to_string()
    };
    let mut out = ChartInfo {
        title: s("title"),
        artist: s("artist"),
        stagefile: s("eyecatch_image"),
        banner: s("banner_image"),
        backbmp: s("back_image"),
        preview: s("preview_music"),
        ..Default::default()
    };
    for key in ["sound_channels", "key_channels", "mine_channels"] {
        for ch in v.get(key).and_then(|c| c.as_array()).into_iter().flatten() {
            if let Some(n) = ch.get("name").and_then(|n| n.as_str()) {
                push(&mut out.sounds, n);
            }
        }
    }
    let headers = v.pointer("/bga/bga_header").and_then(|h| h.as_array());
    for h in headers.into_iter().flatten() {
        if let Some(n) = h.get("name").and_then(|n| n.as_str()) {
            out.add_bga(n);
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bms_and_bmson() {
        let bms = "#TITLE Song [X]\n#artist A\n#WAV02 b.wav\n#WAV01 a.wav\n#RANDOM 2\n#IF 1\n#WAV03 c.ogg\n#ENDIF\n#00111:01\n";
        let i = info(Path::new("x.bme"), bms.as_bytes()).unwrap();
        assert_eq!(i.title, "Song [X]");
        assert_eq!(i.artist, "A");
        assert_eq!(i.sounds, ["a.wav", "b.wav", "c.ogg"]);
        let j = br#"{"info":{"title":"t","artist":"a"},"sound_channels":[{"name":"x.wav","notes":[]},{"name":"x.wav"}],"key_channels":[{"name":"k.wav"}]}"#;
        let i = info(Path::new("x.bmson"), j).unwrap();
        assert_eq!((i.title.as_str(), i.sounds.len()), ("t", 2));
    }

    #[test]
    fn images_movies_and_the_rest() {
        let bms = "#TITLE S\n#STAGEFILE stage.bmp\n#BANNER bn.png\n#BACKBMP back.jpg\n#PREVIEW preview.ogg\n#BMP02 bga/02.bmp\n#BMP01 movie.mpg\n#BMP03 odd.txt\n#BPM 150\n#WAV01 a.wav\n";
        let i = info(Path::new("x.bms"), bms.as_bytes()).unwrap();
        assert_eq!(i.images, ["bga/02.bmp"]);
        assert_eq!(i.movies, ["movie.mpg"]);
        assert_eq!(i.extra_images(), ["stage.bmp", "bn.png", "back.jpg"]);
        assert_eq!(i.preview, "preview.ogg");
        assert_eq!(i.sounds, ["a.wav"]);
        let j = br#"{"info":{"title":"t","eyecatch_image":"e.png","preview_music":"p.ogg"},"bga":{"bga_header":[{"id":1,"name":"a.mp4"},{"id":2,"name":"b.png"}]}}"#;
        let i = info(Path::new("x.bmson"), j).unwrap();
        assert_eq!(
            (i.images, i.movies),
            (vec!["b.png".to_string()], vec!["a.mp4".to_string()])
        );
        assert_eq!(
            (i.stagefile.as_str(), i.preview.as_str()),
            ("e.png", "p.ogg")
        );
        assert_eq!(class_of("Sound/KICK.OGG"), Some(Class::Audio));
        assert_eq!(class_of("x.jpeg"), Some(Class::Image));
        assert_eq!(class_of("x.txt"), None);
        assert_eq!(class_of("noext"), None);
    }
}
