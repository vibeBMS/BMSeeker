//! Where a command's records (JSON Lines or text) and notes go: stdout and
//! stderr, or whatever a platform hands them to (a web page's callback).
//!
//! What a record or a note says is partly another party's (a title, a
//! file's name, a server's words): in text, a control character but a line
//! feed or a tab (a record's own lines and columns) is written as `�`, so
//! none reaches a terminal as an escape sequence (the C1 ones and what
//! turns the reading order around too).

use serde_json::Value;
use std::io::Write;
use std::rc::Rc;

/// A line a command writes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Line<'a> {
    /// A record (stdout).
    Record(&'a str),
    /// A note for a person (stderr, never in the records).
    Note(&'a str),
}

#[derive(Clone)]
pub struct Out {
    pub json: bool,
    sink: Rc<dyn Fn(Line)>,
}

impl Out {
    /// To stdout and stderr.
    pub fn stdio(json: bool) -> Out {
        Out::new(json, |l| match l {
            Line::Record(s) => {
                let mut o = std::io::stdout().lock();
                let _ = writeln!(o, "{s}");
                let _ = o.flush();
            }
            Line::Note(s) => eprintln!("{s}"),
        })
    }

    /// Each line to `sink`.
    pub fn new(json: bool, sink: impl Fn(Line) + 'static) -> Out {
        Out {
            json,
            sink: Rc::new(sink),
        }
    }

    /// One record: a JSON line with `--json`, else `text`.
    pub fn record(&self, v: Value, text: impl FnOnce() -> String) {
        let line = if self.json {
            json_line(&v)
        } else {
            printable(&text()).into_owned()
        };
        (self.sink)(Line::Record(&line));
    }

    /// A record that is always JSON (an event's works and checks).
    #[cfg(not(target_arch = "wasm32"))]
    pub fn json(&self, v: &Value) {
        (self.sink)(Line::Record(&json_line(v)));
    }

    /// A note for a person.
    pub fn note(&self, text: impl AsRef<str>) {
        (self.sink)(Line::Note(&printable(text.as_ref())));
    }
}

/// `v` as a JSON line, the control characters JSON leaves as they are (DEL
/// and the C1 ones, which a terminal may act on) escaped as it escapes the
/// others: the same value.
fn json_line(v: &Value) -> String {
    let line = v.to_string();
    if !line.contains(char::is_control) {
        return line;
    }
    let mut out = String::with_capacity(line.len());
    for c in line.chars() {
        match c.is_control() {
            true => out.push_str(&format!("\\u{:04x}", c as u32)),
            false => out.push(c),
        }
    }
    out
}

/// `text` for a terminal: its control characters but line feed and tab,
/// and the marks that override the reading order, as `�`.
fn printable(text: &str) -> std::borrow::Cow<'_, str> {
    let unsafe_char = |c: char| {
        c.is_control() && c != '\n' && c != '\t'
            || matches!(c, '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}')
    };
    match text.contains(unsafe_char) {
        true => text.replace(unsafe_char, "\u{fffd}").into(),
        false => text.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    /// What a server or a file said reaches a terminal as text only: no
    /// escape sequence, in a record or a note, in text or in JSON.
    #[test]
    fn no_control_characters() {
        let lines: Rc<RefCell<Vec<String>>> = Rc::default();
        let sink = lines.clone();
        let out = |json| {
            let sink = sink.clone();
            Out::new(json, move |l| match l {
                Line::Record(s) | Line::Note(s) => sink.borrow_mut().push(s.to_string()),
            })
        };
        let name = "a\u{1b}[2J\u{9b}31m\r\u{7}b\u{202e}c\u{7f}";
        let text = out(false);
        text.record(serde_json::json!({}), || format!("{name}\n  file:\t{name}"));
        text.note(format!("note: {name}"));
        let shown = "a\u{fffd}[2J\u{fffd}31m\u{fffd}\u{fffd}b\u{fffd}c\u{fffd}";
        assert_eq!(
            *lines.borrow(),
            [
                format!("{shown}\n  file:\t{shown}"),
                format!("note: {shown}")
            ]
        );
        // JSON: the same value, nothing a terminal acts on
        lines.borrow_mut().clear();
        out(true).record(serde_json::json!({"name": name}), String::new);
        let line = lines.borrow()[0].clone();
        assert!(!line.contains(char::is_control), "{line:?}");
        let read: Value = serde_json::from_str(&line).unwrap();
        assert_eq!(read["name"], name);
        // what has none is written as it is
        out(false).record(serde_json::json!({}), || "曲 (ogg)\t★12".into());
        assert_eq!(lines.borrow()[1], "曲 (ogg)\t★12");
    }
}
