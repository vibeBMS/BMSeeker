//! A song's folder named after its charts, as the BeMusicSeeker fork names
//! one (`[%ARTIST%] %TITLE%`): the longest part the charts' titles (and
//! artists) begin with alike, their chart names, prefixes and credits
//! trimmed, the best by its length^0.65 × how many charts give it; a file
//! system's limits kept (128 bytes a folder, 250 a path, as Shift_JIS
//! counts them); `新しいフォルダー` when nothing is left.
//!
//! Derived from the BeMusicSeeker unofficial fork (MIT; copyright notices
//! and licence text in THIRD_PARTY_NOTICES.md).

use crate::chart::ChartInfo;
use crate::text::pattern::Pattern;
use std::path::Path;
use std::sync::LazyLock;

/// The name of a folder with no name of its own (the fork's).
pub(crate) const NEW_FOLDER: &str = "新しいフォルダー";
/// The fork's default format.
pub(crate) const DEFAULT_FORMAT: &str = "[%ARTIST%] %TITLE%";
/// Bytes (Shift_JIS) a folder's name may have.
pub(crate) const MAX_NAME: usize = 128;
/// Bytes (Shift_JIS) a folder's whole path may have.
pub(crate) const MAX_PATH: usize = 250;

fn re(s: &str) -> Pattern {
    Pattern::new(s)
}

// The fork's patterns (BMSLibrary.cs).
static TRIM_START: LazyLock<Pattern> =
    LazyLock::new(|| re(r"(?i)^(\d+(S|D)P|midi|bms|music)[.:・\s]+"));
