//! Services that refused BMSeeker. A server that answers 429, or 401, 403
//! or 451 where BMSeeker asks a service's own page, API or listing, or
//! with a browser check (a Cloudflare challenge page; where an answer
//! failed, a Turnstile widget too), has refused it: its service is not
//! asked again for the rest of the run (no other query, no other URL on
//! it, no other headers), and the run goes on with the other sources and
//! hosts. A `Retry-After` is kept to by not asking again in this run at
//! all.
//!
//! What refuses is the service's own word, not what one link or table can
//! make it say: a 401, 403 or 451 for a file a link leads to, or for a
//! table's header or data, is that file's failure (any server has an
//! address that answers so), and a file that came is a browser check
//! only where no file was sent (no name, no file host). But a host that
//! answers so for [`STRIKES`] different files' addresses, with no answer
//! of its own that came between them, refuses BMSeeker its files: it is
//! not asked for the rest of a table's, one after another. (Not a file
//! host: there each is one file's quota or sharing, however many.) The
//! service charged is the one that answered, where the redirects ended.
//!
//! [`Refusals`] is a run's memory of them, in its
//! [`Context`](crate::hosts::Context), so every source, host and job of the
//! run sees it. [`Guarded`] keeps a driver's effects to it (a transport,
//! `native::Native` or a web page's, keeps to nothing itself); a driver that
//! answers later (asynchronously) keeps to it with [`Refusals::before`] and
//! [`Refusals::after`]. What refused is told to the run's progress and, by
//! the command-line tool, written as a `refused` record.

use crate::hosts::{Context, Link, NOT_WEB};
use crate::net::job::{Driver, Effect, Fed};
use crate::net::{Kind, NetError};
pub use crate::net::{Refusal, Why};
use crate::progress::Progress;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

impl Refusal {
    /// `url`'s service refused for `why`.
    pub(crate) fn of(url: &str, why: Why) -> Refusal {
        Refusal {
            host: service(url),
            why,
        }
    }
}

impl std::fmt::Display for Refusal {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let h = &self.host;
        match self.why {
            Why::Status(n) => write!(f, "{h} refused BMSeeker (HTTP {n})"),
            Why::Files(n) => write!(
                f,
                "{h} refused BMSeeker {STRIKES} files one after another (HTTP {n})"
            ),
            Why::Challenge => write!(
                f,
                "{h} answered with a browser check (a challenge or CAPTCHA page)"
            ),
            Why::RateLimited => write!(f, "{h} limits BMSeeker's requests"),
            Why::Earlier => write!(f, "{h} refused BMSeeker earlier"),
        }?;
        f.write_str(": skipped for the rest of this run")
    }
}

/// Whether `status` from `url` refuses BMSeeker itself: 429 anywhere; 401,
/// 403 and 451 from what BMSeeker asks a service as its own (`endpoint`: a
/// page, an API, a listing; not a file a link names, nor a table's header
/// or data, which are where the table says), but from a file host (there,
/// one file's or link's failure, unless the answer says otherwise:
/// [`blocked`]).
pub(crate) fn refuses(url: &str, status: u16, endpoint: bool) -> bool {
    match status {
        429 => true,
        401 | 403 | 451 => endpoint && !file_host(url),
        _ => false,
    }
}

/// The files' addresses a host may answer 401, 403 or 451 for, one after
/// another, before it has refused BMSeeker its files ([`Why::Files`]):
/// one link (or two) to an address that answers so says nothing of the
/// host, a table's every file answered so does.
pub const STRIKES: usize = 3;

/// Whether `url` is on a file host ([`hosts::service_of`](crate::hosts)).
fn file_host(url: &str) -> bool {
    crate::hosts::service_of(crate::net::host(url)).is_some_and(|s| s.file_host)
}

/// Whether a request of `kind` asks a service's own endpoint
/// ([`refuses`]).
fn endpoint(kind: Kind) -> bool {
    !matches!(kind, Kind::Header | Kind::TableData)
}

/// The mark of a page that came (a success) and is a browser check:
/// Cloudflare's challenge page, by the title it begins with. A title is
/// the site's own; what a page shows further down of others' words (a
/// work's comment, a stored file's text) makes no check of it.
const TITLE: &[u8] = b"<title>just a moment...</title>";

/// Where a page's title is looked for (a challenge page's is in its
/// first hundred bytes).
const TITLE_WITHIN: usize = 1024;

/// The marks of an answer that failed and is a browser check whatever its
/// status: Cloudflare's challenge page ([`TITLE`], `_cf_chl_opt`) and
/// Google's "unusual traffic" CAPTCHA (`/sorry/`), Google refusing
/// automated requests. (Cloudflare's script on ordinary pages,
/// `/cdn-cgi/challenge-platform/`, is no check.)
const CHECKS: &[&[u8]] = &[
    TITLE,
    b"_cf_chl_opt",
    b"our systems have detected unusual traffic",
    b"sending automated queries",
    b"/sorry/index",
];

