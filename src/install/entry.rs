//! A difficulty table's entry (a chart, its song, their links) as the
//! downloads it needs, in order ([`entry_steps`]): nothing when the
//! library has the chart; the separate chart (差分) first when the library
//! has its song; and a batch of entries ([`Batch`]: a table's missing
//! charts) gets each file once. [`EntryJob`] takes them.

use super::job::{Dead, Install, Installed};
use super::{got, Done, Failure};
use crate::chart::md5_key;
use crate::fs::Store;
use crate::hosts::{Context, DownloadKey, Want};
use crate::library::Library;
use crate::net::job::{Fed, Job, Step};
use crate::net::NetError;
use crate::package::Role;
use crate::progress::Progress;
use crate::sources::{Query, Sources};
use std::collections::{HashMap, VecDeque};
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
        Entry::from_table_in(e, &[links])
    }

    /// The table's entry `e`, its links those `lists` know (its own first,
    /// then each list's in turn: [`Links::song_links_in`]).
    ///
    /// [`Links::song_links_in`]: crate::links::Links::song_links_in
    pub fn from_table_in(e: &crate::table::TableEntry, lists: &[&crate::links::Links]) -> Entry {
        use crate::links::Links;
        Entry {
            md5: e.md5.clone(),
            sha256: e.sha256.clone(),
            want: Want {
                title: e.title.clone(),
                artist: e.artist.clone(),
            },
            org_md5: e.org_md5.clone(),
            song: Links::song_links_in(lists, e),
            chart: Links::chart_links_in(lists, e),
        }
    }

    /// Whether the library has the chart.
    pub(crate) fn owned(&self, lib: &Library) -> bool {
        lib.owns(&self.md5) || lib.owns(&self.sha256)
    }

    /// The chart's hashes.
    pub(super) fn hashes(&self) -> Vec<String> {
        vec![self.md5.clone(), self.sha256.clone()]
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

/// The library's folder of the chart of one of `hashes`, if it has it.
pub(super) fn holding(lib: &Library, hashes: &[String]) -> Option<PathBuf> {
    let h = hashes.iter().find(|h| lib.owns(h))?;
    Some(lib.folders_of(h).first().cloned().unwrap_or_default())
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

/// What a batch of downloads (a table's missing charts, a session's run)
/// shares: what each file it has had came to (by its `DownloadKey`), the
/// charts it wants (a separate chart's archive of many keeps those waiting
/// for their songs), the files whose download failed (a copy shares them),
/// and the files being downloaded, by which job ([`Batch::claim`]: a
/// session's jobs at once). What a file brought is kept for good; what
/// failed (a download, a take, a page) for the jobs of the round it was
/// tried in and later ones ([`Asker::round`]).
#[derive(Clone, Debug, Default)]
pub struct Batch {
    seen: HashMap<DownloadKey, Seen>,
    wanted: Vec<String>,
    pub(super) dead: Dead,
    claims: HashMap<DownloadKey, Holder>,
}

/// What a file the batch has had came to.
#[derive(Clone, Debug)]
enum Seen {
    /// It brought `done`, keeping waiting (a separate charts' archive) only
    /// the charts `kept` names: the first so many of those wanted then,
    /// and those its takes were for (None: all it held).
    Brought {
        done: Vec<Done>,
        kept: Option<(usize, Vec<String>)>,
    },
    /// A page for a browser (`page`), or nothing (its take failed), for
    /// the jobs of `round` and later.
    Missed { page: bool, round: u64 },
}

/// The job downloading a file, its progress (how far it has got: whether
/// it shows life), and the job it took the file over from, which may still
/// bring it.
#[derive(Clone, Debug)]
struct Holder {
    id: u64,
    progress: Option<Arc<Progress>>,
    under: Option<Box<Holder>>,
}

impl Holder {
    fn bytes(&self) -> u64 {
        self.progress.as_ref().map_or(0, |p| p.bytes().0)
    }

    /// Job `id` gone from those it was taken over from.
    fn forget(&mut self, id: u64) {
        let Some(u) = &mut self.under else {
            return;
        };
        if u.id == id {
            let below = u.under.take();
            self.under = below;
        } else {
            u.forget(id);
        }
    }
}

/// A job as the batch knows it: its number (0: a job alone), its round
/// (a session's jobs made after [`Session::again`](super::Session::again)
/// are of a later one: what failed before is tried again for them), and
/// the progress its downloads count bytes in.
#[derive(Clone, Debug, Default)]
pub(crate) struct Asker {
    pub(crate) id: u64,
    pub(crate) round: u64,
    pub(crate) progress: Option<Arc<Progress>>,
}

/// Whether a job downloads a file ([`Batch::claim`]).
#[derive(Clone, Debug)]
pub(crate) enum Claim {
    /// The batch has had it: what it came to.
    Had(Had),
    /// This job's to download.
    Mine,
    /// This job's, taken over from one that showed no life (its failure
    /// is not the file's while that one may still bring it).
    Taken,
    /// Another job (`holder`, its progress at `bytes`) downloads it: ask
    /// again after a wait.
    Busy { holder: u64, bytes: u64 },
}

/// What a file the batch has had came to, for a job that asks for it.
#[derive(Clone, Debug)]
pub(crate) enum Had {
    /// It brought these (another job's take of it).
    Brought(Vec<Done>),
    /// A page.
    Page,
    /// Nothing (its take failed).
    Nothing,
}

impl Batch {
    /// A batch for the charts `wanted` (MD5s).
    pub fn new(wanted: impl IntoIterator<Item = String>) -> Batch {
        let mut b = Batch::default();
        b.want(wanted);
        b
    }

    /// The charts `wanted` (MD5s) too.
    pub(super) fn want(&mut self, wanted: impl IntoIterator<Item = String>) {
        for m in wanted.into_iter().filter_map(|m| md5_key(&m)) {
            if !self.wanted.contains(&m) {
                self.wanted.push(m);
            }
        }
    }

    /// The key of `link` for `q` (a song's: a folder's link brings its
    /// archive).
    pub(crate) fn key(link: &str, q: &Query) -> DownloadKey {
        DownloadKey::of(link, (!q.diff).then_some(&q.want))
    }

    /// Whether `job` downloads the file of `key` for `q`: had already
    /// (what it brought, if it kept `q`'s chart had there been one; a page
    /// or nothing, tried in `job`'s round or later); else its own (claimed
    /// until [`Batch::tried`] or [`Batch::release`]), unless another job
    /// has claimed it. That one is taken to be gone when it is `stale`
    /// (the job and its bytes as `job` found them through its wait): the
    /// claim is `job`'s then, until it ends.
    pub(crate) fn claim(
        &mut self,
        key: &DownloadKey,
        job: &Asker,
        q: &Query,
        stale: Option<(u64, u64)>,
    ) -> Claim {
        match self.seen.get(key) {
            Some(Seen::Brought { done, kept }) if self.kept(kept.as_ref(), q) => {
                return Claim::Had(Had::Brought(done.clone()));
            }
            Some(Seen::Missed { page, round }) if *round >= job.round => {
                return Claim::Had(if *page { Had::Page } else { Had::Nothing });
            }
            _ => {}
        }
        let mine = Holder {
            id: job.id,
            progress: job.progress.clone(),
            under: None,
        };
        match self.claims.get_mut(key) {
            None => {
                self.claims.insert(key.clone(), mine);
                Claim::Mine
            }
            Some(h) if h.id == job.id => Claim::Mine,
            Some(h) if stale == Some((h.id, h.bytes())) => {
                let under = std::mem::replace(h, mine);
                h.under = Some(Box::new(under));
                Claim::Taken
            }
            Some(h) => Claim::Busy {
                holder: h.id,
                bytes: h.bytes(),
            },
        }
    }

    /// Whether a file that kept the charts `kept` names kept `q`'s.
    fn kept(&self, kept: Option<&(usize, Vec<String>)>, q: &Query) -> bool {
        let Some((wanted, takers)) = kept else {
            return true;
        };
        md5_key(&q.md5).is_some_and(|m| self.wanted[..*wanted].contains(&m) || takers.contains(&m))
    }

    /// Job `id`'s claims released (it ended, or is gone): one it took over
    /// is the job's it took it from again.
    pub(super) fn release(&mut self, id: u64) {
        self.claims.retain(|_, h| {
            if h.id != id {
                h.forget(id);
                return true;
            }
            match h.under.take() {
                Some(under) => {
                    *h = *under;
                    true
                }
                None => false,
            }
        });
    }

    /// The file of `key` `job` downloaded for `q` came to `done`, the
    /// claim released if it is `job`'s: had, unless it did not come (a
    /// failed download: `dead` answers for it; cancelled) or was not the
    /// chart looked for (found by name, it may be another's). True: it
    /// brought nothing and the job it was taken over from may still bring
    /// it; that one's claim again, nothing kept.
    pub(crate) fn tried(
        &mut self,
        job: &Asker,
        key: DownloadKey,
        q: &Query,
        done: &[Done],
    ) -> bool {
        let came = got(done);
        match self.claims.get_mut(&key) {
            Some(h) if h.id == job.id => match h.under.take() {
                Some(under) if !came => {
                    *h = *under;
                    return true;
                }
                _ => {
                    self.claims.remove(&key);
                }
            },
            Some(h) => h.forget(job.id),
            None => {}
        }
        let again = done
            .iter()
            .any(|d| matches!(d, Done::Failed(Failure::Net(_) | Failure::NotTheChart)));
        if came {
            // (a separate charts' archive kept waiting only those wanted)
            let md5 = md5_key(&q.md5).filter(|_| q.diff);
            let kept = md5.map(|m| (self.wanted.len(), vec![m]));
            match self.seen.get_mut(&key) {
                Some(Seen::Brought { done: had, kept: k }) => {
                    had.extend_from_slice(done);
                    *k = match (k.take(), kept) {
                        (Some((_, mut takers)), Some((wanted, m))) => {
                            takers.extend(m);
                            Some((wanted, takers))
                        }
                        _ => None,
                    };
                }
                _ => {
                    let done = done.to_vec();
                    self.seen.insert(key, Seen::Brought { done, kept });
                }
            }
        } else if !again {
            let page = done.iter().any(|d| matches!(d, Done::Page(_)));
            let newer = |s: &Seen| match s {
                Seen::Brought { .. } => true,
                Seen::Missed { round, .. } => *round > job.round,
            };
            if !self.seen.get(&key).is_some_and(newer) {
                let round = job.round;
                self.seen.insert(key, Seen::Missed { page, round });
            }
        }
        false
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
/// a page): [`Failure::NoChartLink`]. In a [`Session`](super::Session),
/// the chart brought in by another of its jobs ends it ([`Done::Owned`]).
pub struct EntryJob {
    pub(super) install: Install,
    e: Entry,
    /// The steps to come, and whether the sources may be asked for each
    /// (not for an add-on).
    steps: VecDeque<(EntryStep, bool)>,
    sources: Sources,
    /// The first of its links dropped for being written as no download
    /// reads one (`https://user@host/…`): said when nothing else is.
    dropped: Option<String>,
    /// Where the last step's outcomes start in `done`.
    last: usize,
    done: Vec<Done>,
}

impl EntryJob {
    /// `e` from its links and then `sources` into `lib` (natively through
    /// `downloads`). What of `e`'s links is no web link (an empty cell, a
    /// table's note, `file:` or `javascript:`; as a download reads one:
    /// http or https, plainly written) is dropped, not tried: a step left
    /// without links is one for the sources, or none. An http(s) link
    /// dropped for how it is written is said ([`NOT_WEB`]) when the chart
    /// is still missing and nothing but "no link" was found.
    ///
    /// [`NOT_WEB`]: crate::hosts::NOT_WEB
    pub fn new(
        store: Arc<dyn Store + Send + Sync>,
        downloads: PathBuf,
        lib: Library,
        mut e: Entry,
        sources: Sources,
        cx: &Context,
    ) -> EntryJob {
        let web = |u: &String| {
            let u = u.trim().to_ascii_lowercase();
            u.starts_with("http://") || u.starts_with("https://")
        };
        let dropped = (e.song.iter().chain(&e.chart))
            .find(|u| web(u) && !is_link(u))
            .cloned();
        e.song.retain(|u| is_link(u));
        e.chart.retain(|u| is_link(u));
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
            dropped,
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
    /// lacks the chart (what is no web link dropped, as [`EntryJob::new`]
    /// drops it).
    pub fn addons(mut self, links: impl IntoIterator<Item = String>) -> EntryJob {
        self.add(links);
        self
    }

    /// [`EntryJob::addons`] in place.
    pub(super) fn add(&mut self, links: impl IntoIterator<Item = String>) {
        let step = |l| EntryStep {
            links: vec![l],
            role: Role::Diff,
            when: When::Missing,
        };
        self.steps.extend(
            links
                .into_iter()
                .filter(|l| is_link(l))
                .map(|l| (step(l), false)),
        );
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
            let (q, chart) = (self.e.query(step.role), self.e.hashes());
            self.install.again(q, step.links, sources, chart);
            let s = self.install.next();
            return self.went(s);
        }
        let mut done = std::mem::take(&mut self.done);
        let lib = &self.install.lib;
        let page = done.iter().any(|d| matches!(d, Done::Page(_)));
        let missed = !done.is_empty() && !cancelled(&done) && !got(&done);
        // (the library as it is now: another job of a session may have
        // brought its song)
        if missed && !page && has_song(&self.e, lib) {
            done = vec![Done::Failed(Failure::NoChartLink)];
        }
        let no_link = |d: &Done| matches!(d, Done::Failed(Failure::NoLink | Failure::NoChartLink));
        if let Some(u) = &self.dropped {
            if !self.e.owned(&self.install.lib) && done.iter().all(no_link) {
                let why = format!("{u}: {}", crate::hosts::NOT_WEB);
                done = vec![Done::Failed(Failure::Other(why))];
            }
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

/// Whether `e` with the add-ons `addons` has nothing to try, whatever
/// the library has: no link a download could try and no MD5 for the
/// sources. Else [`EntryJob::new`] makes steps, unless the library has
/// the chart: the library decides nothing more of it (`entry_steps`
/// keeps the chart's step when the song is there, the song's when not,
/// each with links or the sources).
pub(super) fn untried(e: &Entry, addons: &[String]) -> bool {
    let mut links = e.song.iter().chain(&e.chart).chain(addons);
    md5_key(&e.md5).is_none() && !links.any(|u| is_link(u))
}

/// Whether a download could try `u` (`hosts::Link`'s rule).
fn is_link(u: &str) -> bool {
    crate::hosts::Link::parse(u, 0).is_some()
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
    use crate::install::md5_hex;
    use crate::net::job::{Served, Unserved};
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

    /// Job `id` of the first round, as the batch knows it.
    fn job(id: u64) -> Asker {
        Asker {
            id,
            ..Default::default()
        }
    }

    /// Whether the batch has had `link`'s file for `asker`'s `q`.
    fn has(b: &mut Batch, link: &str, q: &Query, asker: &Asker) -> bool {
        let c = b.claim(&Batch::key(link, q), asker, q, None);
        b.release(asker.id);
        matches!(c, Claim::Had(_))
    }

    #[test]
    fn a_batch_has_each_file_once() {
        let e = entry("Song", &[]);
        let (song, diff) = (e.query(Role::Song), e.query(Role::Diff));
        let mut b = Batch::new([e.md5.to_uppercase(), "short".into(), md5_hex(b"other")]);
        assert_eq!(b.also(&song), [md5_hex(b"other")]);
        let view = "https://drive.google.com/file/d/ID1/view";
        let uc = "https://drive.google.com/uc?id=ID1&export=download";
        let j = job(9);
        assert!(!has(&mut b, view, &song, &j));
        let added = [Done::Added("bms/S".into(), 2)];
        b.tried(&job(0), Batch::key(view, &song), &song, &added);
        assert!(has(&mut b, uc, &diff, &j));
        // not the chart (found by name), or cancelled: may come again
        let other = "https://e/b.zip";
        let not = [Done::Failed(Failure::NotTheChart)];
        b.tried(&job(0), Batch::key(other, &diff), &diff, &not);
        let cancelled = [Done::Failed(crate::net::NetError::Cancelled.into())];
        b.tried(&job(0), Batch::key(other, &diff), &diff, &cancelled);
        assert!(!has(&mut b, other, &diff, &j));
        // nor one whose download failed (the batch's dead files say why)
        let failed = [Done::Failed(crate::net::NetError::Status(404).into())];
        b.tried(&job(0), Batch::key(other, &diff), &diff, &failed);
        assert!(!has(&mut b, other, &diff, &j));
        // a folder's link: the song looked for in it counts
        let folder = "https://drive.google.com/drive/folders/F1";
        b.tried(&job(0), Batch::key(folder, &song), &song, &added);
        let mut other_song = e.clone();
        other_song.want.title = "Other".into();
        assert!(has(&mut b, folder, &song, &j));
        assert!(!has(&mut b, folder, &other_song.query(Role::Song), &j));
    }

    /// What failed (a page, a take of nothing) answers the jobs of its
    /// round and later ones; what came, every job.
    #[test]
    fn what_failed_answers_its_round() {
        let q = entry("Song", &[]).query(Role::Song);
        let (page, cut, came) = ("https://e/page", "https://e/cut.zip", "https://e/song.zip");
        let mut b = Batch::default();
        let key = |l: &str| Batch::key(l, &q);
        b.tried(&job(1), key(page), &q, &[Done::Page(page.into())]);
        let nothing = [Done::Failed(Failure::Other("cut off".into()))];
        b.tried(&job(1), key(cut), &q, &nothing);
        b.tried(&job(1), key(came), &q, &[Done::Added("bms/S".into(), 2)]);
        let now = |b: &mut Batch, l: &str, round: u64| {
            let asker = Asker {
                id: 2,
                round,
                ..Default::default()
            };
            let c = b.claim(&key(l), &asker, &q, None);
            b.release(2);
            c
        };
        assert!(matches!(now(&mut b, page, 0), Claim::Had(Had::Page)));
        assert!(matches!(now(&mut b, cut, 0), Claim::Had(Had::Nothing)));
        assert!(matches!(now(&mut b, page, 1), Claim::Mine));
        assert!(matches!(now(&mut b, cut, 1), Claim::Mine));
        for round in [0, 1] {
            let c = now(&mut b, came, round);
            assert!(
                matches!(&c, Claim::Had(Had::Brought(d)) if d.len() == 1),
                "{c:?}"
            );
        }
        // a job of an earlier round: a failure since answers it too
        let later = Asker {
            id: 3,
            round: 1,
            ..Default::default()
        };
        b.tried(&later, key(page), &q, &[Done::Page(page.into())]);
        assert!(matches!(now(&mut b, page, 0), Claim::Had(Had::Page)));
        assert!(matches!(now(&mut b, page, 1), Claim::Had(Had::Page)));
        assert!(matches!(now(&mut b, page, 2), Claim::Mine));
    }

    /// A separate charts' archive kept waiting only the charts wanted when
    /// it came: it answers for those, and for the chart its take was for;
    /// a chart wanted since gets it again. A song's archive kept all.
    #[test]
    fn a_charts_archive_answers_for_the_charts_it_kept() {
        let chart = |t: &str| Query {
            md5: md5_hex(t.as_bytes()),
            diff: true,
            ..Default::default()
        };
        let (a, c, n) = (chart("a"), chart("c"), chart("n"));
        let none = Query {
            md5: String::new(),
            ..a.clone()
        };
        let link = "https://e/charts.zip";
        let k = Batch::key(link, &a);
        let mut b = Batch::new([n.md5.clone()]);
        b.tried(&job(1), k.clone(), &a, &[Done::Waiting("dl/a.bms".into())]);
        b.want([c.md5.clone()]);
        let j = job(9);
        assert!(has(&mut b, link, &a, &j) && has(&mut b, link, &n, &j));
        assert!(!has(&mut b, link, &c, &j) && !has(&mut b, link, &none, &j));
        b.tried(&job(2), k, &c, &[Done::Waiting("dl/c.bms".into())]);
        for q in [&a, &c, &n] {
            assert!(has(&mut b, link, q, &j), "{q:?}");
        }
        let song = Query {
            diff: false,
            ..a.clone()
        };
        let pack = "https://e/pack.zip";
        b.tried(
            &job(3),
            Batch::key(pack, &song),
            &song,
            &[Done::Added("bms/S".into(), 2)],
        );
        assert!(has(&mut b, pack, &c, &j) && has(&mut b, pack, &none, &j));
    }

    /// A file one job downloads is another's to wait for, until it came
    /// (had), did not (the next asker's), or the job is gone.
    #[test]
    fn claims() {
        let q = entry("Song", &[]).query(Role::Song);
        let key = Batch::key("https://e/song.zip", &q);
        let mut b = Batch::default();
        let claim = |b: &mut Batch, id| b.claim(&key, &job(id), &q, None);
        assert!(matches!(claim(&mut b, 1), Claim::Mine));
        assert!(matches!(claim(&mut b, 1), Claim::Mine));
        assert!(matches!(
            claim(&mut b, 2),
            Claim::Busy {
                holder: 1,
                bytes: 0
            }
        ));
        let failed = [Done::Failed(crate::net::NetError::Status(404).into())];
        assert!(!b.tried(&job(1), key.clone(), &q, &failed));
        assert!(matches!(claim(&mut b, 2), Claim::Mine));
        b.release(2);
        assert!(matches!(claim(&mut b, 3), Claim::Mine));
        b.release(3);
        assert!(b.claims.is_empty());
        b.tried(&job(1), key.clone(), &q, &[Done::Added("bms/S".into(), 2)]);
        assert!(matches!(claim(&mut b, 3), Claim::Had(_)));
    }

    /// A claim is taken over only from the job, at the bytes, a waiter
    /// found unmoved through its wait: then it is the taker's (a third
    /// waits for it), until the taker ends. Bringing nothing, or gone, the
    /// taker leaves it to the job it took it from, if that one has not
    /// ended; had once the file came, whoever brought it.
    #[test]
    fn a_claim_taken_over() {
        let q = entry("Song", &[]).query(Role::Song);
        let key = Batch::key("https://e/song.zip", &q);
        let failed = [Done::Failed(crate::net::NetError::Status(404).into())];
        let p = crate::progress::Progress::new();
        let first = Asker {
            id: 1,
            progress: Some(p.clone()),
            ..Default::default()
        };
        let mut b = Batch::default();
        let claim = |b: &mut Batch, id, stale| b.claim(&key, &job(id), &q, stale);
        assert!(matches!(b.claim(&key, &first, &q, None), Claim::Mine));
        p.add(5);
        // (its bytes moved since)
        let c = claim(&mut b, 2, Some((1, 0)));
        assert!(
            matches!(
                c,
                Claim::Busy {
                    holder: 1,
                    bytes: 5
                }
            ),
            "{c:?}"
        );
        assert!(matches!(claim(&mut b, 2, Some((1, 5))), Claim::Taken));
        assert!(matches!(
            claim(&mut b, 3, Some((1, 5))),
            Claim::Busy { holder: 2, .. }
        ));
        // the taker brought nothing: the first's again
        assert!(b.tried(&job(2), key.clone(), &q, &failed));
        assert!(matches!(
            claim(&mut b, 3, None),
            Claim::Busy { holder: 1, .. }
        ));
        // taken again, the taker gone
        assert!(matches!(claim(&mut b, 2, Some((1, 5))), Claim::Taken));
        b.release(2);
        assert!(matches!(
            claim(&mut b, 3, None),
            Claim::Busy { holder: 1, .. }
        ));
        // the first ends while the taker downloads: the taker's alone
        assert!(matches!(claim(&mut b, 2, Some((1, 5))), Claim::Taken));
        assert!(!b.tried(&first, key.clone(), &q, &failed));
        assert!(!b.tried(&job(2), key.clone(), &q, &failed));
        assert!(b.claims.is_empty());
        // the taker brings it
        assert!(matches!(claim(&mut b, 1, None), Claim::Mine));
        assert!(matches!(claim(&mut b, 2, Some((1, 0))), Claim::Taken));
        let added = [Done::Added("bms/S".into(), 2)];
        assert!(!b.tried(&job(2), key.clone(), &q, &added));
        assert!(b.claims.is_empty());
        assert!(matches!(claim(&mut b, 3, None), Claim::Had(_)));
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
        let out = crate::net::job::run_job(&mut job, d);
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

    /// What is no web link is not tried, as a link nor as a step: an entry
    /// of none (and no MD5 for the sources) is nothing to try.
    #[test]
    fn only_web_links_tried() {
        let (m, _) = library();
        let m = Arc::new(m);
        let mut e = entry("Another [X]", &["", "javascript:alert(1)"]);
        e.md5.clear();
        e.song = [
            "",
            " ",
            "not a link",
            "file:///x.zip",
            "https://user@e/a.zip",
        ]
        .map(String::from)
        .to_vec();
        let lib = Library::scan(&*m, Path::new("bms"));
        let cx = Context::default();
        let job = EntryJob::new(
            m.clone(),
            "dl".into(),
            lib,
            e.clone(),
            Sources::default(),
            &cx,
        )
        .addons(["--help".to_string(), "mailto:a@b".into()]);
        assert!(job.is_empty());
        // the http(s) link dropped for how it is written is said, nothing
        // asked for
        let d = Canned::default();
        let (out, _) = entry_job(&m, &e, &[], &cx, &d);
        let why = format!("https://user@e/a.zip: {}", crate::hosts::NOT_WEB);
        assert!(downloads(&d).is_empty());
        assert!(
            matches!(&out.done[..], [Done::Failed(Failure::Other(w))] if *w == why),
            "{:?}",
            out.done
        );
        // the links among them, and only those
        e.song.push("HTTPS://e/song.zip".into());
        e.chart.push("https://e/x.bms".into());
        let d = Canned::default();
        let (out, _) = entry_job(&m, &e, &["--help", "https://e/addon.zip"], &cx, &d);
        assert_eq!(
            downloads(&d),
            [
                "download HTTPS://e/song.zip",
                "download https://e/x.bms",
                "download https://e/addon.zip"
            ]
        );
        assert!(!out.done.is_empty() && !got(&out.done), "{:?}", out.done);
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
            let out = crate::net::job::run_job(&mut job.batch(batch), &native);
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
            crate::net::job::run_job(&mut { job }, &crate::native::Native::new(&cx))
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
