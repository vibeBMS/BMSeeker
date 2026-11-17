//! Titles and artists compared as the BeMusicSeeker fork compares them to
//! tell whether a library folder is a chart's song: both normalized
//! ([`title_key`]: NFKC, the trailing group of brackets / `-…-` / `~…~`
//! cut, keeping up to its last `MIX`; [`artist_key`]: the credits of chart
//! makers, BGA, 差分, feat. ... removed, but not those for the music:
//! remix, arrange, 作曲 ...), then compared by the Dice coefficient of
//! their two-letter pieces. A folder's charts make a [`Profile`] (their
//! most common pair); a chart's song is [`Relation::Strong`] when the
//! artists are the same and the titles' Dice is at least 0.72,
//! [`Relation::Related`] when the Dice of the title or the artist of any
//! of the folder's pairs is at least 0.5 (both, with [`Rule::Both`]).

use std::collections::HashMap;

/// How a folder's song stands to a chart's.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub enum Relation {
    /// No title on one side.
    #[default]
    Unavailable,
    Unrelated,
    /// A title or an artist alike (Dice ≥ 0.5).
    Related,
    /// The same artist, and the titles alike (Dice ≥ 0.72).
    Strong,
}

/// What [`Relation::Related`] asks for.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Rule {
    /// The title or the artist alike.
    #[default]
    Either,
    /// Both (the chart's own files already hold most of its sounds: the
    /// files alone say little).
    Both,
}

/// Dice for [`Relation::Strong`] (titles).
pub const STRONG: f64 = 0.72;
/// Dice for [`Relation::Related`].
pub const RELATED: f64 = 0.5;