/// A Turnstile widget's script: a check where an answer failed, but no
/// refusal on a page that came (beside a comment or login form).
const TURNSTILE: &[u8] = b"challenges.cloudflare.com";

/// Where a failed answer's marks are looked for.
const MARKS_WITHIN: usize = 256 * 1024;

/// Whether an answer that came (a success) is a browser check: a page
/// that begins with [`TITLE`].
pub(crate) fn checked(body: &[u8]) -> bool {
    html_has(body, &[TITLE], TITLE_WITHIN)
}

/// Whether an answer that failed refuses the client whatever its status:
/// Cloudflare's `cf-mitigated: challenge` header, a page of [`CHECKS`] or
/// with a Turnstile widget.
pub(crate) fn blocked(cf_mitigated: Option<&str>, body: &[u8]) -> bool {
    cf_mitigated.is_some_and(|v| v.trim().eq_ignore_ascii_case("challenge"))
        || html_has(body, CHECKS, MARKS_WITHIN)
        || html_has(body, &[TURNSTILE], MARKS_WITHIN)
}

/// The error of an answer of `status` to `url` that is no success (or
/// flagged `cf-mitigated`), its redirects ending `at`, with its
/// `cf-mitigated` header and its first bytes `head`: a browser check is a
/// refusal of the service ([`refused_at`]); Google Drive's quota is said as
/// such; a 429 of another service than the one asked (a short link's
/// redirect ended there) is that service's refusal; else the status.
pub(crate) fn answer_error(
    url: &str,
    at: &str,
    status: u16,
    mitigated: Option<&str>,
    head: &[u8],
) -> NetError {
    if blocked(mitigated, head) {
        return NetError::Refused(Refusal::of(refused_at(url, Some(at)), Why::Challenge));
    }
    if crate::hosts::drive_quota(head) {
        return NetError::Other(crate::hosts::DRIVE_QUOTA.into());
    }
    if status == 429 && service(at) != service(url) {
        return NetError::Refused(Refusal::of(at, Why::Status(status)));
    }
    NetError::Status(status)
}

/// What refused when `url` was asked: where its redirects ended `at` (a
/// short link's host did not), but Google's CAPTCHA, which is the service
/// asked refusing.
fn refused_at<'a>(url: &'a str, at: Option<&'a str>) -> &'a str {
    at.filter(|a| !a.contains("google.com/sorry"))
        .unwrap_or(url)
}

/// The service a URL (or a host name) is on: its host, lower case,
/// without `www.`; one name for the hosts of a service BMSeeker knows
/// (`hosts::service_of`: Google Drive's, MEGA's, Dropbox's, MediaFire's,
/// pixeldrain's, BMS SEARCH's).
pub(crate) fn service(url: &str) -> String {
    let host = crate::net::host(url).to_ascii_lowercase();
    let host = host.strip_prefix("www.").unwrap_or(&host);
    crate::hosts::service_of(host)
        .map_or(host, |s| s.name)
        .to_string()
}

/// Whether `body` is a page (HTML) with one of `marks` (lower case) in its
/// first `within` bytes, in any case.
fn html_has(body: &[u8], marks: &[&[u8]], within: usize) -> bool {
    let page = body
        .strip_prefix(b"\xEF\xBB\xBF")
        .unwrap_or(body)
        .trim_ascii_start();
    if !page.starts_with(b"<") {
        return false;
    }
    let page = &page[..page.len().min(within)];
    marks
        .iter()
        .any(|m| page.windows(m.len()).any(|w| w.eq_ignore_ascii_case(m)))
}

/// A run's (or a session's) services that refused. Shared through the
/// run's [`Context`](crate::hosts::Context); keep one context for as long
/// as refusals should be kept to.
#[derive(Debug, Default)]
pub struct Refusals {
    /// Each with whether it was told.
    kept: Mutex<Vec<(Refusal, bool)>>,
    /// By service, the files' addresses it answered 401, 403 or 451 for
    /// since its last answer that came ([`STRIKES`]).
    struck: Mutex<HashMap<String, Vec<String>>>,
}

impl Refusals {
    /// Whether `url`'s service refused: how.
    pub(crate) fn of(&self, url: &str) -> Option<Refusal> {
        let host = service(url);
        let all = self.kept.lock().unwrap();
        all.iter()
            .find(|(r, _)| r.host == host)
            .map(|(r, _)| r.clone())
    }

    /// `r` kept: its service is not asked again. The refusal kept for it
    /// (the first, when it had refused already).
    pub(crate) fn keep(&self, r: Refusal) -> Refusal {
        let mut all = self.kept.lock().unwrap();
        if let Some((first, _)) = all.iter().find(|(k, _)| k.host == r.host) {
            return first.clone();
        }
        all.push((r.clone(), false));
        r
    }

