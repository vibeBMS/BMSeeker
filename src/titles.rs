//! Titles and artists compared as the BeMusicSeeker fork compares them, to
//! tell whether two charts are of one song ([`same_song`]: whether a
//! library folder or a table's entry is a chart's song) and whether a
//! folder's charts are one work ([`Song`] alike): both normalized
//! ([`title_key`]: NFKC, the trailing group of brackets / `-…-` / `~…~`
//! cut, keeping up to its last `MIX`; [`artist_key`]: the credits of chart
//! makers, BGA, 差分, feat. ... removed, but not those for the music:
//! remix, arrange, 作曲 ...), then compared by the Dice coefficient of
//! their two-letter pieces. A folder's charts make a [`Profile`] (their
//! most common pair); a chart's song is [`Relation::Strong`] when the
//! artists are the same and the titles' Dice is at least 0.72,
//! [`Relation::Related`] when the Dice of the title or the artist of any
//! of the folder's pairs is at least 0.5 (both, with [`Rule::Both`]).
//!
//! Derived from the BeMusicSeeker unofficial fork (MIT; copyright notices
//! and licence text in THIRD_PARTY_NOTICES.md).

/// How a folder's song stands to a chart's.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Relation {
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
pub(crate) enum Rule {
    /// The title or the artist alike.
    #[default]
    Either,
    /// Both (the chart's own files already hold most of its sounds: the
    /// files alone say little).
    Both,
}