/// NFKC, ideographic spaces as spaces, runs of space as one, trimmed.
fn common(s: &str) -> String {
    use unicode_normalization::UnicodeNormalization;
    let s: String = s
        .nfkc()
        .map(|c| if c == '\u{3000}' { ' ' } else { c })
        .collect();
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn closing(c: char) -> Option<char> {
    match c {
        '(' => Some(')'),
        '[' => Some(']'),
        '<' => Some('>'),
        '【' => Some('】'),
        '《' => Some('》'),
        _ => None,
    }
}

fn is_close(c: char) -> bool {
    matches!(c, ')' | ']' | '>' | '】' | '》')
}

/// Where the trailing group of bracketed parts begins (blocks with only
/// spaces between them, the last one at the end or not): `(…)`, `[…]`,
/// `<…>`, `【…】`, `《…》`, `~…~`, `〜…〜`, ` -…-`, a lone closing bracket.
fn suffix_start(t: &[char]) -> Option<usize> {
    let mut blocks: Vec<(usize, usize)> = Vec::new();
    let mut i = 0;
    while i < t.len() {
        let c = t[i];
        let symmetric = c == '~' || c == '〜' || (c == '-' && i > 0 && t[i - 1] == ' ');
        if let Some(close) = closing(c) {
            let start = i;
            let mut open = vec![close];
            i += 1;
            while i < t.len() && !open.is_empty() {
                if let Some(nested) = closing(t[i]) {
                    open.push(nested);
                } else if let Some(at) = open.iter().rposition(|&x| x == t[i]) {
                    // crossed brackets: closed as far as they match
                    open.remove(at);
                    if open.is_empty() {
                        break;
                    }
                }
                i += 1;
            }
            blocks.push((start, (i + 1).min(t.len())));
        } else if symmetric {
            let start = i;
            let find = |from: usize| t[from..].iter().position(|&x| x == c).map(|p| p + from);
            let mut end = find(i + 1);
            // `-J-Trance-`: a hyphen with a letter right after is no end
            while let Some(e) = end {
                if c == '-' && e + 1 < t.len() && t[e + 1].is_alphanumeric() {
                    end = find(e + 1);
                } else {
                    break;
                }
            }
            i = end.unwrap_or(t.len());
            blocks.push((start, (i + 1).min(t.len())));
        } else if is_close(c) {
            blocks.push((i, i + 1));
        }
        i += 1;
    }
    let mut g = blocks.len().checked_sub(1)?;
    while g > 0 && blocks[g - 1].0 > 0 {
        let (end, next) = (blocks[g - 1].1, blocks[g].0);
        if end > next || !t[end..next].iter().all(|c| c.is_whitespace()) {
            break;
        }
        g -= 1;
    }
    Some(blocks[g].0)
}

/// A title as shown, its trailing group of bracketed parts (the chart's
/// name: `[ANOTHER]`, `-7EX-`) cut, but kept up to its last `MIX`.
pub fn title_display(title: &str) -> String {
    let n = common(title);
    let t: Vec<char> = n.chars().collect();
    match suffix_start(&t) {
        Some(cut) if cut > 0 => {
            let lower: Vec<char> = t.iter().map(|c| c.to_ascii_lowercase()).collect();
            let mix = lower.windows(3).rposition(|w| w == ['m', 'i', 'x']);
            let end = match mix {
                Some(m) if m >= cut => m + 3,
                _ => cut,
            };
            t[..end].iter().collect::<String>().trim_end().to_string()
        }
        _ => n,
    }
}

/// A title as compared ([`title_display`], lower case).
pub fn title_key(title: &str) -> String {
    title_display(title).to_lowercase()
}

/// An artist as compared ([`artist_display`], lower case).
pub fn artist_key(artist: &str) -> String {
    artist_display(artist).to_lowercase()
}

// The credits an artist names (as the fork's regular expression, written
// out: the `regex` crate has no look-around). Each list in the fork's
// order: an alternative is tried only when the earlier ones do not fit.
const GENERAL: &[&str] = &[
    "featuring",
    "feat",
    "pmsobj",
    "reobj",
    "obj",
    "odj",
    "bga",
    "bgi",
    "movie",
    "mov",
    "illustration",
    "illust",
    "vocals",
    "vocal",
    "差分",
    "譜面",
];
const SHORT: &[&str] = &["ft", "vo"];
const COMMON: &[&str] = &[
    "notes", "note", "objected", "object", "bg", "bms", "charts", "chart", "graphics", "graphic",
    "sliced", "slice", "guitars", "guitar", "pms", "patterns", "pattern", "sequence", "lyrics",
    "lyric", "image", "artwork", "visual",
];
const MUSIC: &[&str] = &[
    "remixed",
    "remix",
    "mixed",
    "mix",
    "rmx",
    "arrangement",
    "arranged",
    "arrange",
    "arr",
    "covered",
    "cover",
    "composed",
    "composer",
    "compose",
];
const EXPLICIT: &[&str] = &["music", "midi", "作曲", "編曲"];

/// What a credit word must be followed by.
fn sep(c: char) -> bool {
    c.is_whitespace() || matches!(c, '.' | ':' | ';' | ',' | '=' | '/')
}

/// The text `t` from `i`: `word` there (ASCII case aside)? Its end.
fn word_at(t: &[char], i: usize, word: &str) -> Option<usize> {
    let mut j = i;
    for w in word.chars() {
        if j >= t.len() || t[j].to_ascii_lowercase() != w {
            return None;
        }
        j += 1;
    }
    Some(j)
}

/// The end of the spaces from `i`.
fn spaces(t: &[char], mut i: usize) -> usize {
    while i < t.len() && t[i].is_whitespace() {
        i += 1;
    }
    i
}

/// At the end, or a separator next.
fn ends(t: &[char], i: usize) -> bool {
    i >= t.len() || sep(t[i])
}

/// `\s*[.:;,=]` next.
fn punct_next(t: &[char], i: usize) -> bool {
    let j = spaces(t, i);
    j < t.len() && matches!(t[j], '.' | ':' | ';' | ',' | '=')
}

/// `/` next, not followed by a space or another `/`.
fn slash_word(t: &[char], i: usize) -> bool {
    i < t.len()
        && t[i] == '/'
        && (i + 1 >= t.len() || !(t[i + 1].is_whitespace() || t[i + 1] == '/'))
}

/// `\s+by` and a separator or the end next.
fn by_next(t: &[char], i: usize) -> bool {
    let j = spaces(t, i);
    j > i && word_at(t, j, "by").is_some_and(|e| ends(t, e))
}

/// The role names that fit at `i`, in the fork's order: (end, for the
/// music).
fn role_names(t: &[char], i: usize) -> Vec<(usize, bool)> {
    let lists: [(&[&str], bool); 5] = [
        (MUSIC, true),
        (EXPLICIT, true),
        (GENERAL, false),
        (SHORT, false),
        (COMMON, false),
    ];
    lists
        .iter()
        .flat_map(|(l, music)| {
            l.iter()
                .filter_map(move |w| word_at(t, i, w).map(|e| (e, *music)))
        })
        .collect()
}

/// Roles joined (`obj/bga`, `BGA & obj`), from `i`: as many as are
/// joined, ending at a separator (the first way that does, as a regular
/// expression finds it), and whether one of them is for the music.
fn joined(t: &[char], i: usize, count: usize, music: bool) -> Option<(usize, bool)> {
    for (e, m) in role_names(t, i) {
        let music = music || m;
        // `\s*[&+]\s*` or `/`, then another
        let j = spaces(t, e);
        let next = if j < t.len() && matches!(t[j], '&' | '+') {
            Some(spaces(t, j + 1))
        } else if e < t.len() && t[e] == '/' {
            Some(e + 1)
        } else {
            None
        };
        if let Some(found) = next.and_then(|n| joined(t, n, count + 1, music)) {
            return Some(found);
        }
        if count >= 1 && ends(t, e) {
            return Some((e, music));
        }
    }
    None
}

/// A credit at `i` (the fork's role pattern): its end (separators and a
/// `by` after it taken too) and whether it is for the music.
fn credit_at(t: &[char], i: usize) -> Option<(usize, bool)> {
    let boundary = |c: char| c.is_ascii_alphanumeric() || c == '_' || c == '-';
    if i > 0 && boundary(t[i - 1]) {
        return None;
    }
    let s = if i < t.len() && t[i] == '-' { i + 1 } else { i };
    let first = |words: &[&str], fits: &dyn Fn(usize) -> bool| {
        words
            .iter()
            .filter_map(|w| word_at(t, s, w))
            .find(|&e| fits(e))
    };
    let found = joined(t, s, 0, false)
        .or_else(|| first(MUSIC, &|e| ends(t, e)).map(|e| (e, true)))
        .or_else(|| {
            first(EXPLICIT, &|e| {
                punct_next(t, e) || slash_or_sep(t, e) || by_next(t, e)
            })
            .map(|e| (e, true))
        })
        .or_else(|| first(GENERAL, &|e| e < t.len() && sep(t[e])).map(|e| (e, false)))
        .or_else(|| first(SHORT, &|e| punct_next(t, e) || slash_word(t, e)).map(|e| (e, false)))
        .or_else(|| {
            first(COMMON, &|e| {
                punct_next(t, e) || slash_word(t, e) || by_next(t, e)
            })
            .map(|e| (e, false))
        });
    let (mut e, music) = found?;
    while e < t.len() && sep(t[e]) {
        e += 1;
    }
    if let Some(b) = word_at(t, e, "by").filter(|&b| ends(t, b)) {
        e = b;
        while e < t.len() && sep(t[e]) {
            e += 1;
        }
    }
    Some((e, music))
}

/// `\s*[.:;,=/]` next (an explicit music credit: `music: X`, `作曲/X`).
fn slash_or_sep(t: &[char], i: usize) -> bool {
    let j = spaces(t, i);
    j < t.len() && t[j] == '/'
}

/// An artist as shown, the credits for anything but the music removed
/// (`alpha / obj: x` is `alpha`; `alpha feat. beta` is `alpha`; `remix:
/// gamma` stays).
pub fn artist_display(artist: &str) -> String {
    let n = common(artist);
    let t: Vec<char> = n.chars().collect();
    // the credits, as a search finds them (one after another)
    let mut credits = Vec::new();
    let mut i = 0;
    while i < t.len() {
        match credit_at(&t, i) {
            Some((e, music)) if e > i => {
                credits.push((i, e, music));
                i = e;
            }
            _ => i += 1,
        }
    }
    let mut out = String::new();
    let mut open: Vec<char> = Vec::new();
    let (mut start, mut keep) = (0, true);
    let mut credits = credits.into_iter().peekable();
    let mut i = 0;
    while i < t.len() {
        if let Some(&(at, end, music)) = credits.peek() {
            if at == i {
                credits.next();
                if keep {
                    append(&mut out, &t, start, i);
                }
                // a credit for the music starts a part kept
                keep = music;
                start = i;
                i = end;
                continue;
            }
        }
        let c = t[i];
        let slash = c == '/'
            && i > 0
            && open.is_empty()
            && (matches!(t[i - 1], ' ' | '/')
                || (i + 1 < t.len() && matches!(t[i + 1], ' ' | '/')));
        if slash {
            if keep {
                append(&mut out, &t, start, i);
            }
            keep = false;
        }
        if let Some(close) = closing(c) {
            open.push(close);
        } else if is_close(c) {
            if let Some(at) = open.iter().rposition(|&x| x == c) {
                open.remove(at);
            }
        }
        i += 1;
    }
    if keep {
        append(&mut out, &t, start, t.len());
    }
    out
}

/// `t[start..end]` onto `out` (a space between), its separators at the end
/// trimmed.
fn append(out: &mut String, t: &[char], start: usize, end: usize) {
    let end = trim_separators(t, start, end);
    if end <= start {
        return;
    }
    if !out.is_empty() {
        out.push(' ');
    }
    out.extend(&t[start..end]);
}

fn trim_separators(t: &[char], start: usize, mut end: usize) -> usize {
    while end > start {
        let before = end;
        while end > start
            && (matches!(t[end - 1], ' ' | '/')
                || is_close(t[end - 1])
                || closing(t[end - 1]).is_some())
        {
            end -= 1;
        }
        let mut s = end;
        while s > start
            && matches!(
                t[s - 1],
                '.' | ':' | ';' | ',' | '=' | '/' | '|' | '&' | '+' | '-'
            )
        {
            s -= 1;
        }
        // `SKY.`: a mark joined to a name stays; one on its own goes
        if s < end && (s == start || t[s - 1] == ' ') {
            end = s;
        }
        if end == before {
            break;
        }
    }
    end
}

/// The two-letter pieces of `s`, counted (one letter: itself).
fn bigrams(s: &str) -> HashMap<(char, char), usize> {
    let c: Vec<char> = s.chars().collect();
    let mut out = HashMap::new();
    if c.len() == 1 {
        out.insert((c[0], '\0'), 1);
    }
    for w in c.windows(2) {
        *out.entry((w[0], w[1])).or_default() += 1;
    }
    out
}

/// The Dice coefficient of `a` and `b` (normalized) by their two-letter
/// pieces: 1 for the same, 0 when either is empty.
pub fn dice(a: &str, b: &str) -> f64 {
    if a.trim().is_empty() || b.trim().is_empty() {
        return 0.0;
    }
    if a == b {
        return 1.0;
    }
    let (x, y) = (bigrams(a), bigrams(b));
    let overlap: usize = x
        .iter()
        .map(|(k, n)| (*n).min(*y.get(k).unwrap_or(&0)))
        .sum();
    let total: usize = x.values().sum::<usize>() + y.values().sum::<usize>();
    2.0 * overlap as f64 / total as f64
}

/// The (title, artist) pairs of a song's charts, as compared: the most
/// common first (then by path, then as given).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Profile {
    pub pairs: Vec<(String, String)>,
}

