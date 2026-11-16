//! What bmseeker needs of a chart: its title, artist and the sound files it
//! uses (to tell which song's folder it belongs in). BMS text (`#TITLE`,
//! `#ARTIST`, `#WAVxx`, every `#RANDOM` branch's) and bmson.

use std::path::Path;

/// The title, artist and sounds of a chart file.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ChartInfo {
    pub title: String,
    pub artist: String,
    /// Sound files it uses (names as written in the chart).
    pub sounds: Vec<String>,
}

/// `bytes` of the chart file `path` (bmson by its extension, else BMS).
pub fn info(path: &Path, bytes: &[u8]) -> Option<ChartInfo> {
    let bmson = path
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("bmson"));
    if bmson {
        return bmson_info(bytes);
    }
    let text = crate::text::decode_text(bytes);
    let mut out = ChartInfo::default();
    // #WAVxx in id order, each once
    let mut wavs: Vec<(String, String)> = Vec::new();
    for line in text.lines() {
        let Some(l) = line.trim_start().strip_prefix('#') else {
            continue;
        };
        let (cmd, arg) = l
            .split_once(|c: char| c.is_whitespace())
            .map_or((l, ""), |(c, a)| (c, a.trim()));
        let cmd = cmd.to_ascii_uppercase();
        match cmd.as_str() {
            "TITLE" => out.title = arg.to_string(),
            "ARTIST" => out.artist = arg.to_string(),
            _ => {
                if let Some(id) = cmd.strip_prefix("WAV").filter(|id| id.len() == 2) {
                    if !arg.is_empty() && id.chars().all(|c| c.is_ascii_alphanumeric()) {
                        match wavs.iter_mut().find(|(i, _)| i == id) {
                            Some(w) => w.1 = arg.to_string(),
                            None => wavs.push((id.to_string(), arg.to_string())),
                        }
                    }
                }
            }
        }
    }
    wavs.sort_by(|a, b| a.0.cmp(&b.0));
    for (_, f) in wavs {
        if !out.sounds.contains(&f) {
            out.sounds.push(f);
        }
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
    let mut sounds: Vec<String> = Vec::new();
    for key in ["sound_channels", "key_channels", "mine_channels"] {
        for ch in v.get(key).and_then(|c| c.as_array()).into_iter().flatten() {
            if let Some(n) = ch.get("name").and_then(|n| n.as_str()) {
                if !n.is_empty() && !sounds.iter().any(|x| x == n) {
                    sounds.push(n.to_string());
                }
            }
        }
    }
    Some(ChartInfo {
        title: s("title"),
        artist: s("artist"),
        sounds,
    })
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
}
