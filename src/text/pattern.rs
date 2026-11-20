//! The few regular expressions the venue pages' labels and the folder
//! names are read with, matched as the `regex` crate matches them
//! (leftmost-first, quantifiers greedy) without its size in a web page:
//! literals, `.`, `[...]` and `[^...]` (no ranges), `\d \w \s \W \b`,
//! groups with `|`, `? * + {m,n} {m,}`, `^ $`, and `(?i)` at the start.
//! `\s`, `\d` and `\w` are Unicode's, as far as Latin and Japanese text
//! go (marks and numbers of other scripts aside).
//!
//! It matches by trying and going back, so a text made for it (`<<<<…`
//! for `<[^>]+>`) would cost the square of its length: a search takes a
//! number of steps in proportion to the text and the pattern at most
//! ([`STEPS`]), and one that needs more finds nothing (more).

use std::ops::Range;

/// A compiled pattern. An unsupported one panics when compiled.
pub(crate) struct Pattern {
    nodes: Vec<Node>,
    /// `(?i)`: letters compared in lower case.
    fold: bool,
    /// How many nodes it has, those in groups and repeats too.
    size: usize,
}

/// The steps a search may take for each node of the pattern and each byte
/// of the text: a text of the pages and names read needs one or two (a
/// title of sixty brackets, each opening a run to its end, four).
const STEPS: usize = 16;

/// One search of a text (a match, or all of them one after another): the
/// steps it may still take.
struct Search<'a> {
    p: &'a Pattern,
    s: &'a str,
    left: std::cell::Cell<usize>,
}

/// How many nodes `nodes` are, with those in them.
fn size(nodes: &[Node]) -> usize {
    nodes
        .iter()
        .map(|n| match n {
            Node::Group(alts) => 1 + alts.iter().map(|a| size(a)).sum::<usize>(),
            Node::Repeat(n, ..) => 1 + size(std::slice::from_ref(n)),
            _ => 1,
        })
        .sum()
}

enum Node {
    Char(char),
    /// Any of the items (`true`: none of them).
    Class(Vec<Item>, bool),
    /// `.`: anything but a line feed.
    Any,
    Start,
    End,
    Boundary,
    Group(Vec<Vec<Node>>),
    Repeat(Box<Node>, u32, u32),
}

enum Item {
    Char(char),
    Digit,
    Word,
    Space,
}

/// What `\w` matches: a letter, a digit, a joining mark or punctuation
/// (`_`).
pub(crate) fn word(c: char) -> bool {
    c.is_alphabetic()
        || digit(c)
        || matches!(c, '_' | '\u{203f}' | '\u{2040}' | '\u{2054}' | '\u{fe33}' | '\u{fe34}'
            | '\u{fe4d}'..='\u{fe4f}' | '\u{ff3f}' | '\u{300}'..='\u{36f}'
            | '\u{302a}'..='\u{302f}' | '\u{3099}' | '\u{309a}')
}

/// What `\d` matches: a number that is a digit (not ①, ², ½, ㈠...).
fn digit(c: char) -> bool {
    c.is_numeric()
        && !c.is_alphabetic()
        && !matches!(c, '\u{b2}' | '\u{b3}' | '\u{b9}' | '\u{bc}'..='\u{be}'
            | '\u{2070}'..='\u{209f}' | '\u{2150}'..='\u{215f}' | '\u{2189}'
            | '\u{2460}'..='\u{24ff}' | '\u{2776}'..='\u{2793}' | '\u{2cfd}'
            | '\u{3192}'..='\u{3195}' | '\u{3220}'..='\u{3229}' | '\u{3248}'..='\u{324f}'
            | '\u{3251}'..='\u{325f}' | '\u{3280}'..='\u{3289}' | '\u{32b1}'..='\u{32bf}')
}

/// `c` as `(?i)` compares it.
fn lower(c: char) -> char {
    if c == 'ſ' {
        return 's';
    }
    let mut l = c.to_lowercase();
    match (l.next(), l.next()) {
        (Some(l), None) => l,
        _ => c,
    }
}