    /// A host that refused in an earlier run of this session (the
    /// window's, `--skip-host`): skipped, and not told again.
    pub fn earlier(&self, host: &str) {
        let r = Refusal::of(host, Why::Earlier);
        let mut all = self.kept.lock().unwrap();
        if !all.iter().any(|(k, _)| k.host == r.host) {
            all.push((r, true));
        }
    }

    /// Every refusal kept.
    #[cfg(test)]
    pub(crate) fn all(&self) -> Vec<Refusal> {
        self.kept
            .lock()
            .unwrap()
            .iter()
            .map(|(r, _)| r.clone())
            .collect()
    }

    /// The refusals kept since the last call (to tell a person once).
    fn news(&self) -> Vec<Refusal> {
        let mut all = self.kept.lock().unwrap();
        all.iter_mut()
            .filter(|(_, told)| !*told)
            .map(|(r, told)| {
                *told = true;
                r.clone()
            })
            .collect()
    }

    /// The file at `url` was answered `status` (401, 403, 451: its own
    /// failure): the refusal this makes, if it is the [`STRIKES`]th
    /// address its host (no file host) answered so one after another.
    fn strike(&self, url: &str, status: u16) -> Option<Refusal> {
        if !matches!(status, 401 | 403 | 451) || file_host(url) {
            return None;
        }
        let mut struck = self.struck.lock().unwrap();
        let files = struck.entry(service(url)).or_default();
        if !files.iter().any(|f| f == url) {
            files.push(url.to_string());
        }
        (files.len() >= STRIKES).then(|| Refusal::of(url, Why::Files(status)))
    }

    /// An answer of `url`'s host came: the files it refused before count
    /// no more.
    fn answered(&self, url: &str) {
        self.struck.lock().unwrap().remove(&service(url));
    }

    /// Before `effect` is done: what it gets at once when its service
    /// refused ([`NetError::Refused`], told to `progress`) or its address
    /// is no web link ([`NOT_WEB`]: whatever asks, a host's rules and the
    /// request agree on where it goes), else nothing (do it, then
    /// [`Refusals::after`]).
    pub fn before(&self, effect: &Effect, progress: Option<&Progress>) -> Option<Fed> {
        let (url, http) = match effect {
            Effect::Http(r) => (r.url(), true),
            Effect::Download { target, .. } => (target.url.as_str(), false),
            Effect::Sleep(_) => return None,
        };
        if Link::parse(url, 0).is_none() {
            return Some(fed(NetError::Other(NOT_WEB.into()), http));
        }
        Some(failed(self.of(url)?, http, progress))
    }

    /// What came of `effect` (a request, or a download): an answer that
    /// refuses (a status [`refuses`], a browser check, the [`STRIKES`]th
    /// file refused one after another) is kept and fails so, told to
    /// `progress`.
    pub fn after(&self, effect: &Effect, fed: Fed, progress: Option<&Progress>) -> Fed {
        // (`file`: what is asked is a file, not an endpoint of the
        // service's own)
        let of_error = |e: &NetError, from: &str, file: bool| match e {
            NetError::Status(n) if refuses(from, *n, !file) => {
                Some(Refusal::of(from, Why::Status(*n)))
            }
            NetError::Status(n) if file => self.strike(from, *n),
            NetError::Refused(r) => Some(r.clone()),
            _ => None,
        };
        let came = |check: bool, from: &str| {
            if !check {
                self.answered(from);
            }
            check.then(|| Refusal::of(from, Why::Challenge))
        };
        let (refusal, http) = match (effect, &fed) {
            (Effect::Http(r), Fed::Http(Err(e))) => {
                (of_error(e, r.url(), !endpoint(r.kind())), true)
            }
            // (a page asked for that came as a check; a table's or an
            // API's JSON is whatever it is)
            (Effect::Http(r), Fed::Http(Ok(body))) => {
                let check = r.kind() == Kind::Page && checked(body);
                (came(check, r.url()), true)
            }
            (Effect::Download { target, .. }, Fed::Downloaded(Err(u))) => {
                let at = refused_at(&target.url, u.at.as_deref());
                (of_error(&u.error, at, true), false)
            }
            // (a file a host stores is its uploader's, whatever is in it)
            (Effect::Download { target, .. }, Fed::Downloaded(Ok(s))) => {
                let at = refused_at(&target.url, s.at.as_deref());
                let check = s.name.is_none() && !file_host(at) && checked(&s.head);
                (came(check, at), false)
            }
            _ => (None, false),
        };
        match refusal {
            Some(r) => failed(self.keep(r), http, progress),
            None => fed,
        }
    }
}

