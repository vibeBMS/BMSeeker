//! From a link where an author put a BMS archive to the archive's own URL,
//! without a browser where that can be done.
//!
//! Each kind of link is a `Host`: Google Drive files and folders,
//! Dropbox, MediaFire, MEGA files and folders, a work's page on an event
//! venue (manbow, BMS SEARCH), hosts that need a browser, and any server
//! that sends the file itself. [`Hosts`] holds them in the order they are
//! asked; the first that accepts a link opens it. A host's answer is a
//! `Resolved`: the file ([`Target`]), another link to follow (a venue's
//! work page names the author's link), a page for a person, or a failure.
//!
//! [`Resolver`] walks a link through the hosts as a [`Task`]: it asks for
//! requests and is fed the answers ([`crate::net`]), so it runs natively,
//! in a web page and in tests alike. Downloading the file itself is the
//! driver's (`native::Native` natively).

mod browser;
pub(crate) mod drive;
mod dropbox;
mod key;
mod mediafire;
mod mega;
mod uploader;
mod venues;

pub(crate) use key::DownloadKey;
pub(crate) use mega::pack_artist;
pub use mega::MegaCipher;

use crate::config::Config;
use crate::net::{Answer, BoxTask, NetError, Next, Task};
use crate::progress::{Progress, Stage};
use std::sync::Arc;

/// Why a link gave no file.
#[derive(Debug, Clone, PartialEq)]
pub enum Fetch {
    /// The link is a page to use in a browser.
    Page,
    /// The download failed ("HTTP 404"...).
    Failed(NetError),
}

impl From<NetError> for Fetch {
    fn from(e: NetError) -> Fetch {
        Fetch::Failed(e)
    }
}

impl From<String> for Fetch {
    fn from(e: String) -> Fetch {
        Fetch::Failed(e.into())
    }
}

/// A song looked for by its title and artist, as a difficulty table names
/// it: a link may lead to a folder of many songs' archives (a MEGA pack of
/// a table's songs, `[artist] title.zip` each).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Want {
    pub title: String,
    pub artist: String,
}

/// The file a link leads to.
#[derive(Clone, Debug, PartialEq)]
pub struct Target {
    pub url: String,
    /// A MEGA file's 32-byte key: the bytes come encrypted ([`MegaCipher`]).
    pub mega_key: Option<Vec<u8>>,
    /// Its name where the host tells it (a folder's listing, MEGA, the
    /// mirror); else the server's or the URL's.
    pub name: Option<String>,
}

impl Target {
    pub fn url(url: impl Into<String>) -> Target {
        Target {
            url: url.into(),
            mega_key: None,
            name: None,
        }
    }

    fn named(url: impl Into<String>, name: Option<String>) -> Target {
        Target {
            name,
            ..Target::url(url)
        }
    }
}

/// What a host makes of a link.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Resolved {
    File(Target),
    /// Other links to go on with, the best first: a venue's work page names
    /// the author's link and its mirrors. The next is tried when one gives
    /// nothing ([`Resolver::try_next`] when its file does not download).
    Follow(Vec<String>),
    /// A page for a person in a browser.
    Page,
    Failed(NetError),
}

/// A link as hosts look at it.
#[derive(Clone, Debug)]
pub(crate) struct Link {
    pub(crate) url: String,
    pub(crate) lower: String,
    /// Its host name, lower case.
    pub(crate) host: String,
    /// Links followed to get here.
    pub(crate) hops: u8,
}

impl Link {
    /// An http(s) link.
    pub(crate) fn parse(url: &str, hops: u8) -> Option<Link> {
        let url = url.trim().to_string();
        let lower = url.to_ascii_lowercase();
        if !(lower.starts_with("https://") || lower.starts_with("http://")) {
            return None;
        }
        let host = lower.split('/').nth(2).unwrap_or("").to_string();
        Some(Link {
            url,
            lower,
            host,
            hops,
        })
    }

    /// Whether its host is `domain` or under it.
    pub(crate) fn on(&self, domain: &str) -> bool {
        self.host == domain || self.host.ends_with(&format!(".{domain}"))
    }
}

/// A kind of link: a strategy of [`Resolver`].
pub(crate) trait Host: Send + Sync {
    fn name(&self) -> &'static str;
    /// Whether this host handles `link`.
    fn accepts(&self, link: &Link) -> bool;
    /// The way from `link` to its file.
    fn open(&self, link: &Link, want: Option<&Want>, config: &Config) -> BoxTask<Resolved>;
}

/// The hosts, asked in order.
pub struct Hosts(Vec<Box<dyn Host>>);

