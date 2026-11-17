//! A difficulty table's entry (a chart, its song, their links) as the
//! downloads it needs, in order ([`entry_steps`]): nothing when the
//! library has the chart; the separate chart (差分) first when the library
//! has its song; and a batch of entries ([`Batch`]: a table's missing
//! charts) gets each file once.

use super::{Done, Failure, Role};
use crate::chart::md5_key;
use crate::hosts::{DownloadKey, Want};
use crate::library::Library;
use crate::sources::Query;
use std::collections::HashSet;

/// A table's entry: the chart wanted, its song, and their links.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Entry {
    pub md5: String,
    pub sha256: String,
    pub want: Want,
    /// The song's own charts a table names (`org_md5`).
    pub org_md5: Vec<String>,
    /// The song's links (its own, then the others known), in order.
    pub song: Vec<String>,
    /// The separate chart's (差分) links.
    pub chart: Vec<String>,
}

impl Entry {
    /// Whether the library has the chart.
    pub fn owned(&self, lib: &Library) -> bool {
        lib.owns(&self.md5) || lib.owns(&self.sha256)
    }

    /// The query for one of its steps.
    pub fn query(&self, role: Role) -> Query {
        Query {
            md5: self.md5.clone(),
            want: self.want.clone(),
            diff: role == Role::Diff,
            org_md5: self.org_md5.clone(),
        }
    }
}

/// When a step is taken, after the one before it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum When {
    Always,
    /// While the library lacks the chart (the song before it may have
    /// brought it).
    Missing,
    /// When the chart before it waits for its song (the library's folder
    /// of the song did not take it: the song's archive may).
    Waiting,
}

/// One download of an entry: its links (with none, those the sources
/// find), what they are for, and when it is taken.
#[derive(Clone, Debug, PartialEq)]
pub struct EntryStep {
    pub links: Vec<String>,
    pub role: Role,
    pub when: When,
}

impl EntryStep {
    /// Whether the step is taken, the library as it is now and `before`
    /// what the step before it came to.
    pub fn due(&self, e: &Entry, lib: &Library, before: &[Done]) -> bool {
        match self.when {
            When::Always => true,
            When::Missing => !e.owned(lib),
            When::Waiting => !e.owned(lib) && before.iter().any(|d| matches!(d, Done::Waiting(_))),
        }
    }
}

/// Whether the library has the entry's song: one of its own charts the
/// table names, or a chart of its title and artist.
pub fn has_song(e: &Entry, lib: &Library) -> bool {
    e.org_md5.iter().any(|m| lib.owns(m)) || !lib.song_folders_of(&e.want).is_empty()
}

/// The downloads `e` needs from the library `lib`, in order: none when it
/// has the chart; with its song there (and the chart a link of its own,
/// or none for the song: the sources may know one), the chart first, then
/// the song only if the chart is left waiting (none of the chart's own
/// links: only if it is still missing); else the song, then the chart
/// unless the song brought it.
pub fn entry_steps(e: &Entry, lib: &Library) -> Vec<EntryStep> {
    let step = |links: &Vec<String>, role, when| EntryStep {
        links: links.clone(),
        role,
        when,
    };
    if e.owned(lib) {
        return Vec::new();
    }
    let mut out = Vec::new();
    if has_song(e, lib) && (!e.chart.is_empty() || md5_key(&e.md5).is_some()) {
        out.push(step(&e.chart, Role::Diff, When::Always));
        if !e.song.is_empty() {
            let when = match e.chart.is_empty() {
                true => When::Missing,
                false => When::Waiting,
            };
            out.push(step(&e.song, Role::Song, when));
        }
        return out;
    }
    out.push(step(&e.song, Role::Song, When::Always));
    if !e.chart.is_empty() {
        out.push(step(&e.chart, Role::Diff, When::Missing));
    }
    out
}

/// What a batch of downloads (a table's missing charts) shares: the files
/// it has had (by their [`DownloadKey`]), and the charts it wants (a
/// separate chart's archive of many keeps those waiting for their songs).
#[derive(Clone, Debug, Default)]
pub struct Batch {
    seen: HashSet<DownloadKey>,
    wanted: Vec<String>,
}