impl Profile {
    /// Of charts' (title, artist, path); charts without a title count not.
    pub fn of<'a>(charts: impl IntoIterator<Item = (&'a str, &'a str, &'a str)>) -> Profile {
        // (pair, count, first path, first index)
        let mut groups: Vec<((String, String), usize, String, usize)> = Vec::new();
        for (i, (title, artist, path)) in charts.into_iter().enumerate() {
            let t = title_key(title);
            if t.is_empty() {
                continue;
            }
            let pair = (t, artist_key(artist));
            match groups.iter_mut().find(|g| g.0 == pair) {
                Some(g) => {
                    g.1 += 1;
                    if path.to_lowercase() < g.2.to_lowercase() {
                        g.2 = path.to_string();
                    }
                }
                None => groups.push((pair, 1, path.to_string(), i)),
            }
        }
        groups.sort_by(|a, b| {
            b.1.cmp(&a.1)
                .then_with(|| a.2.to_lowercase().cmp(&b.2.to_lowercase()))
                .then(a.3.cmp(&b.3))
        });
        Profile {
            pairs: groups.into_iter().map(|g| g.0).collect(),
        }
    }

    /// Of one chart.
    pub fn one(title: &str, artist: &str) -> Profile {
        Profile::of([(title, artist, "")])
    }

    /// The most common pair.
    pub fn dominant(&self) -> Option<&(String, String)> {
        self.pairs.first()
    }

    /// How the song of `other` (a folder's charts) stands to this one (a
    /// chart's): [`Relation::Strong`] by the two most common pairs; else
    /// [`Relation::Related`] when any of `other`'s pairs is alike by
    /// `rule`.
    pub fn relation(&self, other: &Profile, rule: Rule) -> Relation {
        let (Some((t, a)), Some((ot, oa))) = (self.dominant(), other.dominant()) else {
            return Relation::Unavailable;
        };
        if a == oa && dice(t, ot) >= STRONG {
            return Relation::Strong;
        }
        let related = other.pairs.iter().any(|(ot, oa)| {
            let (x, y) = (dice(t, ot) >= RELATED, dice(a, oa) >= RELATED);
            match rule {
                Rule::Either => x || y,
                Rule::Both => x && y,
            }
        });
        if related {
            Relation::Related
        } else {
            Relation::Unrelated
        }
    }
}