/// Dice for [`Relation::Strong`] (titles).
pub(crate) const STRONG: f64 = 0.72;
/// Dice for [`Relation::Related`].
pub(crate) const RELATED: f64 = 0.5;

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
pub(crate) fn title_display(title: &str) -> String {
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
pub(crate) fn title_key(title: &str) -> String {
    title_display(title).to_lowercase()
}

/// An artist as compared ([`artist_display`], lower case).
pub(crate) fn artist_key(artist: &str) -> String {
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

/// `\s*/` next (with [`punct_next`], what ends an explicit music credit:
/// `music: X`, `作曲/X`).
fn slash_or_sep(t: &[char], i: usize) -> bool {
    let j = spaces(t, i);
    j < t.len() && t[j] == '/'
}

/// An artist as shown, the credits for anything but the music removed
/// (`alpha / obj: x` is `alpha`; `alpha feat. beta` is `alpha`; `remix:
/// gamma` stays).
pub(crate) fn artist_display(artist: &str) -> String {
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

/// The two-letter pieces of `s`, sorted (one letter: itself).
fn bigrams(s: &str) -> Vec<(char, char)> {
    let c: Vec<char> = s.chars().collect();
    let mut out: Vec<(char, char)> = match c[..] {
        [one] => vec![(one, '\0')],
        _ => c.windows(2).map(|w| (w[0], w[1])).collect(),
    };
    out.sort_unstable();
    out
}

/// The Dice coefficient of `a` and `b` (normalized) by their two-letter
/// pieces: 1 for the same, 0 when either is empty.
pub(crate) fn dice(a: &str, b: &str) -> f64 {
    if a.trim().is_empty() || b.trim().is_empty() {
        return 0.0;
    }
    if a == b {
        return 1.0;
    }
    let (x, y) = (bigrams(a), bigrams(b));
    // the pieces both have, each as often as the one that has it less
    let (mut i, mut j, mut overlap) = (0, 0, 0);
    while i < x.len() && j < y.len() {
        match x[i].cmp(&y[j]) {
            std::cmp::Ordering::Less => i += 1,
            std::cmp::Ordering::Greater => j += 1,
            std::cmp::Ordering::Equal => (i, j, overlap) = (i + 1, j + 1, overlap + 1),
        }
    }
    2.0 * overlap as f64 / (x.len() + y.len()) as f64
}

/// A chart's title and artist as compared ([`title_key`], [`artist_key`]).
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub(crate) struct Song {
    pub(crate) title: String,
    pub(crate) artist: String,
}

impl Song {
    pub(crate) fn of(title: &str, artist: &str) -> Song {
        Song {
            title: title_key(title),
            artist: artist_key(artist),
        }
    }
}

/// Whether `a` and `b` are one song's charts ([`Relation::Strong`]): a
/// title, the same artist, and the titles alike (Dice ≥ [`STRONG`]).
pub(crate) fn same_song(a: &Song, b: &Song) -> bool {
    !a.title.is_empty() && a.artist == b.artist && dice(&a.title, &b.title) >= STRONG
}

/// The (title, artist) pairs of a song's charts, as compared: the most
/// common first (then by path, then as given).
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Profile {
    pub(crate) pairs: Vec<Song>,
}

impl Profile {
    /// Of charts' (title, artist, path); charts without a title count not.
    pub(crate) fn of<'a>(charts: impl IntoIterator<Item = (&'a str, &'a str, &'a str)>) -> Profile {
        // (pair, count, first path, first index)
        let mut groups: Vec<(Song, usize, String, usize)> = Vec::new();
        for (i, (title, artist, path)) in charts.into_iter().enumerate() {
            let pair = Song::of(title, artist);
            if pair.title.is_empty() {
                continue;
            }
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
    pub(crate) fn one(title: &str, artist: &str) -> Profile {
        Profile::of([(title, artist, "")])
    }

    /// The most common pair.
    pub(crate) fn dominant(&self) -> Option<&Song> {
        self.pairs.first()
    }

    /// How the song of `other` (a folder's charts) stands to this one (a
    /// chart's): [`Relation::Strong`] by the two most common pairs; else
    /// [`Relation::Related`] when any of `other`'s pairs is alike by
    /// `rule`.
    pub(crate) fn relation(&self, other: &Profile, rule: Rule) -> Relation {
        let (Some(s), Some(o)) = (self.dominant(), other.dominant()) else {
            return Relation::Unavailable;
        };
        if same_song(s, o) {
            return Relation::Strong;
        }
        let related = other.pairs.iter().any(|o| {
            let x = dice(&s.title, &o.title) >= RELATED;
            let y = dice(&s.artist, &o.artist) >= RELATED;
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

// Names (an archive's in a pack, a search's results) matched to a song's
// title and artist by what they start with. Whether two charts are of one
// song is [`same_song`].

/// `name` without bracketed parts (`[ANOTHER]`, `(obj: ...)`, `【...】`),
/// lower case, spaces collapsed.
fn strip_brackets(name: &str) -> String {
    let mut out = String::new();
    let mut depth = 0usize;
    for c in name.chars() {
        match c {
            '[' | '(' | '【' | '（' | '［' | '<' | '〔' => depth += 1,
            ']' | ')' | '】' | '）' | '］' | '>' | '〕' => depth = depth.saturating_sub(1),
            _ if depth == 0 => out.extend(c.to_lowercase()),
            _ => {}
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Titles match when one starts with the other once tags are gone
/// ("Arcadia -★1-" and "Arcadia"); the shorter needs three characters.
pub fn same_title(a: &str, b: &str) -> bool {
    let (a, b) = (strip_brackets(a), strip_brackets(b));
    let (short, long) = if a.chars().count() <= b.chars().count() {
        (a, b)
    } else {
        (b, a)
    };
    short.chars().count() >= 3 && long.starts_with(&short)
}

/// The same title, but for its brackets (`Song [ANOTHER]` is `Song`).
pub fn same_name(a: &str, b: &str) -> bool {
    let a = strip_brackets(a);
    !a.is_empty() && a == strip_brackets(b)
}

/// Artists match without the chart maker's part ("artist / obj: someone").
pub fn same_artist(a: &str, b: &str) -> bool {
    let clean = |s: &str| {
        let s = s.to_lowercase();
        let cut = ["/", "obj", "差分", "譜面"]
            .iter()
            .filter_map(|m| s.find(m))
            .min()
            .unwrap_or(s.len());
        strip_brackets(&s[..cut])
    };
    let (a, b) = (clean(a), clean(b));
    if a.is_empty() || b.is_empty() {
        return a == b;
    }
    let (short, long) = if a.len() <= b.len() { (a, b) } else { (b, a) };
    long.starts_with(&short)
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
        assert_eq!(folder.dominant().unwrap(), &Song::of("song", "alpha"));
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
        let same = |t: &str, a: &str, u: &str, b: &str| same_song(&Song::of(t, a), &Song::of(u, b));
        assert!(same("Song [X]", "alpha / obj: b", "Song", "alpha"));
        assert!(!same("Song", "alpha", "Song", "beta"));
        assert!(!same("Love Story", "a", "Lovely Day", "a"));
        assert!(!same("", "a", "", "a"));
    }

    /// Real titles and artists (difficulty tables' charts and their songs),
    /// as compared: whether they are one song's ([`same_song`]: a library
    /// folder or a table's entry is a chart's song) and one work's (equal
    /// [`Song`]s: a download's folder of charts is one package).
    #[test]
    fn real_titles() {
        #[rustfmt::skip]
        let pairs: &[(&str, &str, &str, &str, bool, bool)] = &[
            // one song's charts
            ("Mirage Garden [ANOTHER]", "Gom / obj: x", "Mirage Garden", "Gom", true, true),
            ("Born [29Another]", "Ninja Action Team / 29", "Born [ANOTHER]", "Ninja Action Team", true, true),
            ("BRILLIANT STAR ～煌星の雫～ [WHITE ANOTHER]", "shimizushi BGA :vol-t & a.a / obj:DJ.END", "BRILLIANT STAR ～煌星の雫～ [Shooting Star]", "shimizushi BGA : vol-t & a.a / obj : monaka", true, true),
            ("Lily [KOCHITHER]", "tolz feat.音街ウナ obj:東風谷早苗", "Lily [S-]", "tolz feat. 音街ウナ obj:S", true, true),
            ("QSHELL －拒色のシェルル－ [ZOI]", "Se-U-Ra obj:SUZUKAZEΛ", "QSHELL -拒色のシェルル- [Geraeusch]", "Se-U-Ra / И", true, true),
            ("Name of oath [SP BEGINNER]", "void feat. コツキミヤ , Gt. えば BGA : Photonskyto", "Name of oath [yumether]", "void feat. コツキミヤ , Gt. えば BGA:Photonskyto / OBJ:夢瑠", true, true),
            ("X (AFOTHER)", "sasakure.UK / black train", "X (虎のANOTHER)", "sasakure.UK / black train", true, true),
            ("F", "bubbles", "F [F]", "bubbles obj:Fender", true, true),
            ("S [ANOTHER]", "D.J.Nero", "S [HARDEST]", "D.J.Nero /差分 赤い人", true, true),
            ("Caelumize [SPA]", "void", "Caelumize [SP Cloudy]", "void obj.Cloudy Boy", true, true),
            // titles alike: one song, not one work
            ("AltMirrorBell Another", "Grand thaw", "AltMirrorBell HYPER", "Grand thaw", true, false),
            ("Imperishable Night 2006 for D", "篠螺悠那 feat. 鯛の小骨", "Imperishable Night 2006 -SP ANOTHER-", "篠螺悠那 feat. 鯛の小骨", true, false),
            ("Angelic layer -ANOTHER-", "SHIKI (obj.mp)", "Angelic delayer", "SHIKI / 水素", true, false),
            ("the lost dedicated [nostalgia]", "哀愁マニア / obj.siokaze", "the lost delaycated (distant memory)", "哀愁マニア obj: qtp13", true, false),
            ("Caelumize [SPA]", "void", "Caelum", "void", true, false),
            // two songs of an artist, alike
            ("MARENOL[HARD]", "LeaF Movie:Optie&Fiz/obj:赤い人", "MARENOIA [INSANE]", "LeaF Movie:Optie&Fiz", true, false),
            ("プナイプナイせんそう -Luna-", "立秋 feat.ちょこ / selene.", "プナイプナイげんそう -LAST BOSS-", "立秋 feat.ちょこ / obj:七海", true, false),
            ("E.V.E [ANOTHER]", "SOMON", "E.V.E.R [MARYTHER]", "SOMON / obj: Mary_Sue", true, false),
            ("Love Story", "a", "Lovely Day", "a", false, false),
            // a remix is a song of its own
            ("JUMMER (HARD+)", "momo★yama　　(BGA:アカツキユウ) obj:LAPIS", "JUMMER (LASER PSY REMIX) [3X]", "momo★yama obj:trainer", false, false),
            ("Toki (Sound piercer's Timesucker Remix) [Personality]", "puru twk. sound piercer (feat.Hee Bun.D) /obj:BAECON vs みっと", "Toki [ClockWise]", "puru / kkkfff2 obj.sialua", false, false),
            // one song's charts whose artists differ as compared: a name
            // after a `/` without spaces, a word, a music credit
            ("Trinity [Maniac]", "TAK/A-zert", "Trinity -GOD-", "TAK / selene.", false, false),
            ("硬核機動 [Maniac]", "しらいし / obj.siokaze", "硬核機動 [EX]", "しらいし/さ", false, false),
            ("BB BLOW (7key another)", "onoken", "BB BLOW -EX-", "onoken/y", false, false),
            ("Meaning of Existence [OVERHEAT]", "TKR/obj_jakko BGA:seizan", "Meaning of Existence [magical]", "TKR/obj.もにもに BGA:seizan", false, false),
            ("Blooming Jump! [ugdther]", "卯月 obj:ugd", "Blooming Jump! [Maneuver]", "卯月 改", false, false),
            ("Whispers from a Mirror", "Quint", "Whispers from a Mirror", "Quint SubAtrist", false, false),
            ("Dreadnought [LastMoment]", "Mastermind(xi+nora2r) iimo / obj:秘密", "Dreadnought [Lagoon]", "Mastermind(xi+nora2r) / obj : LindBlum iimo", false, false),
            ("tinnitus [Intense]", "SP-# arr:魔女 obj:potechang", "tinnitus -7 Minether-", "SP-# obj:MineC24", false, false),
            ("アズール <ANOTHER>", "メスクラ (エヴァたん＆メコたん)", "アズール <鬼畜>", "メスクラ (エヴァたん＆メコたん)/yokosuka", false, false),
            // a chart's name not in brackets
            ("Air ANOTHER", "SHIKI / rio", "Air [Myth]", "SHIKI obj:1008", false, false),
            // no title
            ("", "a", "", "a", false, false),
        ];
        let mut wrong = Vec::new();
        for &(t, a, u, b, song, work) in pairs {
            let (x, y) = (Song::of(t, a), Song::of(u, b));
            let got = (same_song(&x, &y), !x.title.is_empty() && x == y);
            if got != (song, work) || same_song(&y, &x) != song {
                wrong.push(format!("{t} | {a} ~ {u} | {b}: {got:?} ({x:?} {y:?})"));
            }
        }
        assert!(wrong.is_empty(), "{}", wrong.join("\n"));
    }

    /// The BeMusicSeeker fork's test vectors (its
    /// `InstallEstimationMetadataNormalizerTests`, MIT).
    #[test]
    fn fork_vectors() {
        let same = |t: &str, a: &str, u: &str, b: &str| same_song(&Song::of(t, a), &Song::of(u, b));
        let mut wrong = Vec::new();
        #[rustfmt::skip]
        let titles: &[(&str, &str)] = &[
        ("Ark (remix/5keys)", "Ark (remix"),
        ("Ark (remix/10lines)", "Ark (remix"),
        ("Ark (mixed)", "Ark (mix"),
        ("夏草の線路 -J-Trance remix-", "夏草の線路 -J-Trance remix"),
        ("夏草の線路 -J-Trance remix- [Another]", "夏草の線路 -J-Trance remix"),
        ("Air -GOD-", "Air"),
        ("Air -Revolutionary.Himix- [7keys ANOTHER]", "Air -Revolutionary.Himix"),
        ("Toki (Sound piercer's Timesucker Remix) [SP ANOTHER]", "Toki (Sound piercer's Timesucker Remix"),
        ("VIEW [Portable Disco Mix] [H14]", "VIEW [Portable Disco Mix"),
        ("Lamellibrachia [laner+]", "Lamellibrachia"),
        ("Lamellibrachia [athlete]", "Lamellibrachia"),
        ("Lamellibrachia [robo]", "Lamellibrachia"),
        ("new earth [ZOI]", "new earth"),
        ("[主題] [差分]", "[主題]"),
        ("【Freak《Show】Down》 [Another]", "【Freak《Show】Down》"),
        ("【Freak《Show】Down》", "【Freak《Show】Down》"),
        ("Nothingness[7A(LN)]", "Nothingness"),
        ("Title (A) 続き [差分]", "Title (A) 続き"),
        ("Title [差分] 追加表記", "Title"),
        ("~メッセージ~ [Another]", "~メッセージ~"),
        ("Title (未閉鎖", "Title"),
        ("Title ]", "Title"),
        ("Title 【差分】 《追加》", "Title"),
        ("Title <差分> 〜追加〜", "Title"),
        (" Ｔｉｔｌｅ　（Ａｎｏｔｈｅｒ） ", "Title"),
        ("Title-Another", "Title-Another"),
        ("(Wrapped Title)", "(Wrapped Title)"),
        ("Title [mix A remix B]", "Title [mix A remix"),
        ];
        for (raw, shown) in titles {
            if title_display(raw) != *shown {
                wrong.push(format!("title {raw:?}: {:?}", title_display(raw)));
            }
        }
        // other credits than the music's left out; a credit for the music
        // where the comparing starts again
        #[rustfmt::skip]
        let alike: &[(&str, &str)] = &[
        ("", ""),
        ("obj: diffuser", ""),
        ("  ＳＫＹ．　ＯＢＪ ：；／ Diff  ", "SKY."),
        ("SKY.obj:M1CH1", "SKY."),
        ("Artist ; .: NoTeS: Diff", "Artist"),
        ("Artist;notes:Diff", "Artist;"),
        ("Artist ;/; notes:Diff", "Artist"),
        ("Music:わかどり | Movie:もがみ", "Music:わかどり"),
        ("Artist|obj:Diff", "Artist|"),
        ("GU-Note / obj: Mary_Sue", "GU-Note"),
        ("Frontier Lyric / obj.Diff", "Frontier Lyric"),
        ("Frontier Lyric/ obj.Diff", "Frontier Lyric"),
        ("Raver -feat: Guest-", "Raver"),
        ("Artist feat: Guest-remix: A", "Artist"),
        ("Music:Musi_chan / BMS:G&P", "Music:Musi_chan"),
        ("Artist obj:Diff & BGA:Visual", "Artist"),
        ("Lalot. feat. 翠京 (BGA:SENALI × Frill OBJ: LuvTek ×", "Lalot."),
        ("源屋feat.ユスラ", "源屋"),
        ("Artist featuring Guest", "Artist"),
        ("Artist ft. Guest", "Artist"),
        ("Artist Vo.: Guest", "Artist"),
        ("Artist vocals Guest", "Artist"),
        ("Artist obj Guest", "Artist"),
        ("Artist objected by Diff", "Artist"),
        ("Artist note by Diff", "Artist"),
        ("Artist pmsobj=Diff", "Artist"),
        ("MIPSYLE odj.ユ / BGA: ジンジャー塚原", "MIPSYLE"),
        ("Nota BGI:雪椿", "Nota"),
        ("Nota/obj:みあ BGI:雪椿", "Nota"),
        ("あとぅす // 差分: Eri BGI:りーふぱいザウルス", "あとぅす"),
        ("Artist 譜面: Diff", "Artist"),
        ("Artist 差分 Diff", "Artist"),
        ("AKITO / #050C1B", "AKITO"),
        ("Artist//Diff", "Artist"),
        ("Artist/ Diff", "Artist"),
        ("Abel (Mov.Siesphere Illust.suisou)", "Abel"),
        ("kanone[BGI:TAKA.S", "kanone"),
        ("Artist ( [\nfeat. Guest", "Artist"),
        ("Artist <Movie: Visual>", "Artist"),
        ("Artist 【《illust.:. Visual》】", "Artist"),
        ("yoayua (yoa + yuayua) Movie:自動車 原付 & うまるつふり", "yoayua (yoa + yuayua)"),
        ("AKITO chart:exclusion", "AKITO"),
        ("Artist BMS:Chart_Generator,obj:Diff", "Artist"),
        ("Artist Graphic : Visual", "Artist"),
        ("牧野 Sliced by И / obj: Diff", "牧野"),
        ("Artist (Guitar: Player, Vocal: Singer)", "Artist"),
        ("Artist PMS by Diff", "Artist"),
        ("Artist Pattern By Diff", "Artist"),
        ("Artist Sequence: Diff", "Artist"),
        ("Artist Lyrics: Guest", "Artist"),
        ("Artist image: Visual", "Artist"),
        ("Artist Artwork: Visual", "Artist"),
        ("Artist Visual: Visual", "Artist"),
        ("Artist Illust&obj:JURUMA", "Artist"),
        ("Artist BGA&obj:Diff", "Artist"),
        ("Artist obj + BGA:Diff obj:Tail", "Artist"),
        ("Artist (bga/obj.: Diff)", "Artist"),
        ("Artist feat. Guest / BGA & obj:Diff", "Artist"),
        ("Artist Bms＆Bga:Diff", "Artist"),
        ("Artist obj.better music seter", "Artist"),
        ("MiKaDo feat. はらもりよしな Guitar:裸(ﾗｵｳ)王 BGA:MiKaDo Illust&obj:JURUMA", "MiKaDo"),
        ("Aliesrite* (Ym1024 feat. GuestA)", "Aliesrite* (Ym1024 feat. GuestB)"),
        ("Aliesrite* (Ym1024 feat. Guest)", "Aliesrite* (Ym1024)"),
        ("A (B [C feat. X", "A (B [C)"),
        ("A (B] feat. X", "A (B]"),
        ("Zris Remixed by Daily BG by Rosa1100", "Zris Remixed by Daily"),
        ("plastic / Remix: AYhaz / BGI: Loni", "plastic / Remix: AYhaz"),
        ("(C)Nintendo / Arranged by Y.W", "(C)Nintendo / Arranged by Y.W"),
        ("AKITO // mixed by TOSH", "AKITO // mixed by TOSH"),
        ("株式会社タカラ / Covered by はろるど", "株式会社タカラ / Covered by はろるど"),
        ("ねこみりん feat.みゆ Remixed by Ras", "ねこみりん Remixed by Ras"),
        ("DJ obelisk feat.axia /remixed.Pino obj:Diff", "DJ obelisk remixed.Pino"),
        ("Artist / Remix: A / Arr. B / obj: Diff", "Artist Remix: A Arr. B"),
        ("Artist feat GuestA Remixed by X", "Artist feat GuestB Remixed by X"),
        ("Artist feat G (Remixed by X)", "Artist (Remixed by X)"),
        ("Artist -remix: A obj:Diff", "Artist -remix: A"),
        ("Symphonian feat.kei_iwata rmx bbangsami, illust CAPOKI", "Symphonian rmx bbangsami,"),
        ("SC.Kanata feat.ORI(composed:OKAYAN)", "SC.Kanata composed:OKAYAN"),
        ("Midi:sawa Bms＆Bga:くだみつ", "Midi:sawa"),
        ("Music/BMS/BGA - A obj:Diff", "Music/BMS/BGA - A"),
        ("Artist feat G music & movie: A", "Artist music & movie: A"),
        ("BMS:ChartAuthor MIDI:Composer", "MIDI:Composer"),
        ("BMS/GYAON 作曲/井上⊿ chart:Diff", "作曲/井上⊿"),
        ("Artist feat G Remix / Pino", "Artist Remix / Pino"),
        ];
        for (a, b) in alike {
            if !same("  Ｓｏｎｇ [ANOTHER]  ", a, "Song", b) {
                wrong.push(format!(
                    "alike {a:?} {b:?}: {:?} {:?}",
                    artist_key(a),
                    artist_key(b)
                ));
            }
        }
        // a name is no credit
        #[rustfmt::skip]
        let apart: &[(&str, &str)] = &[
        ("Key Note Team", "Key"),
        ("Object", ""),
        ("FT 718", ""),
        ("Vo Team", ""),
        ("BGAキャラ水着ガチャ", ""),
        ("ZizhiRFeather", "ZizhiR"),
        ("Artist_feat.Guest", "Artist"),
        ("PakPiano-Note , DJ Demadosa", "PakPiano"),
        ("Frontier Lyric / obj.Diff", "Frontier"),
        ("q/stol", "q"),
        ("Dango/Dx", "Dango"),
        ("1/8", "1"),
        ("π/3", "π"),
        ("/指紋", ""),
        ("AcuticNotes(An/レク)", "AcuticNotes(An)"),
        ("A (B / C)", "A (B)"),
        ("sasakure.UK", "sasakure."),
        ("Re:gats&", "Re"),
        ("A & B + C vs D", "A"),
        ("Artist | Other", "Artist"),
        ];
        for (a, b) in apart {
            if same("Song", a, "Song", b) {
                wrong.push(format!("apart {a:?} {b:?}: {:?}", artist_key(a)));
            }
        }
        #[rustfmt::skip]
        let songs: &[(&str, &str, &str, &str, bool)] = &[
        ("Ice Festival", "Nota BGI:雪椿", "Ice Festival[札幌雪祭り]", "Nota/obj:みあ BGI:雪椿", true),
        ("I believe", "Gun string", "I believe...", "Gun string", true),
        ("Midnight BEAST", "AKITO", "Midnight BEAST -deep azure mix-", "AKITO / #050C1B", false),
        ("Song", "Zris Remixed by Daily天利 BG by Rosa", "Song", "Zris Remixed by Daily BG by Rosa", false),
        ("Song", "Origin / Remixed by A", "Song", "Origin / Remixed by B", false),
        ("Song", "Artist feat GuestA Remixed by X", "Song", "Artist feat GuestB Remixed by Y", false),
        ("Song", "Artist obj:Same", "Song", "Other obj:Same", false),
        ("Song", "q/stol", "Song", "q/other", false),
        ];
        for &(t, a, u, b, want) in songs {
            if same(t, a, u, b) != want {
                wrong.push(format!("song {t:?} {a:?} {u:?} {b:?}"));
            }
        }
        assert!(wrong.is_empty(), "{}", wrong.join("\n"));
    }
}