impl Default for Hosts {
    fn default() -> Hosts {
        Hosts(vec![
            Box::new(drive::Drive),
            Box::new(dropbox::Dropbox),
            Box::new(venues::Manbow),
            Box::new(venues::BmsSearchVenue),
            Box::new(venues::BmsSearchPage),
            Box::new(mediafire::MediaFire),
            Box::new(mega::Mega),
            Box::new(browser::BrowserOnly),
            Box::new(browser::Direct),
        ])
    }
}

impl Hosts {
    /// The [`Hosts::default`] ones, with the settings' dead uploaders'
    /// mirrors first (third parties': with [`Config::third_party`]).
    pub(crate) fn from_config(config: &Config) -> Hosts {
        let mut hosts = Hosts::default();
        if config.third_party && !config.uploaders.is_empty() {
            hosts.first(Box::new(uploader::Uploaders(config.uploaders.clone())));
        }
        hosts
    }

    /// Add `host` before the others.
    pub(crate) fn first(&mut self, host: Box<dyn Host>) {
        self.0.insert(0, host);
    }

    /// Leave out the host named `name`.
    #[cfg(test)]
    pub(crate) fn without(mut self, name: &str) -> Hosts {
        self.0.retain(|h| h.name() != name);
        self
    }

    #[cfg(test)]
    pub(crate) fn names(&self) -> Vec<&'static str> {
        self.0.iter().map(|h| h.name()).collect()
    }

    /// The host that handles `link`.
    pub(crate) fn find(&self, link: &Link) -> Option<&dyn Host> {
        self.0.iter().find(|h| h.accepts(link)).map(|h| h.as_ref())
    }
}

/// What resolving and downloading run with: the settings, the hosts, and
/// the progress a download reports to (and is cancelled by).
#[derive(Clone, Default)]
pub struct Context {
    pub config: Arc<Config>,
    pub hosts: Arc<Hosts>,
    pub progress: Option<Arc<Progress>>,
    /// A download that fails is a page (a web page's proxy passes archives
    /// only, refusing anything else). Not one a service refused.
    pub failures_are_pages: bool,
    /// The services that refused BMSeeker in this run: not asked again
    /// ([`crate::refusal`]). Shared by the context's clones.
    pub refused: Arc<crate::refusal::Refusals>,
}

impl Context {
    pub fn new(config: Config) -> Context {
        Context {
            hosts: Arc::new(Hosts::from_config(&config)),
            config: Arc::new(config),
            progress: None,
            failures_are_pages: false,
            refused: Arc::default(),
        }
    }

    /// This context reporting to `p` (the settings and hosts shared).
    pub fn with_progress(&self, p: Arc<Progress>) -> Context {
        Context {
            progress: Some(p),
            ..self.clone()
        }
    }

    /// The progress to report to: this context's, else the thread's
    /// ([`crate::progress::scope`], kept for older callers).
    pub(crate) fn progress(&self) -> Option<Arc<Progress>> {
        self.progress.clone().or_else(crate::progress::current)
    }

    /// Whether the download was cancelled.
    pub(crate) fn cancelled(&self) -> bool {
        self.progress().is_some_and(|p| p.cancelled())
    }

    /// Set the progress' stage (if there is one).
    pub fn stage(&self, s: Stage) {
        if let Some(p) = self.progress() {
            p.set_stage(s);
        }
    }

    /// Tell the progress (if there is one) where it is looking: `place`,
    /// a source's name or a link's host.
    pub(crate) fn at(&self, place: &str) {
        if let Some(p) = self.progress() {
            let host = Link::parse(place, 0).map(|l| l.host);
            let at = host.as_deref().unwrap_or(place);
            p.set_at(at.strip_prefix("www.").unwrap_or(at));
        }
    }
}

/// How long a download may go without data before it is given up (ms).
pub const STALL_MS: u64 = 60_000;

/// Links followed at most before giving up.
const MAX_HOPS: u8 = 3;

/// A link walked through the hosts to its file.
pub struct Resolver {
    cx: Context,
    want: Option<Want>,
    link: String,
    hops: u8,
    task: Option<BoxTask<Resolved>>,
    /// The host that answered last.
    host: &'static str,
    /// The other links the last page named (mirrors), and their hop.
    alts: Vec<String>,
    alt_hops: u8,
    /// [`Resolver::try_next`] was called: the next start goes on with it.
    resume: bool,
    /// A link that gave a page (a browser may still get its file).
    paged: bool,
}

impl Resolver {
    /// For the song `want` (a folder link: its archive).
    pub fn new(link: &str, want: Option<Want>, cx: &Context) -> Resolver {
        Resolver {
            cx: cx.clone(),
            want,
            link: link.trim().to_string(),
            hops: 0,
            task: None,
            host: "",
            alts: Vec::new(),
            alt_hops: 0,
            resume: false,
            paged: false,
        }
    }