impl Batch {
    /// A batch for the charts `wanted` (MD5s).
    pub fn new(wanted: impl IntoIterator<Item = String>) -> Batch {
        Batch {
            seen: HashSet::new(),
            wanted: wanted.into_iter().filter_map(|m| md5_key(&m)).collect(),
        }
    }

    /// The key of `link` for `q` (a song's: a folder's link brings its
    /// archive).
    pub fn key(link: &str, q: &Query) -> DownloadKey {
        DownloadKey::of(link, (!q.diff).then_some(&q.want))
    }

    /// Whether the batch has had the file of `key`.
    pub fn has(&self, key: &DownloadKey) -> bool {
        self.seen.contains(key)
    }

    /// The file of `key` came to `done`: had, unless it was not the chart
    /// looked for (found by name, it may be another's) or cancelled.
    pub fn tried(&mut self, key: DownloadKey, done: &[Done]) {
        let again = done
            .iter()
            .any(|d| matches!(d, Done::Failed(f) if f.cancelled() || *f == Failure::NotTheChart));
        if !again {
            self.seen.insert(key);
        }
    }

    /// The charts wanted but `q`'s.
    pub fn also(&self, q: &Query) -> Vec<String> {
        let md5 = md5_key(&q.md5);
        self.wanted
            .iter()
            .filter(|m| Some(*m) != md5.as_ref())
            .cloned()
            .collect()
    }
}