impl Item {
    fn has(&self, c: char, fold: bool) -> bool {
        match *self {
            Item::Char(x) if fold => lower(x) == lower(c),
            Item::Char(x) => x == c,
            Item::Digit => digit(c),
            Item::Word => word(c),
            Item::Space => c.is_whitespace(),
        }
    }
}

/// A pattern's text being read.
struct Parser<'a> {
    p: std::iter::Peekable<std::str::Chars<'a>>,
}

impl Parser<'_> {
    fn alternatives(&mut self) -> Vec<Vec<Node>> {
        let mut alts = vec![Vec::new()];
        while let Some(&c) = self.p.peek() {
            match c {
                ')' => break,
                '|' => {
                    self.p.next();
                    alts.push(Vec::new());
                }
                _ => {
                    let atom = self.atom();
                    let node = self.quantified(atom);
                    alts.last_mut().expect("an alternative").push(node);
                }
            }
        }
        alts
    }

    fn atom(&mut self) -> Node {
        match self.p.next().expect("an atom") {
            '(' => {
                let alts = self.alternatives();
                assert_eq!(self.p.next(), Some(')'), "a group closed");
                Node::Group(alts)
            }
            '[' => self.class(),
            '.' => Node::Any,
            '^' => Node::Start,
            '$' => Node::End,
            '\\' => match self.p.next().expect("an escape") {
                'b' => Node::Boundary,
                'W' => Node::Class(vec![Item::Word], true),
                c => match escaped(c) {
                    Item::Char(c) => Node::Char(c),
                    item => Node::Class(vec![item], false),
                },
            },
            c => {
                assert!(!"*+?{".contains(c), "nothing to repeat");
                Node::Char(c)
            }
        }
    }

    fn class(&mut self) -> Node {
        let not = self.p.next_if_eq(&'^').is_some();
        let mut items = Vec::new();
        loop {
            match self.p.next().expect("a class closed") {
                ']' if !items.is_empty() => return Node::Class(items, not),
                '\\' => items.push(escaped(self.p.next().expect("an escape"))),
                '-' if !items.is_empty() && self.p.peek() != Some(&']') => {
                    panic!("no ranges")
                }
                c => items.push(Item::Char(c)),
            }
        }
    }

    fn quantified(&mut self, atom: Node) -> Node {
        let (min, max) = match self.p.peek() {
            Some('?') => (0, 1),
            Some('*') => (0, u32::MAX),
            Some('+') => (1, u32::MAX),
            Some('{') => {
                self.p.next();
                let min = self.number().expect("a count");
                let max = match self.p.next() {
                    Some('}') => min,
                    Some(',') => {
                        let max = self.number().unwrap_or(u32::MAX);
                        assert_eq!(self.p.next(), Some('}'), "a count closed");
                        max
                    }
                    _ => panic!("a count"),
                };
                return Node::Repeat(Box::new(atom), min, max);
            }
            _ => return atom,
        };
        self.p.next();
        assert!(!matches!(self.p.peek(), Some('?' | '+')), "greedy only");
        Node::Repeat(Box::new(atom), min, max)
    }

    fn number(&mut self) -> Option<u32> {
        let mut n = String::new();
        while let Some(d) = self.p.next_if(char::is_ascii_digit) {
            n.push(d);
        }
        n.parse().ok()
    }
}

fn escaped(c: char) -> Item {
    match c {
        'd' => Item::Digit,
        'w' => Item::Word,
        's' => Item::Space,
        c if c.is_ascii_punctuation() => Item::Char(c),
        c => panic!("\\{c} is not supported"),
    }
}

/// What comes after a part of the pattern matched (where it ended):
/// whether the whole pattern matched.
type Then<'k> = &'k mut dyn FnMut(usize) -> bool;

impl Pattern {
    pub(crate) fn new(pattern: &str) -> Pattern {
        let (fold, rest) = match pattern.strip_prefix("(?i)") {
            Some(rest) => (true, rest),
            None => (false, pattern),
        };
        let mut p = Parser {
            p: rest.chars().peekable(),
        };
        let mut alts = p.alternatives();
        assert!(p.p.next().is_none(), "unbalanced )");
        let nodes = match alts.len() {
            1 => alts.pop().expect("one"),
            _ => vec![Node::Group(alts)],
        };
        Pattern {
            size: size(&nodes),
            nodes,
            fold,
        }
    }

