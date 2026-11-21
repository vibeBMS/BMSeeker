//! A difficulty table's entry (a chart, its song, their links) as the
//! downloads it needs, in order ([`entry_steps`]): nothing when the
//! library has the chart; the separate chart (差分) first when the library
//! has its song; and a batch of entries ([`Batch`]: a table's missing
//! charts) gets each file once. [`EntryJob`] takes them.

use super::job::{Dead, Fed, Install, Installed, Job, Step};
use super::{got, Done, Failure};
use crate::chart::md5_key;
use crate::fs::Store;
use crate::hosts::{Context, DownloadKey, Want};
use crate::library::Library;
use crate::net::NetError;
use crate::package::Role;
use crate::sources::{Query, Sources};
use std::collections::{HashSet, VecDeque};
use std::path::PathBuf;
use std::sync::Arc;

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
    /// The table's entry `e`, its links those `links` knows (its own
    /// first).
    pub fn from_table(e: &crate::table::TableEntry, links: &crate::links::Links) -> Entry {
        Entry {
            md5: e.md5.clone(),
            sha256: e.sha256.clone(),
            want: Want {
                title: e.title.clone(),
                artist: e.artist.clone(),
            },
            org_md5: e.org_md5.clone(),
            song: links.song_links(e),
            chart: links.chart_links(e),
        }
    }

    /// Whether the library has the chart.
    pub(crate) fn owned(&self, lib: &Library) -> bool {
        lib.owns(&self.md5) || lib.owns(&self.sha256)
    }

    /// The query for one of its steps.
    pub(crate) fn query(&self, role: Role) -> Query {
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
pub(crate) enum When {
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
pub(crate) struct EntryStep {
    pub(crate) links: Vec<String>,
    pub(crate) role: Role,
    pub(crate) when: When,
}

impl EntryStep {
    /// Whether the step is taken, the library as it is now and `before`
    /// what the step before it came to.
    pub(crate) fn due(&self, e: &Entry, lib: &Library, before: &[Done]) -> bool {
        match self.when {
            When::Always => true,
            When::Missing => !e.owned(lib),
            When::Waiting => !e.owned(lib) && before.iter().any(|d| matches!(d, Done::Waiting(_))),
        }
    }
}

/// Whether the library has the entry's song: one of its own charts the
/// table names, or a chart of its title and artist.
pub(crate) fn has_song(e: &Entry, lib: &Library) -> bool {
    e.org_md5.iter().any(|m| lib.owns(m)) || lib.has_song(&e.want)
}

/// The downloads `e` needs from the library `lib`, in order: none when it
/// has the chart; with its song there (and the chart a link of its own,
/// or none for the song: the sources may know one), the chart first, then
/// the song only if the chart is left waiting (none of the chart's own
/// links: only if it is still missing); else the song, then the chart
/// unless the song brought it.
pub(crate) fn entry_steps(e: &Entry, lib: &Library) -> Vec<EntryStep> {
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
/// it has had (by their `DownloadKey`), the charts it wants (a separate
/// chart's archive of many keeps those waiting for their songs), and the
/// files whose download failed (a copy shares them).
#[derive(Clone, Debug, Default)]
pub struct Batch {
    seen: HashSet<DownloadKey>,
    wanted: Vec<String>,
    pub(super) dead: Dead,
}

impl Batch {
    /// A batch for the charts `wanted` (MD5s).
    pub fn new(wanted: impl IntoIterator<Item = String>) -> Batch {
        Batch {
            wanted: wanted.into_iter().filter_map(|m| md5_key(&m)).collect(),
            ..Batch::default()
        }
    }

    /// The key of `link` for `q` (a song's: a folder's link brings its
    /// archive).
    pub(crate) fn key(link: &str, q: &Query) -> DownloadKey {
        DownloadKey::of(link, (!q.diff).then_some(&q.want))
    }

    /// Whether the batch has had the file of `key`.
    pub(crate) fn has(&self, key: &DownloadKey) -> bool {
        self.seen.contains(key)
    }

    /// The file of `key` came to `done`: had, unless it did not come (a
    /// failed download: `dead` answers for it; cancelled) or was not the
    /// chart looked for (found by name, it may be another's).
    pub(crate) fn tried(&mut self, key: DownloadKey, done: &[Done]) {
        let again = done
            .iter()
            .any(|d| matches!(d, Done::Failed(Failure::Net(_) | Failure::NotTheChart)));
        if !again {
            self.seen.insert(key);
        }
    }

    /// The charts wanted but `q`'s.
    pub(crate) fn also(&self, q: &Query) -> Vec<String> {
        let md5 = md5_key(&q.md5);
        self.wanted
            .iter()
            .filter(|m| Some(*m) != md5.as_ref())
            .cloned()
            .collect()
    }
}

/// An entry's steps (`entry_steps`, then its add-ons) one after another,
/// each an [`Install`] into the library (what comes in added to it), the
/// next one only when it is due (`EntryStep::due`). The sources are
/// asked only with the chart's MD5, and only for the song or, with the
/// song in the library when the chart's step comes (the song's step may
/// have brought it), for the chart; never for an add-on. With the song in
/// the library and none of the chart's links bringing it (nor leading to
/// a page): [`Failure::NoChartLink`].
pub struct EntryJob {
    install: Install,
    e: Entry,
    /// The steps to come, and whether the sources may be asked for each
    /// (not for an add-on).
    steps: VecDeque<(EntryStep, bool)>,
    sources: Sources,
    have_song: bool,
    /// Where the last step's outcomes start in `done`.
    last: usize,
    done: Vec<Done>,
}

impl EntryJob {
    /// `e` from its links and then `sources` into `lib` (natively through
    /// `downloads`).
    pub fn new(
        store: Arc<dyn Store + Send + Sync>,
        downloads: PathBuf,
        lib: Library,
        e: Entry,
        sources: Sources,
        cx: &Context,
    ) -> EntryJob {
        let have_song = has_song(&e, &lib);
        let steps = entry_steps(&e, &lib)
            .into_iter()
            // (nothing to try)
            .filter(|s| !s.links.is_empty() || asks(&e, s.role, &lib))
            .map(|s| (s, true))
            .collect();
        let install = Install::new(store, downloads, lib, Query::default(), Vec::new(), cx);
        EntryJob {
            install,
            e,
            steps,
            sources,
            have_song,
            last: 0,
            done: Vec::new(),
        }
    }

    /// What the run has had (a table's missing charts).
    pub fn batch(mut self, batch: Batch) -> EntryJob {
        self.install = self.install.batch(batch);
        self
    }

    /// Then each of `links` on its own, as the chart's, while the library
    /// lacks the chart.
    pub fn addons(mut self, links: impl IntoIterator<Item = String>) -> EntryJob {
        let step = |l| EntryStep {
            links: vec![l],
            role: Role::Diff,
            when: When::Missing,
        };
        self.steps
            .extend(links.into_iter().map(|l| (step(l), false)));
        self
    }

    /// Whether there is nothing to try.
    pub fn is_empty(&self) -> bool {
        self.steps.is_empty()
    }

    /// The next step due, else the end.
    fn next(&mut self) -> Step<Installed> {
        while let Some((step, ask)) = self.steps.pop_front() {
            if self.install.cx.cancelled() {
                if !cancelled(&self.done[self.last..]) {
                    self.done.push(Done::Failed(NetError::Cancelled.into()));
                }
                break;
            }
            if !step.due(&self.e, &self.install.lib, &self.done[self.last..]) {
                continue;
            }
            let sources = match ask && asks(&self.e, step.role, &self.install.lib) {
                true => self.sources.clone(),
                false => Sources::default(),
            };
            self.install
                .again(self.e.query(step.role), step.links, sources);
            let s = self.install.next();
            return self.went(s);
        }
        let mut done = std::mem::take(&mut self.done);
        let page = done.iter().any(|d| matches!(d, Done::Page(_)));
        if self.have_song && !done.is_empty() && !page && !cancelled(&done) && !got(&done) {
            done = vec![Done::Failed(Failure::NoChartLink)];
        }
        Step::Done(self.install.installed(done))
    }

    fn went(&mut self, step: Step<Vec<Done>>) -> Step<Installed> {
        match step {
            Step::Do(e) => Step::Do(e),
            Step::Done(done) => {
                self.last = self.done.len();
                self.done.extend(done);
                self.next()
            }
        }
    }
}

/// Whether the sources are asked for `e`'s step of `role`, the library as
/// it is now: with the chart's MD5, for the song, or for the chart with
/// the song in the library.
fn asks(e: &Entry, role: Role, lib: &Library) -> bool {
    md5_key(&e.md5).is_some() && (role == Role::Song || has_song(e, lib))
}

fn cancelled(done: &[Done]) -> bool {
    done.iter()
        .any(|d| matches!(d, Done::Failed(f) if f.cancelled()))
}

impl Job for EntryJob {
    type Output = Installed;
    fn start(&mut self) -> Step<Installed> {
        self.next()
    }
    fn feed(&mut self, fed: Fed) -> Step<Installed> {
        let step = self.install.fed(fed);
        self.went(step)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fs::Mem;
    use crate::install::job::tests::Canned;
    use crate::install::job::{Served, Unserved};
    use crate::install::md5_hex;
    use crate::testkit::zip;
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
        // nor one whose download failed (the batch's dead files say why)
        b.tried(
            Batch::key(other, &diff),
            &[Done::Failed(crate::net::NetError::Status(404).into())],
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

    /// A source that finds nothing, keeping whether each query was a
    /// separate chart's.
    #[derive(Clone, Default)]
    struct Asked(Arc<std::sync::Mutex<Vec<bool>>>);

    impl crate::sources::Source for Asked {
        fn name(&self) -> &'static str {
            "asked"
        }
        fn find(&self, q: &Query) -> crate::net::BoxTask<Vec<String>> {
            self.0.lock().unwrap().push(q.diff);
            crate::net::Ready::boxed(Vec::new())
        }
    }

    /// `e` (and `addons`) into the library `m` holds, the driver `d`
    /// serving; what the sources were asked (a separate chart's: true).
    fn entry_job(
        m: &Arc<Mem>,
        e: &Entry,
        addons: &[&str],
        cx: &Context,
        d: &Canned,
    ) -> (Installed, Vec<bool>) {
        let asked = Asked::default();
        let mut sources = Sources::default();
        sources.push(Box::new(asked.clone()));
        let lib = Library::scan(&**m, Path::new("bms"));
        let job = EntryJob::new(m.clone(), "dl".into(), lib, e.clone(), sources, cx);
        let mut job = job.addons(addons.iter().map(|s| s.to_string()));
        let out = crate::install::job::run_job(&mut job, d);
        let asked = asked.0.lock().unwrap().clone();
        (out, asked)
    }

    fn served(bytes: Vec<u8>) -> Result<Served, Unserved> {
        Ok(Served::bytes(bytes, None, None))
    }

    fn downloads(d: &Canned) -> Vec<String> {
        d.done.borrow().clone()
    }

    const X: &str = "#TITLE Another [X]\n#ARTIST alpha\n#WAV01 01.wav\n";

    /// The song not in the library: the song (the sources asked for it),
    /// then the chart (not asked for it) unless the song brought it; an
    /// add-on only while the chart is missing.
    #[test]
    fn song_then_chart() {
        let song = zip(&[
            (
                "Another/n.bms",
                "#TITLE Another [N]\n#ARTIST alpha\n#WAV01 01.wav\n",
            ),
            ("Another/01.ogg", "x"),
        ]);
        let mut e = entry("Another [X]", &["https://e/x.bms"]);
        e.md5 = md5_hex(X.as_bytes());
        let (m, _) = library();
        let m = Arc::new(m);
        let d = Canned::default();
        d.file("https://e/song.zip", served(song.clone()));
        d.file("https://e/x.bms", served(X.into()));
        let (out, asked) = entry_job(&m, &e, &["https://e/addon.zip"], &Context::default(), &d);
        assert!(
            matches!(&out.done[..], [Done::Added(..), Done::Placed(p)]
                if p.ends_with("Another/x.bms")),
            "{:?}",
            out.done
        );
        assert!(out.lib.owns(&e.md5) && asked.is_empty());
        assert_eq!(
            downloads(&d),
            ["download https://e/song.zip", "download https://e/x.bms"]
        );
        // the song brings the chart: nothing more
        let (m, _) = library();
        let both = zip(&[
            (
                "Another/n.bms",
                "#TITLE Another [N]\n#ARTIST alpha\n#WAV01 01.wav\n",
            ),
            ("Another/x.bms", X),
            ("Another/01.ogg", "x"),
        ]);
        let d = Canned::default();
        d.file("https://e/song.zip", served(both));
        let (out, _) = entry_job(
            &Arc::new(m),
            &e,
            &["https://e/addon.zip"],
            &Context::default(),
            &d,
        );
        assert!(matches!(&out.done[..], [Done::Added(..)]), "{:?}", out.done);
        assert_eq!(downloads(&d), ["download https://e/song.zip"]);
        // none of them comes: the sources asked for the song only, never
        // for an add-on; the song not in the library is no NoChartLink
        let (m, _) = library();
        let d = Canned::default();
        let (out, asked) = entry_job(
            &Arc::new(m),
            &e,
            &["https://e/addon.zip"],
            &Context::default(),
            &d,
        );
        assert_eq!(asked, [false]);
        assert_eq!(downloads(&d).len(), 3);
        assert!(
            matches!(
                &out.done[..],
                [
                    Done::Failed(Failure::Net(_)),
                    Done::Failed(Failure::Net(_)),
                    Done::Failed(Failure::Net(_))
                ]
            ),
            "{:?}",
            out.done
        );
    }

    /// The song's step brings the song but not its separate chart, whose
    /// own link is a page (an author's site): the sources are asked for the
    /// chart then, the song being in the library (発狂 ★1 "Canon (blazing
    /// summer mix) [EXTRA]": the song from its event, the chart in a
    /// package found by its MD5).
    #[test]
    fn the_chart_asked_for_once_its_song_came() {
        let song = zip(&[
            (
                "Another/n.bms",
                "#TITLE Another [N]\n#ARTIST alpha\n#WAV01 01.wav\n",
            ),
            ("Another/01.ogg", "x"),
        ]);
        let mut e = entry("Another [X]", &["https://e/site/"]);
        e.md5 = md5_hex(X.as_bytes());
        let (m, _) = library();
        let d = Canned::default();
        d.file("https://e/song.zip", served(song));
        d.file("https://e/site/", served(b"<html>".to_vec()));
        let (out, asked) = entry_job(&Arc::new(m), &e, &[], &Context::default(), &d);
        assert_eq!(asked, [true]);
        assert!(
            matches!(&out.done[..], [Done::Added(..), Done::Page(_)]),
            "{:?}",
            out.done
        );
    }

    /// The song in the library: the chart first (the sources asked for it
    /// with the chart's MD5 only); none of its links working, nor an
    /// add-on's: NoChartLink.
    #[test]
    fn the_chart_with_its_song_in() {
        let (m, _) = library();
        let m = Arc::new(m);
        let e = entry("Song [X]", &["https://e/x.zip"]);
        let d = Canned::default();
        let (out, asked) = entry_job(&m, &e, &["https://e/addon.zip"], &Context::default(), &d);
        assert_eq!(asked, [true]);
        assert_eq!(
            downloads(&d),
            ["download https://e/x.zip", "download https://e/addon.zip"]
        );
        assert!(
            matches!(&out.done[..], [Done::Failed(Failure::NoChartLink)]),
            "{:?}",
            out.done
        );
        // no MD5: no source asked; a page stays a page
        let mut no_md5 = e.clone();
        no_md5.md5.clear();
        let d = Canned::default();
        d.file("https://e/x.zip", served(b"<html>".to_vec()));
        let (out, asked) = entry_job(&m, &no_md5, &[], &Context::default(), &d);
        assert!(asked.is_empty());
        assert!(matches!(&out.done[..], [Done::Page(_)]), "{:?}", out.done);
        // nothing to try: no step
        no_md5.chart.clear();
        no_md5.song.clear();
        let lib = Library::scan(&*m, Path::new("bms"));
        let job = EntryJob::new(
            m.clone(),
            "dl".into(),
            lib,
            no_md5,
            Sources::default(),
            &Context::default(),
        );
        assert!(job.is_empty());
    }

    /// The chart waits for its song: the song next, the chart into its
    /// folder.
    #[test]
    fn a_waiting_chart_then_its_song() {
        let (m, _) = library();
        let m = Arc::new(m);
        let zx = "#TITLE Zeta [X]\n#ARTIST omega\n#WAV01 q.wav\n";
        let mut e = entry("Zeta [X]", &["https://e/zx.bms"]);
        e.want.artist = "omega".into();
        e.md5 = md5_hex(zx.as_bytes());
        // (the song is the library's by a chart the table names)
        e.org_md5 = vec![md5_hex(SONG.as_bytes())];
        let song = zip(&[
            (
                "Zeta/n.bms",
                "#TITLE Zeta [N]\n#ARTIST omega\n#WAV01 q.wav\n",
            ),
            ("Zeta/q.ogg", "x"),
        ]);
        let d = Canned::default();
        d.file("https://e/zx.bms", served(zx.into()));
        d.file("https://e/song.zip", served(song));
        let (out, _) = entry_job(&m, &e, &[], &Context::default(), &d);
        assert!(
            matches!(&out.done[..], [Done::Waiting(_), Done::Added(..), Done::Placed(p)]
                if p.ends_with("Zeta/zx.bms")),
            "{:?}",
            out.done
        );
        assert!(out.lib.owns(&e.md5));
    }

    /// A cancel ends it: the steps after are not taken.
    #[test]
    fn cancelled_between_steps() {
        let (m, _) = library();
        let p = crate::progress::Progress::new();
        let d = Canned {
            cancel: Some(p.clone()),
            ..Default::default()
        };
        let e = entry("Another [X]", &["https://e/x.bms"]);
        let cx = Context::default().with_progress(p);
        let (out, _) = entry_job(&Arc::new(m), &e, &["https://e/addon.zip"], &cx, &d);
        assert_eq!(downloads(&d), ["download https://e/song.zip"]);
        assert!(
            matches!(&out.done[..], [Done::Failed(f)] if f.cancelled()),
            "{:?}",
            out.done
        );
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
        use crate::testkit::{serve, Reply};
        let server = serve(vec![
            (
                "/alpha.zip",
                Reply::ok(zip(&[
                    ("Alpha/n.bms", a_n.as_str()),
                    ("Alpha/01.ogg", "1"),
                    ("Alpha/02.ogg", "2"),
                ])),
            ),
            (
                "/beta.zip",
                Reply::ok(zip(&[
                    ("Beta/n.bms", b_n.as_str()),
                    ("Beta/01.ogg", "1"),
                    ("Beta/02.ogg", "2"),
                ])),
            ),
            (
                "/pack.zip",
                Reply::ok(zip(&[("p/ax.bms", &a_x), ("p/bx.bms", &b_x)])),
            ),
        ]);
        let base = &server.url;
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
        // (the files are on this machine's own server)
        let cx = Context::new(crate::config::Config {
            local_hosts: true,
            ..Default::default()
        });
        let native = crate::native::Native::new(&cx);
        for e in &entries {
            let dl = downloads.clone();
            let job = EntryJob::new(Arc::new(Disk), dl, lib, e.clone(), Sources::default(), &cx);
            let out = crate::install::job::run_job(&mut job.batch(batch), &native);
            (lib, batch) = (out.lib, out.batch);
        }
        let mut asked = server.asked();
        asked.sort();
        assert_eq!(asked, ["/alpha.zip", "/beta.zip", "/pack.zip"]);
        for e in &entries {
            assert!(lib.owns(&e.md5), "{}", e.want.title);
        }
        // two songs' folders, the charts in them, none waiting
        let folders = crate::placement::song_folders_except(&Disk, &library, None);
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

    /// Ginger then Konmai, by the chart's MD5: Ginger's package without
    /// the chart is not taken; Konmai's brings it. A chart without a known
    /// MD5 asks neither.
    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn packages_by_md5() {
        use crate::config::Config;
        use crate::fs::Disk;
        use crate::testkit::{serve, Reply};
        let ours = "#TITLE Ours\n#ARTIST a\n#WAV01 01.wav\n";
        let md5 = md5_hex(ours.as_bytes());
        let files = serve(vec![
            ("/other.zip", Reply::ok(zip(&[("o/o.bms", "#TITLE Other")]))),
            (
                "/ours.zip",
                Reply::ok(zip(&[("s/s.bms", ours), ("s/01.ogg", "1")])),
            ),
        ]);
        let f = &files.url;
        let api = serve(vec![
            (
                &*format!("/ginger/{md5}"),
                Reply::ok(format!(r#"{{"downloadURL":"{f}/other.zip"}}"#)),
            ),
            (
                &*format!("/konmai?md5={md5}"),
                Reply::ok(format!(
                    r#"{{"result":"success","data":{{"song_url":"{f}/ours.zip"}}}}"#
                )),
            ),
        ]);
        let cx = Context::new(Config {
            third_party: true,
            sources: vec!["ginger".into(), "konmai".into()],
            ginger: format!("{}/ginger/", api.url),
            konmai: format!("{}/konmai?md5=", api.url),
            local_hosts: true,
            ..Config::default()
        });
        let sources = Sources::from_config(&cx.config, Vec::new());
        let root = crate::testkit::tmp("packages");
        let run = |e: Entry| {
            let lib = Library::scan(&Disk, &root.join("bms"));
            let job = EntryJob::new(
                Arc::new(Disk),
                root.join("dl"),
                lib,
                e,
                sources.clone(),
                &cx,
            );
            crate::install::job::run_job(&mut { job }, &crate::native::Native::new(&cx))
        };
        let want = Want {
            title: "Ours".into(),
            artist: "a".into(),
        };
        // no MD5: nothing to ask
        let e = Entry {
            want: want.clone(),
            ..Default::default()
        };
        assert!(EntryJob::new(
            Arc::new(Disk),
            root.join("dl"),
            Library::new(&root),
            e,
            sources.clone(),
            &cx
        )
        .is_empty());
        let out = run(Entry {
            md5: md5.clone(),
            want,
            ..Default::default()
        });
        assert!(
            matches!(&out.done[..], [Done::Added(_, 2)]),
            "{:?}",
            out.done
        );
        assert!(out.lib.owns(&md5));
        assert_eq!(
            api.asked(),
            [format!("/ginger/{md5}"), format!("/konmai?md5={md5}")]
        );
        assert_eq!(files.asked(), ["/other.zip", "/ours.zip"]);
    }
}