/// A request's (`http`) or a download's failure for refusal `r`, told to
/// `progress`.
fn failed(r: Refusal, http: bool, progress: Option<&Progress>) -> Fed {
    if let Some(p) = progress {
        p.refused(&r.host);
    }
    fed(NetError::Refused(r), http)
}

/// A request's (`http`) or a download's failure `e`.
fn fed(e: NetError, http: bool) -> Fed {
    match http {
        true => Fed::Http(Err(e)),
        false => Fed::Downloaded(Err(e.into())),
    }
}

/// A driver kept to a run's refusals ([`Refusals::before`],
/// [`Refusals::after`]): a service that refused is not asked again. Each
/// new refusal goes to `told` (the command-line tool's `refused` records).
pub struct Guarded<'a> {
    net: &'a dyn Driver,
    refusals: &'a Refusals,
    progress: Option<Arc<Progress>>,
    told: &'a dyn Fn(&Refusal),
}

impl<'a> Guarded<'a> {
    /// `net` kept to `cx`'s refusals, telling `cx`'s progress.
    pub fn new(net: &'a dyn Driver, cx: &'a Context) -> Guarded<'a> {
        fn untold(_: &Refusal) {}
        Guarded {
            net,
            refusals: &cx.refused,
            progress: cx.progress.clone(),
            told: &untold,
        }
    }

    /// Each new refusal told to `told` as it comes.
    pub fn telling(self, told: &'a dyn Fn(&Refusal)) -> Guarded<'a> {
        Guarded { told, ..self }
    }

    /// The refusals kept since the last effect (one a task kept from an
    /// answer: MEGA's `-4`) to `told`.
    pub fn tell(&self) {
        for r in self.refusals.news() {
            (self.told)(&r);
        }
    }
}

