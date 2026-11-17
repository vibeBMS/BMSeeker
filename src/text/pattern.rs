//! The few regular expressions the venue pages' labels and the folder
//! names are read with, matched as the `regex` crate matches them
//! (leftmost-first, quantifiers greedy) without its size in a web page:
//! literals, `.`, `[...]` and `[^...]` (no ranges), `\d \w \s \W \b`,
//! groups with `|`, `? * + {m,n} {m,}`, `^ $`, and `(?i)` at the start.
//! `\s`, `\d` and `\w` are Unicode's, as far as Latin and Japanese text
//! go (marks and numbers of other scripts aside).

use std::ops::Range;

/// A compiled pattern. An unsupported one panics when compiled.
pub(crate) struct Pattern {
    nodes: Vec<Node>,
    /// `(?i)`: letters compared in lower case.
    fold: bool,
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
        Pattern { nodes, fold }
    }

    /// Where the leftmost match is.
    pub(crate) fn find(&self, s: &str) -> Option<Range<usize>> {
        self.find_at(s, 0)
    }

    fn find_at(&self, s: &str, from: usize) -> Option<Range<usize>> {
        let starts = s[from..].char_indices().map(|(i, _)| from + i);
        starts.chain([s.len()]).find_map(|start| {
            let mut end = None;
            self.seq(&self.nodes, s, start, &mut |e| {
                end = Some(e);
                true
            });
            end.map(|end| start..end)
        })
    }

    pub(crate) fn is_match(&self, s: &str) -> bool {
        self.find(s).is_some()
    }

    /// The matches, one after another (none overlapping).
    pub(crate) fn find_iter<'a>(&'a self, s: &'a str) -> impl Iterator<Item = Range<usize>> + 'a {
        let mut at = Some(0);
        std::iter::from_fn(move || {
            let m = self.find_at(s, at?)?;
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

    fn seq(&self, nodes: &[Node], s: &str, i: usize, then: Then) -> bool {
        match nodes.split_first() {
            None => then(i),
            Some((n, rest)) => self.one(n, s, i, &mut |j| self.seq(rest, s, j, then)),
        }
    }

    /// Where a node of one character matching at `i` ends (None: another
    /// node, or no match).
    fn char_at(&self, n: &Node, s: &str, i: usize) -> Option<usize> {
        let c = s[i..].chars().next()?;
        let ok = match n {
            Node::Char(x) if self.fold => lower(c) == lower(*x),
            Node::Char(x) => c == *x,
            Node::Class(items, not) => items.iter().any(|it| it.has(c, self.fold)) != *not,
            Node::Any => c != '\n',
            _ => false,
        };
        ok.then(|| i + c.len_utf8())
    }

    fn one(&self, n: &Node, s: &str, i: usize, then: Then) -> bool {
        match n {
            Node::Char(_) | Node::Class(..) | Node::Any => self.char_at(n, s, i).is_some_and(then),
            Node::Start => i == 0 && then(i),
            Node::End => i == s.len() && then(i),
            Node::Boundary => {
                let before = s[..i].chars().next_back().is_some_and(word);
                before != s[i..].chars().next().is_some_and(word) && then(i)
            }
            Node::Group(alts) => alts.iter().any(|a| self.seq(a, s, i, then)),
            Node::Repeat(n, min, max) => self.repeat(n, *min, *max, s, i, then),
        }
    }

    /// `n` `min` to `max` times, as many as can be.
    fn repeat(&self, n: &Node, min: u32, max: u32, s: &str, i: usize, then: Then) -> bool {
        if let Node::Group(_) = n {
            let more = max > 0
                && self.one(n, s, i, &mut |j| {
                    // (an empty match is not repeated)
                    (j != i || min > 0)
                        && self.repeat(n, min.saturating_sub(1), max - 1, s, j, then)
                });
            return more || min == 0 && then(i);
        }
        // (one character at a time: no recursion for each)
        let mut ends = vec![i];
        while ends.len() <= max as usize {
            match self.char_at(n, s, ends[ends.len() - 1]) {
                Some(j) => ends.push(j),
                None => break,
            }
        }
        ends.iter()
            .enumerate()
            .rev()
            .any(|(count, &e)| count >= min as usize && then(e))
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
}