static TRIM_END: LazyLock<Pattern> =
    LazyLock::new(|| re(r##"(^|\s+)[\-!"#$%&`'(,./:;<=>?@\[^_~|]+$"##));
static TRIM_END_LEVEL: LazyLock<Pattern> =
    LazyLock::new(|| re(r"(?i)\s?[\[(](\d+|H|SP?|\d+key[^\s]*|hard])?$"));
static CREDITS: LazyLock<Pattern> = LazyLock::new(|| {
    re(
        r"(?i)(^|[\[［/(（<＜\s]+)((.?obj|mov&obj|bga|midi|movie|object|mov|image|bgi|illust(ration|rated)?|差分|LYRICS|イラスト|絵|#include)([.:・\s]+.*$|$)|(bms|layer|visual):.*$)",
    )
});
static SPACED_MARKS: LazyLock<Pattern> = LazyLock::new(|| {
    re("\\s+[\u{3000}！“”＃＄％＆‘’（）＊＋，－．／：；＜＝＞？＠［￥］\u{ff3e}\u{ff3f}\u{ffe3}]+\\s+")
});
static TORTOISE: LazyLock<Pattern> = LazyLock::new(|| re("〔(.*)〕"));
static SPACES: LazyLock<Pattern> = LazyLock::new(|| re(r"\s{2,}"));
static END_BRACKETS: LazyLock<Pattern> = LazyLock::new(|| {
    re(
        r#"\s*([-].*[-]|[`'].*[`']|["].*["]|[～].*[～]|[－].*[－]|[‘’].*[‘’]|[“”].*[“”]|[<].*[>]|[{].*[}]|[\(].*[\)]|[\[].*[\]]|[＜].*[＞]|[（].*[）]|[「].*[」]|[【].*[】]|[『].*[』]|[［].*[］]|[〈].*[〉]|[《].*[》]|[〔].*[〕]|[｛].*[｝])$"#,
    )
});
static LEVEL_WORDS: LazyLock<Pattern> = LazyLock::new(|| {
    re(
        r"(?i)(PMS|SP|ANOTHER|HYPER|NORMAL|EMPTY|DP|BGA|4[^\d]|5[^\d]|7[^\d]|9[^\d]|10[^\d]|14[^\d])[^\w]+",
    )
});

/// Characters a file name cannot have (Windows').
fn invalid(c: char) -> bool {
    matches!(c, '"' | '<' | '>' | '|' | ':' | '*' | '?' | '\\' | '/') || c.is_control()
}

/// Wide ASCII and spaces narrow, narrow kana wide (NFKC), spaces as one.
fn natural(s: &str) -> String {
    use unicode_normalization::UnicodeNormalization;
    let s: String = s.nfkc().collect();
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// `s` without a bracketed part at its end that names a chart (`[ANOTHER]`,
/// `(7keys)`).
fn cut_chart_name(s: &str) -> String {
    if let Some(m) = END_BRACKETS.find(s) {
        if LEVEL_WORDS.is_match(&s[m.clone()]) {
            return s[..m.start].to_string();
        }
    }
    s.to_string()
}

/// The common start of `a` and `b`.
fn common_start(a: &str, b: &str) -> String {
    a.chars()
        .zip(b.chars())
        .take_while(|(x, y)| x == y)
        .map(|(x, _)| x)
        .collect()
}

/// A candidate tidied: prefixes, credits and level tails trimmed, the
/// characters a file name cannot have made wide.
fn tidy(s: &str) -> String {
    let s = TRIM_START.remove_first(s);
    let s = CREDITS.remove_first(s.trim());
    let s = TRIM_END.remove_first(&s);
    let s = TRIM_END_LEVEL.remove_first(&s);
    let s: String = s
        .chars()
        .filter_map(|c| match c {
            c if c.is_control() => None,
            // full width: U+FF01.. for '!'..
            c if invalid(c) => char::from_u32(c as u32 - 0x21 + 0xFF01),
            c => Some(c),
        })
        .collect();
    let s = SPACED_MARKS.replace_all(&s, |m| m.trim().to_string());
    // (〔…〕)
    let s = TORTOISE.replace_all(&s, |m| format!("[{}]", m[3..m.len() - 3].trim()));
    SPACES.replace_all(&s, |_| " ".into()).trim().to_string()
}

/// The longest part `strings` (charts' titles, or artists) begin with
/// alike: of each two, the start they share (the first's chart name cut);
/// none of two letters: each one, its chart name cut. Tidied (prefixes, credits, level tails),
/// the best is the one with the most of length^0.65 × count.
pub(crate) fn longest_common(strings: &[&str]) -> String {
    let given: Vec<&str> = strings
        .iter()
        .copied()
        .filter(|s| !s.trim().is_empty())
        .collect();
    let mut list: Vec<String> = Vec::new();
    for (i, a) in given.iter().enumerate() {
        for b in &given[i + 1..] {
            let a = cut_chart_name(&natural(a));
            let c = common_start(&a, &natural(b)).trim().to_string();
            if c.chars().count() > 1 {
                list.push(c);
            }
        }
    }
    if list.is_empty() {
        list = given.iter().map(|s| cut_chart_name(&natural(s))).collect();
    }
    // (name, count) in the order first met
    let mut groups: Vec<(String, usize)> = Vec::new();
    for s in list.iter().map(|s| tidy(s)) {
        match groups.iter_mut().find(|g| g.0 == s) {
            Some(g) => g.1 += 1,
            None => groups.push((s, 1)),
        }
    }
    let score = |g: &(String, usize)| (g.0.chars().count() as f64).powf(0.65) * g.1 as f64;
    let mut best: Option<&(String, usize)> = None;
    for g in &groups {
        if best.is_none_or(|b| score(g) > score(b)) {
            best = Some(g);
        }
    }
    best.map(|b| b.0.clone()).unwrap_or_default()
}

/// Bytes `s` takes in Shift_JIS (a character it cannot have: as UTF-8).
pub(crate) fn sjis_len(s: &str) -> usize {
    let mut buf = [0u8; 4];
    s.chars()
        .map(|c| {
            let one = c.encode_utf8(&mut buf);
            let (b, _, bad) = encoding_rs::SHIFT_JIS.encode(one);
            if bad {
                one.len()
            } else {
                b.len()
            }
        })
        .sum()
}

/// `s` with only the characters Shift_JIS has.
fn sjis_only(s: &str) -> String {
    let mut buf = [0u8; 4];
    s.chars()
        .filter(|c| !encoding_rs::SHIFT_JIS.encode(c.encode_utf8(&mut buf)).2)
        .collect()
}

/// `s` as a folder's name: no character a file name cannot have, no space
/// or dot at either end, not a name Windows keeps for itself.
fn valid(s: &str) -> String {
    let s: String = s.chars().filter(|c| !invalid(*c)).collect();
    let s = s.trim_matches(|c: char| c.is_whitespace() || c == '.' || c == '\u{3000}');
    let upper = s.to_ascii_uppercase();
    let stem = upper.split('.').next().unwrap_or("");
    let reserved = matches!(stem, "CON" | "PRN" | "AUX" | "NUL" | "CLOCK$")
        || (stem.len() == 4
            && (stem.starts_with("COM") || stem.starts_with("LPT"))
            && stem.as_bytes()[3].is_ascii_digit());
    if reserved {
        // (`CON.x` is `x`)
        return s
            .split_once('.')
            .map(|(_, r)| r.trim_start_matches('.'))
            .unwrap_or("")
            .into();
    }
    s.to_string()
}

/// `s` cut to `max` bytes ([`sjis_len`]), a character at a time.
fn fit(s: &str, max: usize) -> String {
    let mut s: Vec<char> = s.chars().collect();
    while s.len() > 1 && sjis_len(&s.iter().collect::<String>()) > max {
        s.pop();
    }
    s.into_iter().collect::<String>().trim_end().to_string()
}

/// The name of a song's folder for its charts `charts`, by `format`
/// (`%ARTIST%`, `%TITLE%`: what the charts' artists and titles share,
/// [`longest_common`]); `sjis_only`: only characters Shift_JIS has (for LR2
/// on Windows); [`MAX_NAME`] bytes at most; [`NEW_FOLDER`] when nothing is
/// left.
pub(crate) fn folder_name(charts: &[ChartInfo], format: &str, sjis_only_chars: bool) -> String {
    let titles: Vec<&str> = charts.iter().map(|c| c.title.as_str()).collect();
    let artists: Vec<&str> = charts.iter().map(|c| c.artist.as_str()).collect();
    let title = longest_common(&titles);
    let artist = longest_common(&artists);
    let mut s = format
        .replace("%ARTIST%", &artist)
        .replace("%TITLE%", &title)
        .trim()
        .to_string();
    if artist.is_empty() {
        // (no artist: no empty brackets)
        s = s.replace("[]", "").replace("()", "").trim().to_string();
    }
    if sjis_only_chars {
        s = sjis_only(&s);
    }
    let s = valid(&s);
    let s = if s.trim().is_empty() {
        NEW_FOLDER.to_string()
    } else {
        s
    };
    fit(&s, MAX_NAME)
}

/// `name` cut so that `parent` and it take [`MAX_PATH`] bytes at most.
pub(crate) fn fit_path(parent: &Path, name: &str) -> String {
    let room = MAX_PATH.saturating_sub(sjis_len(&parent.to_string_lossy()) + 1);
    match fit(name, room.max(1)) {
        n if n.is_empty() => NEW_FOLDER.into(),
        n => n,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chart(title: &str, artist: &str) -> ChartInfo {
        ChartInfo {
            title: title.into(),
            artist: artist.into(),
            ..Default::default()
        }
    }

    #[test]
    fn common_parts() {
        assert_eq!(
            longest_common(&["Song [NORMAL]", "Song [HYPER]", "Song [ANOTHER]"]),
            "Song"
        );
        // one chart: its own title, its chart name cut
        assert_eq!(
            longest_common(&["Mirage Garden [ANOTHER]"]),
            "Mirage Garden"
        );
        assert_eq!(longest_common(&["Song (7keys)"]), "Song");
        // credits and prefixes
        assert_eq!(
            longest_common(&["alpha / obj: x", "alpha / obj: y"]),
            "alpha"
        );
        assert_eq!(longest_common(&["alpha obj:x"]), "alpha");
        assert_eq!(longest_common(&["BMS: Song"]), "Song");
        // a file name's characters made wide
        assert_eq!(longest_common(&["A/B: C?"]), "A／B： C？");
        assert_eq!(longest_common(&["ＡＢＣ"]), "ABC");
        assert_eq!(longest_common(&[]), "");
        // the longer shared by more wins
        assert_eq!(
            longest_common(&["Longer Title [A]", "Longer Title [B]", "Lo"]),
            "Longer Title"
        );
    }

    #[test]
    fn folder_names() {
        let charts = [
            chart("Song [NORMAL]", "alpha"),
            chart("Song [ANOTHER]", "alpha / obj: x"),
        ];
        assert_eq!(folder_name(&charts, DEFAULT_FORMAT, false), "[alpha] Song");
        assert_eq!(folder_name(&charts, "%TITLE%", false), "Song");
        assert_eq!(folder_name(&[], DEFAULT_FORMAT, false), NEW_FOLDER);
        assert_eq!(
            folder_name(&[chart("Song", "")], DEFAULT_FORMAT, false),
            "Song"
        );
        // Shift_JIS only: what it lacks goes
        assert_eq!(
            folder_name(&[chart("Song 한", "a")], DEFAULT_FORMAT, true),
            "[a] Song"
        );
        // 128 bytes at most (a kanji: two)
        let long = "曲".repeat(100);
        let n = folder_name(&[chart(&long, "a")], "%TITLE%", false);
        assert_eq!(sjis_len(&n), 128);
        assert_eq!(valid(" .name. "), "name");
        assert_eq!(valid("CON"), "");
        assert_eq!(valid("com1.txt"), "txt");
        let parent = Path::new("/").join("x".repeat(240));
        assert_eq!(fit_path(&parent, "abcdefghijkl"), "abcdefgh");
    }
}