impl Driver for Guarded<'_> {
    fn perform(&self, effect: Effect) -> Fed {
        let p = self.progress.as_deref();
        let fed = match self.refusals.before(&effect, p) {
            Some(fed) => fed,
            None => {
                let asked = effect.clone();
                self.refusals.after(&asked, self.net.perform(effect), p)
            }
        };
        self.tell();
        fed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::net::job::{Served, Target, Unserved};
    use crate::net::Request;
    use std::cell::RefCell;

    #[test]
    fn services() {
        assert_eq!(service("https://api.bmssearch.net/v1/x"), "bmssearch.net");
        assert_eq!(service("https://venue.bmssearch.net/e/1"), "bmssearch.net");
        assert_eq!(
            service("https://drive.usercontent.google.com/download?id=1"),
            "drive.google.com"
        );
        assert_eq!(service("https://g.api.mega.co.nz/cs?id=0"), "mega.nz");
        // where Drive's downloads end: Drive's, a file host
        let dl = "https://doc-0s-docs.googleusercontent.com/docs/securesc/x";
        assert_eq!(service(dl), "drive.google.com");
        assert!(!refuses(dl, 403, true) && refuses(dl, 429, true));
        assert_eq!(
            service("https://dl.dropboxusercontent.com/s/a"),
            "dropbox.com"
        );
        assert_eq!(
            service("http://download1527.mediafire.com/x"),
            "mediafire.com"
        );
        assert_eq!(service("https://WWW.Example.com:8080/a"), "example.com");
        assert_eq!(service("gingerrush.com"), "gingerrush.com");
        // a shared host's sites are their own
        assert_eq!(service("http://a.web.fc2.com/"), "a.web.fc2.com");
        assert_ne!(
            service("http://a.web.fc2.com/"),
            service("http://b.web.fc2.com/")
        );
    }

    const TURNSTILE_PAGE: &[u8] = br#"<html><body><h1>Comments</h1><form><div class="cf-turnstile"></div></form>
        <a href="song.zip">song.zip</a>
        <script src="https://challenges.cloudflare.com/turnstile/v0/api.js" async></script></body></html>"#;

    #[test]
    fn challenge_pages() {
        let page = br#"<!DOCTYPE html><html lang="en-US"><head><title>Just a moment...</title>
            <script>window._cf_chl_opt={cvId: '3'}</script></head></html>"#;
        assert!(checked(page) && blocked(None, page));
        // a Turnstile widget: a check where the answer failed only
        assert!(!checked(TURNSTILE_PAGE) && blocked(None, TURNSTILE_PAGE));
        assert!(blocked(Some("challenge"), b"{}") && !checked(b"{}"));
        // an ordinary page that Cloudflare serves (its script in it)
        let ordinary = br#"<html><head><meta name="bmstable" content="header.json"></head><body>
            <script>var u='/cdn-cgi/challenge-platform/scripts/jsd/main.js?ray=a5'</script></body></html>"#;
        assert!(!blocked(None, ordinary) && !checked(ordinary));
        assert!(!blocked(Some("managed"), ordinary));
        // an archive or JSON that names it is no page
        assert!(!blocked(None, b"PK\x03\x04 challenges.cloudflare.com"));
        assert!(!blocked(None, br#"{"t":"Just a moment..."}"#));
        // Google's CAPTCHA (an answer that failed: 429)
        let sorry = br#"<html><head><title>https://drive.usercontent.google.com/download</title></head><body>
            <div>Our systems have detected unusual traffic from your computer network.</div>
            <form id="captcha-form" action="index" method="post"></form></body></html>"#;
        assert!(blocked(None, sorry));
        // a page that came is a check by its title only: what a page shows
        // of others' words (a comment, a file's text) is no check
        let quoted = br#"<html><head><title>Work 12</title></head><body><p>window._cf_chl_opt
            Our systems have detected unusual traffic. /sorry/index
            &lt;title&gt;Just a moment...&lt;/title&gt;</p></body></html>"#;
        assert!(!checked(sorry) && !checked(quoted) && blocked(None, quoted));
        let deep = [
            &b"<html><head><title>Work 12</title></head><body>"[..],
            &[b' '; 2000],
            TITLE,
        ]
        .concat();
        assert!(!checked(&deep) && blocked(None, &deep));
        let at = "https://www.google.com/sorry/index?continue=x";
        assert!(matches!(
            answer_error("https://drive.google.com/x", at, 429, None, sorry),
            NetError::Refused(r) if r.host == "drive.google.com"
        ));
    }

    #[test]
    fn file_hosts_403_is_a_files() {
        let refuses = |u: &str, status: u16| refuses(u, status, true);
        let drive = "https://drive.usercontent.google.com/download?id=1";
        assert!(!refuses(drive, 403) && !refuses(drive, 401) && !refuses(drive, 451));
        assert!(refuses(drive, 429));
        for u in [
            "https://g.api.mega.co.nz/cs?id=0",
            "https://dl.dropboxusercontent.com/s/a",
            "https://download1.mediafire.com/x/a.rar",
            "https://pixeldrain.net/api/filesystem/x",
            "https://raw.githubusercontent.com/vibeBMS/BMS/main/events.json",
        ] {
            assert!(!refuses(u, 403) && refuses(u, 429), "{u}");
        }
        for u in [
            "https://api.bmssearch.net/v1/x",
            "https://stellabms.xyz/score_upload_full.json",
            "https://manbow.nothing.sh/event/event.cgi",
            "https://gingerrush.com/download/package/x",
            "https://bms.alvorna.com/api/hash?md5=x",
            "https://darksabun.club/table/archive/insane1/",
        ] {
            assert!(refuses(u, 403) && refuses(u, 401) && refuses(u, 451), "{u}");
        }
        assert!(!refuses("https://api.bmssearch.net/v1/x", 404));
        // what is no endpoint of a service's own (a link's file, a table's
        // data): its 403 is that file's, a 429 the service's
        let file = "https://darksabun.club/table/archive/";
        assert!(!super::refuses(file, 403, false) && super::refuses(file, 429, false));
    }

    /// Answers each request with `answer`, noting the URLs asked.
    struct Asked<'a> {
        asked: RefCell<Vec<String>>,
        answer: &'a dyn Fn(&str) -> Fed,
    }

    impl Driver for Asked<'_> {
        fn perform(&self, effect: Effect) -> Fed {
            let url = match &effect {
                Effect::Http(r) => r.url().to_string(),
                Effect::Download { target, .. } => target.url.clone(),
                Effect::Sleep(_) => return Fed::Woke,
            };
            self.asked.borrow_mut().push(url.clone());
            (self.answer)(&url)
        }
    }

    fn get(url: &str) -> Effect {
        Effect::Http(Request::Get(url.into(), Kind::Api))
    }

    fn get_of(url: &str, kind: Kind) -> Effect {
        Effect::Http(Request::Get(url.into(), kind))
    }

    fn download(url: &str) -> Effect {
        Effect::Download {
            target: Target::url(url),
            slot: "slot".into(),
        }
    }

    #[test]
    fn a_refusal_stops_its_service() {
        let net = Asked {
            asked: RefCell::default(),
            answer: &|u| match u.contains("refusing") {
                true => Fed::Http(Err(NetError::Status(403))),
                false => Fed::Http(Ok(b"{}".to_vec())),
            },
        };
        let told = RefCell::new(Vec::new());
        let tell = |r: &Refusal| told.borrow_mut().push(r.to_string());
        let progress = Progress::new();
        let cx = Context::default().with_progress(progress.clone());
        let g = Guarded::new(&net, &cx).telling(&tell);
        let refused = |f: Fed| match f {
            Fed::Http(Err(NetError::Refused(r))) => r,
            f => panic!("{f:?}"),
        };
        let r = refused(g.perform(get("https://refusing.test/a?q=1")));
        assert_eq!(r.why, Why::Status(403));
        assert_eq!(
            r.to_string(),
            "refusing.test refused BMSeeker (HTTP 403): skipped for the rest of this run"
        );
        assert_eq!(NetError::Refused(r).code(), "refused");
        // other URLs and queries on it are not asked
        refused(g.perform(get("https://refusing.test/b?q=2")));
        refused(g.perform(get("http://www.refusing.test/c")));
        assert!(matches!(
            g.perform(download("https://refusing.test/s.zip")),
            Fed::Downloaded(Err(u)) if matches!(u.error, NetError::Refused(_))
        ));
        // the others are
        assert!(matches!(
            g.perform(get("https://other.test/a")),
            Fed::Http(Ok(_))
        ));
        assert_eq!(
            *net.asked.borrow(),
            ["https://refusing.test/a?q=1", "https://other.test/a"]
        );
        // told once
        assert_eq!(told.borrow().len(), 1);
        assert_eq!(progress.refusals(), ["refusing.test"]);
        // one a task kept from an answer (MEGA's -4) is told by the next
        // `tell`
        cx.refused
            .keep(Refusal::of("https://mega.nz/x", Why::RateLimited));
        g.tell();
        assert_eq!(told.borrow().len(), 2);
    }

    #[test]
    fn a_file_host_that_limits_and_a_challenge() {
        let net = Asked {
            asked: RefCell::default(),
            answer: &|u| match u {
                u if u.contains("busy") => Fed::Downloaded(Err(Unserved {
                    error: NetError::Status(429),
                    at: None,
                })),
                u if u.contains("checked") => Fed::Downloaded(Ok(Served::bytes(
                    b"<html><head><title>Just a moment...</title></head></html>".to_vec(),
                    None,
                    None,
                ))),
                u if u.contains("widget") => Fed::Http(Ok(TURNSTILE_PAGE.to_vec())),
                u if u.contains("fine.test/page") => Fed::Http(Ok(b"<html></html>".to_vec())),
                _ => Fed::Downloaded(Ok(Served::bytes(b"PK\x03\x04".to_vec(), None, None))),
            },
        };
        let told = RefCell::new(0);
        let tell = |_: &Refusal| *told.borrow_mut() += 1;
        let cx = Context::default();
        let g = Guarded::new(&net, &cx).telling(&tell);
        let error = |f: Fed| match f {
            Fed::Downloaded(Err(u)) => u.error,
            f => panic!("{f:?}"),
        };
        assert!(matches!(
            error(g.perform(download("https://busy.test/1.zip"))),
            NetError::Refused(Refusal {
                why: Why::Status(429),
                ..
            })
        ));
        assert!(matches!(
            error(g.perform(download("https://busy.test/2.zip"))),
            NetError::Refused(_)
        ));
        assert!(matches!(
            error(g.perform(download("https://checked.test/1.zip"))),
            NetError::Refused(Refusal {
                why: Why::Challenge,
                ..
            })
        ));
        assert!(matches!(
            g.perform(get("https://checked.test/api")),
            Fed::Http(Err(NetError::Refused(_)))
        ));
        assert!(matches!(
            g.perform(get_of("https://fine.test/page", Kind::Page)),
            Fed::Http(Ok(_))
        ));
        assert!(matches!(
            g.perform(download("https://fine.test/1.zip")),
            Fed::Downloaded(Ok(_))
        ));
        // a page that came with a Turnstile widget (a form) is no refusal
        assert!(matches!(
            g.perform(get_of("https://widget.test/p", Kind::Page)),
            Fed::Http(Ok(_))
        ));
        assert!(matches!(
            g.perform(get_of("https://widget.test/q", Kind::Page)),
            Fed::Http(Ok(_))
        ));
        assert_eq!(
            *net.asked.borrow(),
            [
                "https://busy.test/1.zip",
                "https://checked.test/1.zip",
                "https://fine.test/page",
                "https://fine.test/1.zip",
                "https://widget.test/p",
                "https://widget.test/q",
            ]
        );
        assert_eq!(*told.borrow(), 2);
        // a refusal from an earlier run: skipped, not told again
        cx.refused.earlier("other.test");
        assert!(matches!(
            g.perform(get("https://other.test/x")),
            Fed::Http(Err(NetError::Refused(_)))
        ));
        assert_eq!(*told.borrow(), 2);
        assert_eq!(cx.refused.all().len(), 3);
    }
    const CHECK_PAGE: &[u8] = b"<html><head><title>Just a moment...</title></head></html>";

    /// What a link or a table can make a server say refuses no service:
    /// an address (or two) that answers 403, a stored file that reads as
    /// a check.
    #[test]
    fn what_a_link_names_refuses_no_service() {
        let stored = |name: Option<&str>, at: &str| {
            Fed::Downloaded(Ok(Served::bytes(
                CHECK_PAGE.to_vec(),
                name.map(String::from),
                Some(at.into()),
            )))
        };
        let net = Asked {
            asked: RefCell::default(),
            answer: &|u| match u {
                // a folder without an index, a file shared privately
                u if u.contains("/closed/") && u.ends_with(".zip") => {
                    Fed::Downloaded(Err(NetError::Status(403).into()))
                }
                u if u.contains("/closed/") => Fed::Http(Err(NetError::Status(403))),
                // a file its uploader made to read as a browser check
                u if u.contains("drive.usercontent.google.com") => stored(None, u),
                u if u.contains("raw.githubusercontent.com") => stored(None, u),
                u if u.contains("sent-as-a-file") => stored(Some("a.html"), u),
                u if u.ends_with(".json") => Fed::Http(Ok(CHECK_PAGE.to_vec())),
                _ => Fed::Http(Ok(b"{}".to_vec())),
            },
        };
        let cx = Context::default();
        let g = Guarded::new(&net, &cx);
        for effect in [
            download("https://mirror.test/closed/a.zip"),
            get_of("https://mirror.test/closed/header.json", Kind::Header),
            get_of("https://other.test/closed/data.json", Kind::TableData),
            download("https://drive.usercontent.google.com/download?id=1"),
            download("https://raw.githubusercontent.com/a/b/main/c.zip"),
            download("https://host.test/sent-as-a-file"),
            get_of("https://host.test/header.json", Kind::Header),
            get_of("https://host.test/api.json", Kind::Api),
        ] {
            let fed = g.perform(effect.clone());
            let refused = matches!(
                &fed,
                Fed::Http(Err(NetError::Refused(_)))
                    | Fed::Downloaded(Err(Unserved {
                        error: NetError::Refused(_),
                        ..
                    }))
            );
            assert!(!refused, "{effect:?}");
        }
        assert!(cx.refused.all().is_empty(), "{:?}", cx.refused.all());
        // the service's own endpoints still say it: an API's or a page's
        // 403, a page asked for that came as a check
        assert!(matches!(
            g.perform(get_of("https://mirror.test/closed/api", Kind::Api)),
            Fed::Http(Err(NetError::Refused(_)))
        ));
        assert!(matches!(
            g.perform(get_of("https://page.test/table.json", Kind::Page)),
            Fed::Http(Err(NetError::Refused(_)))
        ));
        let hosts: Vec<String> = cx.refused.all().into_iter().map(|r| r.host).collect();
        assert_eq!(hosts, ["mirror.test", "page.test"]);
    }

    /// A host that answers 403 for file after file has refused BMSeeker
    /// its files: three different addresses one after another, no answer
    /// of its own coming between them. Never a file host.
    #[test]
    fn files_refused_one_after_another() {
        let net = Asked {
            asked: RefCell::default(),
            answer: &|u| match u {
                u if u.contains("/closed/") && u.ends_with(".json") => {
                    Fed::Http(Err(NetError::Status(403)))
                }
                u if u.contains("/closed/") || u.contains("drive.google.com") => {
                    Fed::Downloaded(Err(NetError::Status(403).into()))
                }
                u if u.contains("/moved/") => Fed::Downloaded(Err(Unserved {
                    error: NetError::Status(451),
                    at: Some(u.replace("short.test/moved", "host.test/closed")),
                })),
                _ => Fed::Downloaded(Ok(Served::bytes(b"PK\x03\x04".to_vec(), None, None))),
            },
        };
        let told = RefCell::new(Vec::new());
        let tell = |r: &Refusal| told.borrow_mut().push(r.clone());
        let cx = Context::default();
        let g = Guarded::new(&net, &cx).telling(&tell);
        let error = |f: Fed| match f {
            Fed::Downloaded(Err(u)) => Some(u.error),
            Fed::Http(Err(e)) => Some(e),
            _ => None,
        };
        let closed = |n: u32| download(&format!("https://host.test/closed/{n}.zip"));
        // one link's 403 refuses nothing, however often it is tried
        for _ in 0..5 {
            assert_eq!(error(g.perform(closed(1))), Some(NetError::Status(403)));
        }
        // nor a second; and an answer of the host's that came between
        // them and the next has them count no more
        assert_eq!(error(g.perform(closed(2))), Some(NetError::Status(403)));
        assert_eq!(
            error(g.perform(download("https://host.test/open/a.zip"))),
            None
        );
        assert_eq!(error(g.perform(closed(3))), Some(NetError::Status(403)));
        // (another host's are its own)
        for n in 1..=2 {
            let other = download(&format!("https://other.test/closed/{n}.zip"));
            assert_eq!(error(g.perform(other)), Some(NetError::Status(403)));
        }
        // a table's data there, then a third file (where a short link's
        // redirects ended): the host's refusal
        let data = get_of("https://www.host.test/closed/data.json", Kind::TableData);
        assert_eq!(error(g.perform(data)), Some(NetError::Status(403)));
        assert!(cx.refused.all().is_empty());
        let third = error(g.perform(download("https://short.test/moved/5.zip")));
        let Some(NetError::Refused(r)) = third else {
            panic!("{third:?}")
        };
        assert_eq!((r.host.as_str(), r.why), ("host.test", Why::Files(451)));
        assert_eq!((r.code(), r.status()), ("files", Some(451)));
        assert_eq!(
            r.to_string(),
            "host.test refused BMSeeker 3 files one after another (HTTP 451): \
             skipped for the rest of this run"
        );
        // not asked again, for a file that would come either; told once
        let asked = net.asked.borrow().len();
        for u in [
            "https://host.test/open/a.zip",
            "https://host.test/closed/6.zip",
        ] {
            assert!(matches!(
                error(g.perform(download(u))),
                Some(NetError::Refused(_))
            ));
        }
        assert_eq!(net.asked.borrow().len(), asked);
        assert_eq!(*told.borrow(), [r]);
        // a file host's 403s are each one file's (its quota, its
        // sharing), however many
        for n in 0..10 {
            let file = download(&format!("https://drive.google.com/uc?id={n}"));
            assert_eq!(error(g.perform(file)), Some(NetError::Status(403)));
        }
        assert_eq!(cx.refused.all().len(), 1);
    }

    /// A refusal is the service's that answered: where a short link's
    /// redirects ended.
    #[test]
    fn the_service_that_answered_is_charged() {
        let (short, drive) = (
            "https://bit.ly/x",
            "https://drive.usercontent.google.com/download?id=1",
        );
        // a request's answer, as a driver makes it of where it came from
        let limited = answer_error(short, drive, 429, None, b"");
        assert!(matches!(&limited, NetError::Refused(r) if r.host == "drive.google.com"));
        assert_eq!(
            answer_error(drive, drive, 429, None, b""),
            NetError::Status(429)
        );
        // a file's 403 behind the short link is nobody's refusal
        assert_eq!(
            answer_error(short, drive, 403, None, b""),
            NetError::Status(403)
        );
        let net = Asked {
            asked: RefCell::default(),
            answer: &|u| match u {
                u if u.contains("bit.ly/page") => Fed::Http(Err(limited.clone())),
                u if u.contains("bit.ly/gone") => Fed::Downloaded(Err(Unserved {
                    error: NetError::Status(403),
                    at: Some(drive.into()),
                })),
                u if u.contains("bit.ly") => Fed::Downloaded(Err(Unserved {
                    error: NetError::Status(429),
                    at: Some(drive.into()),
                })),
                _ => Fed::Downloaded(Ok(Served::bytes(b"PK\x03\x04".to_vec(), None, None))),
            },
        };
        for first in [download("https://bit.ly/file"), get("https://bit.ly/page")] {
            let cx = Context::default();
            let g = Guarded::new(&net, &cx);
            g.perform(download("https://bit.ly/gone"));
            assert!(cx.refused.all().is_empty());
            g.perform(first);
            let hosts: Vec<String> = cx.refused.all().into_iter().map(|r| r.host).collect();
            assert_eq!(hosts, ["drive.google.com"]);
            // Drive is not asked again; the short links' host is
            let asked = net.asked.borrow().len();
            assert!(matches!(
                g.perform(download(drive)),
                Fed::Downloaded(Err(u)) if matches!(u.error, NetError::Refused(_))
            ));
            assert_eq!(net.asked.borrow().len(), asked);
            g.perform(download("https://bit.ly/other"));
            assert_eq!(net.asked.borrow().len(), asked + 1);
        }
    }

    /// An address that is no plain web link is not asked, whoever asks.
    #[test]
    fn only_web_links_are_asked() {
        let net = Asked {
            asked: RefCell::default(),
            answer: &|_| Fed::Http(Err(NetError::Status(429))),
        };
        let cx = Context::default();
        let g = Guarded::new(&net, &cx);
        for u in [
            "https://evil.test\\@www.dropbox.com/s/a/b.zip",
            "https://user@host.test/a.json",
            "file:///etc/passwd",
            "ftp://host.test/a",
        ] {
            assert!(
                matches!(g.perform(get(u)), Fed::Http(Err(NetError::Other(e))) if e == NOT_WEB),
                "{u}"
            );
            assert!(matches!(
                g.perform(download(u)),
                Fed::Downloaded(Err(e)) if e.error == NetError::Other(NOT_WEB.into())
            ));
        }
        assert!(net.asked.borrow().is_empty() && cx.refused.all().is_empty());
    }
}