    fn search<'a>(&'a self, s: &'a str) -> Search<'a> {
        Search {
            p: self,
            s,
            left: std::cell::Cell::new(STEPS * self.size * (s.len() + 1)),
        }
    }

    /// Where the leftmost match is.
    pub(crate) fn find(&self, s: &str) -> Option<Range<usize>> {
        self.search(s).find_at(0)
    }

    pub(crate) fn is_match(&self, s: &str) -> bool {
        self.find(s).is_some()
    }

    /// The matches, one after another (none overlapping).
    pub(crate) fn find_iter<'a>(&'a self, s: &'a str) -> impl Iterator<Item = Range<usize>> + 'a {
        let search = self.search(s);
        let mut at = Some(0);
        std::iter::from_fn(move || {
            let m = search.find_at(at?)?;
            // (after an empty match, from the next character)
            at = match m.is_empty() {
                true => s[m.end..].chars().next().map(|c| m.end + c.len_utf8()),
                false => Some(m.end),
            };
            Some(m)
        })
    }

    /// `s` with each match as `with` makes it of the matched text.
    pub(crate) fn replace_all(&self, s: &str, mut with: impl FnMut(&str) -> String) -> String {
        let mut out = String::new();
        let mut last = 0;
        for m in self.find_iter(s) {
            out.push_str(&s[last..m.start]);
            out.push_str(&with(&s[m.clone()]));
            last = m.end;
        }
        out + &s[last..]
    }

    /// `s` without its leftmost match.
    pub(crate) fn remove_first(&self, s: &str) -> String {
        match self.find(s) {
            Some(m) => [&s[..m.start], &s[m.end..]].concat(),
            None => s.to_string(),
        }
    }

    /// `s` in the parts between the matches.
    pub(crate) fn split<'a>(&'a self, s: &'a str) -> impl Iterator<Item = &'a str> + 'a {
        let mut last = 0;
        let mut matches = self.find_iter(s);
        std::iter::from_fn(move || {
            let part = match matches.next() {
                Some(m) => {
                    let part = &s[last..m.start];
                    last = m.end;
                    part
                }
                None if last <= s.len() => {
                    let part = &s[last..];
                    last = s.len() + 1;
                    part
                }
                None => return None,
            };
            Some(part)
        })
    }
}