/// Whether two charts are of one song by [`Relation::Strong`].
pub fn same_song(title: &str, artist: &str, other_title: &str, other_artist: &str) -> bool {
    Profile::one(title, artist).relation(&Profile::one(other_title, other_artist), Rule::Either)
        == Relation::Strong
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn titles() {
        let t = |s: &str| title_display(s);
        assert_eq!(t("Mirage Garden [ANOTHER]"), "Mirage Garden");
        assert_eq!(t("Arcadia -★1-"), "Arcadia");
        assert_eq!(t("CG901B -7EX-[Short Ver.]"), "CG901B");
        assert_eq!(t("Song (7KEYS) [EX]"), "Song");
        // only the trailing group: a title's own brackets stay
        assert_eq!(t("[ASIA] Song (HYPER)"), "[ASIA] Song");
        assert_eq!(t("(the) Song"), "(the) Song");
        // kept up to its last MIX
        assert_eq!(t("Song (Hoge Remix) [A]"), "Song (Hoge Remix");
        assert_eq!(t("Song -J-Trance Mix-"), "Song -J-Trance Mix");
        // NFKC; ideographic spaces
        assert_eq!(t("ＡＢＣ\u{3000}ｄｅｆ  [X]"), "ABC def");
        assert_eq!(t("Song ~Long Ver.~"), "Song");
        assert_eq!(title_key("SONG [x]"), "song");
        assert_eq!(t(""), "");
    }

    #[test]
    fn artists() {
        let a = |s: &str| artist_display(s);
        assert_eq!(a("alpha / obj: x"), "alpha");
        assert_eq!(a("void obj.Cloudy Boy"), "void");
        assert_eq!(a("ねこみみ魔法使い / obj:秘密"), "ねこみみ魔法使い");
        assert_eq!(a("Squar / 差分 赤い人"), "Squar");
        assert_eq!(a("alpha feat. beta"), "alpha");
        assert_eq!(a("alpha (BGA: gamma)"), "alpha");
        assert_eq!(a("alpha obj/bga: x"), "alpha");
        // music credits stay
        assert_eq!(a("alpha remix: beta"), "alpha remix: beta");
        assert_eq!(a("作曲: alpha / 譜面: beta"), "作曲: alpha");
        // a name of its own is no credit
        assert_eq!(a("Frontier Lyric"), "Frontier Lyric");
        assert_eq!(a("SKY."), "SKY.");
        assert_eq!(a("objective"), "objective");
        assert_eq!(artist_key("ALPHA / obj"), "alpha");
    }

    #[test]
    fn dice_and_relations() {
        assert_eq!(dice("song", "song"), 1.0);
        assert_eq!(dice("", "song"), 0.0);
        assert!((dice("night", "nacht") - 0.25).abs() < 1e-9);
        assert_eq!(dice("a", "a"), 1.0);
        let chart = Profile::one("Song [ANOTHER]", "alpha / obj: x");
        let folder = Profile::of([
            ("Song [NORMAL]", "alpha", "n.bms"),
            ("Song [HYPER]", "alpha", "h.bms"),
            ("Song [B]", "beta", "b.bms"),
        ]);
        assert_eq!(folder.dominant().unwrap(), &("song".into(), "alpha".into()));
        assert_eq!(chart.relation(&folder, Rule::Either), Relation::Strong);
        // another artist's song of a like title: related
        let other = Profile::one("Songs", "gamma");
        assert_eq!(chart.relation(&other, Rule::Either), Relation::Related);
        assert_eq!(chart.relation(&other, Rule::Both), Relation::Unrelated);
        let far = Profile::one("Zzz", "q");
        assert_eq!(chart.relation(&far, Rule::Either), Relation::Unrelated);
        assert_eq!(
            chart.relation(&Profile::default(), Rule::Either),
            Relation::Unavailable
        );
        assert!(same_song("Song [X]", "alpha / obj: b", "Song", "alpha"));
        assert!(!same_song("Song", "alpha", "Song", "beta"));
        assert!(!same_song("Love Story", "a", "Lovely Day", "a"));
    }
}
