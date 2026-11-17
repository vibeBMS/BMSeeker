//! Where a command's records (JSON Lines or text) and notes go: stdout and
//! stderr, or whatever a platform hands them to (a web page's callback).

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
        let line = if self.json { v.to_string() } else { text() };
        (self.sink)(Line::Record(&line));
    }

    /// A record that is always JSON (an event's works and checks).
    #[cfg(not(target_arch = "wasm32"))]
    pub fn json(&self, v: &Value) {
        (self.sink)(Line::Record(&v.to_string()));
    }

    /// A note for a person.
    pub fn note(&self, text: impl AsRef<str>) {
        (self.sink)(Line::Note(text.as_ref()));
    }
}