impl Search<'_> {
    /// One step taken, if there is one left.
    fn step(&self) -> bool {
        let left = self.left.get();
        self.left.set(left.saturating_sub(1));
        left > 0
    }

    /// The leftmost match from `from` on (none once the steps are spent).
    fn find_at(&self, from: usize) -> Option<Range<usize>> {
        let s = self.s;
        let starts = s[from..].char_indices().map(|(i, _)| from + i);
        for start in starts.chain([s.len()]) {
            let mut end = None;
            self.seq(&self.p.nodes, start, &mut |e| {
                end = Some(e);
                true
            });
            if self.left.get() == 0 {
                return None;
            }
            if let Some(end) = end {
                return Some(start..end);
            }
        }
        None
    }

    fn seq(&self, nodes: &[Node], i: usize, then: Then) -> bool {
        match nodes.split_first() {
            None => then(i),
            Some((n, rest)) => self.one(n, i, &mut |j| self.seq(rest, j, then)),
        }
    }

    /// Where a node of one character matching at `i` ends (None: another
    /// node, or no match).
    fn char_at(&self, n: &Node, i: usize) -> Option<usize> {
        let fold = self.p.fold;
        let c = self.s[i..].chars().next()?;
        let ok = match n {
            Node::Char(x) if fold => lower(c) == lower(*x),
            Node::Char(x) => c == *x,
            Node::Class(items, not) => items.iter().any(|it| it.has(c, fold)) != *not,
            Node::Any => c != '\n',
            _ => false,
        };
        ok.then(|| i + c.len_utf8())
    }

    fn one(&self, n: &Node, i: usize, then: Then) -> bool {
        if !self.step() {
            return false;
        }
        let s = self.s;
        match n {
            Node::Char(_) | Node::Class(..) | Node::Any => self.char_at(n, i).is_some_and(then),
            Node::Start => i == 0 && then(i),
            Node::End => i == s.len() && then(i),
            Node::Boundary => {
                let before = s[..i].chars().next_back().is_some_and(word);
                before != s[i..].chars().next().is_some_and(word) && then(i)
            }
            Node::Group(alts) => alts.iter().any(|a| self.seq(a, i, then)),
            Node::Repeat(n, min, max) => self.repeat(n, *min, *max, i, then),
        }
    }

    /// `n` `min` to `max` times, as many as can be.
    fn repeat(&self, n: &Node, min: u32, max: u32, i: usize, then: Then) -> bool {
        if let Node::Group(_) = n {
            let more = max > 0
                && self.one(n, i, &mut |j| {
                    // (an empty match is not repeated)
                    (j != i || min > 0) && self.repeat(n, min.saturating_sub(1), max - 1, j, then)
                });
            return more || min == 0 && then(i);
        }
        // (one character at a time: no recursion for each)
        let mut ends = vec![i];
        while ends.len() <= max as usize && self.step() {
            match self.char_at(n, ends[ends.len() - 1]) {
                Some(j) => ends.push(j),
                None => break,
            }
        }
        ends.iter()
            .enumerate()
            .rev()
            .any(|(count, &e)| count >= min as usize && self.left.get() > 0 && then(e))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn as_regex_matches() {
        let p = Pattern::new(r"(?i)new ?(ver|link)|^\W*new\W*$|fixed");
        assert!(p.is_match("NEW Ver.2") && p.is_match("【New】") && !p.is_match("news"));
        assert_eq!(p.find("a FIXED b"), Some(2..7));
        // greedy, giving back what the rest needs
        let p = Pattern::new(r"\s*([\(].*[\)]|[-].*[-])$");
        assert_eq!(p.find("a (b) c (d)"), Some(1..11));
        assert_eq!(p.find("x -y-"), Some(1..5));
        assert_eq!(
            Pattern::new(r"team\b.{0,4}pack").find("team b pack"),
            Some(0..11)
        );
        assert!(!Pattern::new(r"alt\b").is_match("alt版"));
        assert!(Pattern::new(r"^(\d+(S|D)P|bms)[.:・\s]+").is_match("１２SP: x"));
        let spaces = Pattern::new(r"\s{2,}");
        assert_eq!(
            spaces.replace_all("a  b \u{3000}c d", |_| " ".into()),
            "a b c d"
        );
        let br = Pattern::new(r"(?i)<br\s*/?>");
        let parts: Vec<&str> = br.split("a<BR>b<br />").collect();
        assert_eq!(parts, ["a", "b", ""]);
        assert_eq!(Pattern::new("[ab]").remove_first("cab"), "cb");
    }

    /// A text made for a pattern (each place a run to the text's end, then
    /// back) costs the text's length, not its square: the search finds
    /// nothing, as there is nothing; what a text of a page's kind holds is
    /// found as before.
    #[test]
    fn a_search_takes_its_texts_length() {
        let n = 300_000;
        let since = std::time::Instant::now();
        let tag = Pattern::new(r"<[^>]+>");
        assert_eq!(tag.find(&"<".repeat(n)), None);
        assert_eq!(tag.replace_all(&"<".repeat(n), |_| String::new()).len(), n);
        let brackets = Pattern::new(r"\s*([\(].*[\)]|[\[].*[\]])$");
        assert_eq!(brackets.find(&"(".repeat(n)), None);
        assert!(since.elapsed() < std::time::Duration::from_secs(20));
        // a long text of a page's kind: every match, in its length's time
        let page = "<p class=x>text</p> ".repeat(n / 20);
        assert_eq!(tag.find_iter(&page).count(), 2 * (n / 20));
        let title = format!("{} (a) [b] -c- (d)", "song ".repeat(40));
        assert_eq!(brackets.find(&title).map(|m| m.end), Some(title.len()));
    }
}