    /// After the file the last run gave did not download: go on with the
    /// next of the links the page named (a mirror), if there is one; then
    /// run the resolver again.
    pub(crate) fn try_next(&mut self) -> bool {
        if self.alts.is_empty() {
            return false;
        }
        self.link = self.alts.remove(0);
        self.hops = self.alt_hops;
        self.resume = true;
        true
    }

    /// The next alternative after one gave `err`, else the end.
    fn or_next(&mut self, err: Fetch) -> Next<Result<Target, Fetch>> {
        self.paged |= err == Fetch::Page;
        if self.alts.is_empty() {
            // a page beats an error: a browser may still get the file
            return Next::Done(Err(if self.paged { Fetch::Page } else { err }));
        }
        self.link = self.alts.remove(0);
        self.hops = self.alt_hops;
        self.open()
    }

    /// The link it goes on with (the one it was made for, at first).
    pub(crate) fn link(&self) -> &str {
        &self.link
    }

    /// The host that opened the last link ("drive", "mega"...).
    pub fn host(&self) -> &'static str {
        self.host
    }

    fn open(&mut self) -> Next<Result<Target, Fetch>> {
        let Some(link) = Link::parse(&self.link, self.hops) else {
            return self.or_next(Fetch::Page);
        };
        let Some(host) = self.cx.hosts.find(&link) else {
            return self.or_next(Fetch::Page);
        };
        self.host = host.name();
        let mut task = host.open(&link, self.want.as_ref(), &self.cx.config);
        let next = task.start();
        self.task = Some(task);
        self.step(next)
    }

    fn step(&mut self, next: Next<Resolved>) -> Next<Result<Target, Fetch>> {
        match next {
            Next::Ask(r) => Next::Ask(r),
            Next::Done(Resolved::File(t)) => Next::Done(Ok(t)),
            Next::Done(Resolved::Follow(mut links))
                if self.hops < MAX_HOPS && !links.is_empty() =>
            {
                self.hops += 1;
                self.link = links.remove(0);
                // the page's other links before those of an earlier page
                links.append(&mut self.alts);
                self.alts = links;
                self.alt_hops = self.hops;
                self.open()
            }
            Next::Done(Resolved::Follow(_) | Resolved::Page) => self.or_next(Fetch::Page),
            Next::Done(Resolved::Failed(e)) => {
                // (a refusal an API said in its answer: MEGA's)
                if let NetError::Refused(r) = &e {
                    self.cx.refused.keep(r.clone());
                }
                self.or_next(Fetch::Failed(e))
            }
        }
    }
}

impl Task for Resolver {
    type Output = Result<Target, Fetch>;
    fn start(&mut self) -> Next<Self::Output> {
        if std::mem::take(&mut self.resume) {
            return self.open();
        }
        self.hops = 0;
        self.alts.clear();
        self.paged = false;
        self.open()
    }
    fn feed(&mut self, answer: Answer) -> Next<Self::Output> {
        match self.task.as_mut() {
            Some(t) => {
                let next = t.feed(answer);
                self.step(next)
            }
            None => Next::Done(Err(Fetch::Failed("nothing asked".into()))),
        }
    }
}

/// A GET whose answer `then` makes into a [`Resolved`] (a failed request:
/// [`Resolved::Failed`]).
pub(crate) fn get_then<F>(url: String, then: F) -> BoxTask<Resolved>
where
    F: FnOnce(Vec<u8>) -> Resolved + Send + 'static,
{
    crate::net::get_then(url, |answer| match answer {
        Ok(body) => then(body),
        Err(e) => Resolved::Failed(e),
    })
}

/// For a browser showing a [`Fetch::Page`]: on a OneDrive share (a file,
/// or a folder, which comes as a zip) it presses the page's Download button
/// once, as a person would. OneDrive has no download URL to fetch without a
/// browser any more (its API wants an app's token; its pages are built by
/// script). Run it on every page load (wry: `with_initialization_script`;
/// Android: `evaluateJavascript` when the page has finished).
pub const PRESS_DOWNLOAD_JS: &str = r#"(function () {
  if (window.top !== window || window.__bmseekerPressed) return;
  if (!/(^|\.)(onedrive\.live\.com|1drv\.ms|sharepoint\.com)$/i.test(location.hostname)) return;
  var name = /^\s*(download|ダウンロード|다운로드|下载|下載)\s*$/i;
  var tries = 0;
  var timer = setInterval(function () {
    if (++tries > 120) return clearInterval(timer);
    var all = document.querySelectorAll('button, [role="button"], [role="menuitem"]');
    for (var i = 0; i < all.length; i++) {
      var b = all[i];
      var label = b.getAttribute("aria-label") || b.textContent || "";
      if (name.test(label) && !b.disabled && b.offsetParent !== null) {
        window.__bmseekerPressed = true;
        clearInterval(timer);
        b.click();
        return;
      }
    }
  }, 500);
})();"#;