/// An entry's steps ([`entry_steps`]) one after another, each through its
/// links and the sources ([`super::run`]), into `lib` (what comes
/// in added to it); `batch`: what the run has had. `each`: what a step
/// came to, as it comes.
#[cfg(not(target_arch = "wasm32"))]
pub fn entry_in(
    downloads: &std::path::Path,
    lib: &mut Library,
    e: &Entry,
    sources: &crate::sources::Sources,
    batch: &mut Batch,
    cx: &crate::hosts::Context,
    mut each: impl FnMut(&EntryStep, &[Done]),
) -> Vec<Done> {
    let mut out = Vec::new();
    let mut last: Vec<Done> = Vec::new();
    for step in entry_steps(e, lib) {
        if cx.cancelled() || !step.due(e, lib, &last) {
            continue;
        }
        let q = e.query(step.role);
        last = super::run(downloads, lib, &q, &step.links, sources, batch, cx);
        each(&step, &last);
        out.extend(last.iter().cloned());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fs::Mem;
    use crate::install::md5_hex;
    use std::path::Path;

    const SONG: &str = "#TITLE Song [N]\n#ARTIST alpha\n#WAV01 01.wav\n";

    fn library() -> (Mem, Library) {
        let m = Mem::default();
        m.put(Path::new("bms/Song/n.bms"), SONG.as_bytes(), 0);
        m.put(Path::new("bms/Song/01.ogg"), b"x", 0);
        let lib = Library::scan(&m, Path::new("bms"));
        (m, lib)
    }

    fn entry(title: &str, chart: &[&str]) -> Entry {
        Entry {
            md5: md5_hex(b"#TITLE Song [X]"),
            want: Want {
                title: title.into(),
                artist: "alpha".into(),
            },
            song: vec!["https://e/song.zip".into()],
            chart: chart.iter().map(|s| s.to_string()).collect(),
            ..Default::default()
        }
    }

    fn roles(steps: &[EntryStep]) -> Vec<(Role, When)> {
        steps.iter().map(|s| (s.role, s.when)).collect()
    }

    #[test]
    fn steps_by_what_the_library_has() {
        let (_, lib) = library();
        // the chart there: nothing
        let mut owned = entry("Song [N]", &[]);
        owned.md5 = md5_hex(SONG.as_bytes()).to_uppercase();
        assert!(entry_steps(&owned, &lib).is_empty());
        // the song there: the chart first, the song if it waits
        let e = entry("Song [X]", &["https://e/x.zip"]);
        assert_eq!(
            roles(&entry_steps(&e, &lib)),
            [(Role::Diff, When::Always), (Role::Song, When::Waiting)]
        );
        // no link of its own: the sources', then the song if still missing
        assert_eq!(
            roles(&entry_steps(&entry("Song [X]", &[]), &lib)),
            [(Role::Diff, When::Always), (Role::Song, When::Missing)]
        );
        // the song by the table's org_md5 (its title another)
        let mut by_org = entry("Other", &["https://e/x.zip"]);
        assert_eq!(roles(&entry_steps(&by_org, &lib))[0].0, Role::Song);
        by_org.org_md5 = vec![md5_hex(SONG.as_bytes())];
        assert_eq!(roles(&entry_steps(&by_org, &lib))[0].0, Role::Diff);
        // the song not there: the song, then the chart unless it came
        let e = entry("Another", &["https://e/x.zip"]);
        assert_eq!(
            roles(&entry_steps(&e, &lib)),
            [(Role::Song, When::Always), (Role::Diff, When::Missing)]
        );
        assert_eq!(
            roles(&entry_steps(&entry("Another", &[]), &lib)),
            [(Role::Song, When::Always)]
        );
    }

    #[test]
    fn steps_due() {
        let (m, mut lib) = library();
        let e = entry("Song [X]", &["https://e/x.zip"]);
        let steps = entry_steps(&e, &lib);
        let waiting = [Done::Waiting("downloads/x.bms".into())];
        let failed = [Done::Failed(Failure::NoLink)];
        assert!(steps[0].due(&e, &lib, &[]));
        assert!(steps[1].due(&e, &lib, &waiting));
        assert!(!steps[1].due(&e, &lib, &failed));
        // the chart came in: nothing more
        m.put(Path::new("bms/Song/x.bms"), b"#TITLE Song [X]", 0);
        lib.add(Path::new("bms/Song/x.bms"), Some(&e.md5));
        assert!(!steps[1].due(&e, &lib, &waiting));
        let missing = EntryStep {
            links: Vec::new(),
            role: Role::Diff,
            when: When::Missing,
        };
        assert!(!missing.due(&e, &lib, &[]));
    }

    #[test]
    fn a_batch_has_each_file_once() {
        let e = entry("Song", &[]);
        let (song, diff) = (e.query(Role::Song), e.query(Role::Diff));
        let mut b = Batch::new([e.md5.to_uppercase(), "short".into(), md5_hex(b"other")]);
        assert_eq!(b.also(&song), [md5_hex(b"other")]);
        let view = "https://drive.google.com/file/d/ID1/view";
        let uc = "https://drive.google.com/uc?id=ID1&export=download";
        assert!(!b.has(&Batch::key(view, &song)));
        b.tried(Batch::key(view, &song), &[Done::Added("bms/S".into(), 2)]);
        assert!(b.has(&Batch::key(uc, &diff)));
        // not the chart (found by name), or cancelled: may come again
        let other = "https://e/b.zip";
        b.tried(
            Batch::key(other, &diff),
            &[Done::Failed(Failure::NotTheChart)],
        );
        b.tried(
            Batch::key(other, &diff),
            &[Done::Failed(crate::net::NetError::Cancelled.into())],
        );
        assert!(!b.has(&Batch::key(other, &diff)));
        // a folder's link: the song looked for in it counts
        let folder = "https://drive.google.com/drive/folders/F1";
        b.tried(Batch::key(folder, &song), &[]);
        let mut other_song = e.clone();
        other_song.want.title = "Other".into();
        assert!(b.has(&Batch::key(folder, &song)));
        assert!(!b.has(&Batch::key(folder, &other_song.query(Role::Song))));
    }

    /// A server on this machine for `files` (path, body), counting the
    /// requests for each path.
    #[cfg(not(target_arch = "wasm32"))]
    fn serve(
        files: Vec<(&'static str, Vec<u8>)>,
    ) -> (String, std::sync::Arc<std::sync::Mutex<Vec<String>>>) {
        use std::io::{Read, Write};
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = l.local_addr().unwrap();
        let asked = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let log = asked.clone();
        std::thread::spawn(move || {
            for c in l.incoming() {
                let Ok(mut c) = c else { return };
                let mut buf = [0u8; 4096];
                let n = c.read(&mut buf).unwrap_or(0);
                let head = String::from_utf8_lossy(&buf[..n]).into_owned();
                let path = head.split(' ').nth(1).unwrap_or("").to_string();
                log.lock().unwrap().push(path.clone());
                let reply = match files.iter().find(|(p, _)| *p == path) {
                    Some((_, b)) => {
                        let mut r = format!(
                            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                            b.len()
                        )
                        .into_bytes();
                        r.extend(b);
                        r
                    }
                    None => {
                        b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                            .to_vec()
                    }
                };
                let _ = c.write_all(&reply);
            }
        });
        (format!("http://{addr}"), asked)
    }

    /// A table's run (audit defect 3): two entries of one song and a pack
    /// of separate charts for two songs. The song is downloaded once, the
    /// pack once; each chart ends in its song's folder.
    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn a_tables_run_gets_each_file_once() {
        use crate::fs::Disk;
        let chart = |title: &str, artist: &str| {
            format!("#TITLE {title}\n#ARTIST {artist}\n#WAV01 01.wav\n#WAV02 02.wav\n")
        };
        let (a_n, a_x, b_n, b_x) = (
            chart("Alpha [N]", "aa"),
            chart("Alpha [X]", "aa / obj: z"),
            chart("Beta [N]", "bb"),
            chart("Beta [X]", "bb / obj: z"),
        );
        let (base, asked) = serve(vec![
            (
                "/alpha.zip",
                crate::testkit::zip(&[
                    ("Alpha/n.bms", a_n.as_str()),
                    ("Alpha/01.ogg", "1"),
                    ("Alpha/02.ogg", "2"),
                ]),
            ),
            (
                "/beta.zip",
                crate::testkit::zip(&[
                    ("Beta/n.bms", b_n.as_str()),
                    ("Beta/01.ogg", "1"),
                    ("Beta/02.ogg", "2"),
                ]),
            ),
            (
                "/pack.zip",
                crate::testkit::zip(&[("p/ax.bms", &a_x), ("p/bx.bms", &b_x)]),
            ),
        ]);
        let root = crate::testkit::tmp("batch");
        let (library, downloads) = (root.join("bms"), root.join("bms/.dl"));
        let entry = |md5: &str, title: &str, artist: &str, song: &str, chart: &str| Entry {
            md5: md5.into(),
            want: Want {
                title: title.into(),
                artist: artist.into(),
            },
            song: vec![format!("{base}{song}")],
            chart: if chart.is_empty() {
                Vec::new()
            } else {
                vec![format!("{base}{chart}")]
            },
            ..Default::default()
        };
        let entries = [
            entry(
                &md5_hex(a_n.as_bytes()),
                "Alpha [N]",
                "aa",
                "/alpha.zip",
                "",
            ),
            entry(
                &md5_hex(a_x.as_bytes()),
                "Alpha [X]",
                "aa",
                "/alpha.zip",
                "/pack.zip",
            ),
            entry(
                &md5_hex(b_x.as_bytes()),
                "Beta [X]",
                "bb",
                "/beta.zip",
                "/pack.zip",
            ),
        ];
        let mut lib = Library::scan(&Disk, &library);
        let mut batch = Batch::new(entries.iter().map(|e| e.md5.clone()));
        let cx = crate::hosts::Context::default();
        let none = crate::sources::Sources::default();
        for e in &entries {
            entry_in(&downloads, &mut lib, e, &none, &mut batch, &cx, |_, _| {});
        }
        let mut asked = asked.lock().unwrap().clone();
        asked.sort();
        assert_eq!(asked, ["/alpha.zip", "/beta.zip", "/pack.zip"]);
        for e in &entries {
            assert!(lib.owns(&e.md5), "{}", e.want.title);
        }
        // two songs' folders, the charts in them, none waiting
        let folders = crate::placement::song_folders(&Disk, &library);
        let names: Vec<Vec<String>> = folders.values().cloned().collect();
        assert_eq!(
            names,
            [
                ["01.ogg", "02.ogg", "ax.bms", "n.bms"],
                ["01.ogg", "02.ogg", "bx.bms", "n.bms"]
            ],
            "{folders:?}"
        );
        assert!(std::fs::read_dir(&downloads)
            .unwrap()
            .flatten()
            .all(|e| !crate::placement::is_chart(&e.path())));
    }
}