/// Google Drive's answer for a file downloaded too often lately.
pub(crate) const DRIVE_QUOTA: &str =
    "Google Drive: the file was downloaded too often lately (try again later, up to a day)";

/// Whether a page's first bytes are Google Drive's "Quota exceeded" (also
/// answered 403: "Too many users have viewed or downloaded this file
/// recently").
pub(crate) fn drive_quota(head: &[u8]) -> bool {
    let head = String::from_utf8_lossy(&head[..head.len().min(4096)]);
    head.contains("<title>Google Drive")
        && (head.contains("Quota exceeded")
            || head.contains("Too many users have viewed or downloaded this file recently"))
}

/// The last part of a URL's path, decoded (a file's name).
pub(crate) fn url_file_name(url: &str) -> Option<String> {
    let path = url.split(['?', '#']).next()?.trim_end_matches('/');
    let last = path.rsplit('/').next()?;
    if path.matches('/').count() < 3 {
        return None; // just the host
    }
    crate::text::safe_file_name(&crate::net::percent_decode(last))
}

/// The best of (name, item) among a folder's archives: ogg over wav, full
/// over BGA-only and patches, the first of equals.
fn pick_file<'a, T>(files: impl Iterator<Item = (&'a str, T)>) -> Option<T> {
    let score = |n: &str| {
        let n = n.to_lowercase();
        let has = |ws: &[&str]| ws.iter().any(|w| n.contains(w));
        let mut s = 0;
        if has(&["ogg"]) {
            s += 4;
        }
        if has(&["wav", "flac"]) {
            s -= 4;
        }
        if has(&["lq", "low", "lr2", "wmv", "mpg", "lite", "compact"]) {
            s -= 2;
        }
        if has(&[
            "bga only",
            "bga_only",
            "only bga",
            "bga単体",
            "patch",
            "差分",
            "追加",
        ]) {
            s -= 8;
        }
        s
    };
    let mut best: Option<(i32, T)> = None;
    for (name, item) in files {
        let s = score(name);
        if best.as_ref().is_none_or(|(b, _)| s > *b) {
            best = Some((s, item));
        }
    }
    best.map(|(_, item)| item)
}

fn between<'a>(s: &'a str, a: &str, b: &str) -> Option<&'a str> {
    let i = s.find(a)? + a.len();
    let j = s[i..].find(b)? + i;
    Some(&s[i..j])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::net::{run, Canned};

    /// `link` resolved with canned answers.
    pub(crate) fn resolve(link: &str, answers: &[(&str, &[u8])]) -> Result<Target, Fetch> {
        let http = Canned(
            answers
                .iter()
                .map(|(u, b)| (u.to_string(), Ok(b.to_vec())))
                .collect(),
        );
        run(&mut Resolver::new(link, None, &Context::default()), &http)
    }

    #[test]
    fn hosts_in_order() {
        let h = Hosts::default();
        assert_eq!(
            h.names(),
            [
                "drive",
                "dropbox",
                "manbow",
                "bmssearch-venue",
                "bmssearch-page",
                "mediafire",
                "mega",
                "browser",
                "direct"
            ]
        );
        let at = |u: &str| h.find(&Link::parse(u, 0).unwrap()).map(|h| h.name());
        assert_eq!(at("https://drive.google.com/file/d/X/view"), Some("drive"));
        assert_eq!(at("https://1drv.ms/u/c/x"), Some("browser"));
        assert_eq!(at("http://example.com/a.zip"), Some("direct"));
        assert!(Link::parse("not a link", 0).is_none());
        assert_eq!(
            Hosts::default()
                .without("direct")
                .find(&Link::parse("http://e/a.zip", 0).unwrap())
                .map(|h| h.name()),
            None
        );
    }

    #[test]
    fn simple_links() {
        assert_eq!(
            resolve("http://example.com/a.zip", &[]),
            Ok(Target::url("http://example.com/a.zip"))
        );
        assert_eq!(resolve("https://1drv.ms/u/c/x", &[]), Err(Fetch::Page));
        assert_eq!(resolve("not a link", &[]), Err(Fetch::Page));
        // (no canned answer for its page)
        let page = "https://www.mediafire.com/file/x/a.rar/file";
        assert_eq!(
            resolve(page, &[]),
            Err(Fetch::Failed(NetError::Other(format!(
                "no answer for {page}"
            ))))
        );
        assert_ne!(resolve(page, &[]), Err(Fetch::Failed("other".into())));
    }

    #[test]
    fn picks_files() {
        let names = ["song_wav.zip", "song_ogg.zip", "bga only.zip"];
        assert_eq!(
            pick_file(names.iter().map(|n| (*n, *n))),
            Some("song_ogg.zip")
        );
    }
}
