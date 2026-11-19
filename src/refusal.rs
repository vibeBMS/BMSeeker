//! Services that refused BMSeeker. A server that answers 401, 403, 429 or
//! 451, or with a browser check (a Cloudflare challenge page; where an
//! answer failed, a Turnstile widget too), has refused it: its service is
//! not asked again for the rest of the run (no other query, no other URL on
//! it, no other headers), and the run goes on with the other sources and
//! hosts. A `Retry-After` is kept to by not asking again in this run at
//! all.
//!
//! [`Refusals`] is a run's memory of them, in its
//! [`Context`](crate::hosts::Context), so every source, host and job of the
//! run sees it. [`Guarded`] keeps a driver's effects to it (a transport,
//! `native::Native` or a web page's, keeps to nothing itself); a driver that
//! answers later (asynchronously) keeps to it with [`Refusals::before`] and
//! [`Refusals::after`]. What refused is told to the run's progress and, by
//! the command-line tool, written as a `refused` record.

use crate::hosts::Context;
use crate::install::job::{Driver, Effect, Fed};
use crate::net::NetError;
use crate::progress::Progress;
use std::sync::{Arc, Mutex};

/// Why a service is not asked.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Why {
    /// It answered this HTTP status (401, 403, 429, 451).
    Status(u16),
    /// It answered with a browser check (a Cloudflare challenge page,
    /// Turnstile).
    Challenge,
    /// Its API said it limits BMSeeker's requests (MEGA's `-4`).
    RateLimited,
    /// It refused in an earlier run of this session (`--skip-host`).
    Earlier,
}

/// A service that refused, by its host ([`service`]).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Refusal {
    pub host: String,
    pub why: Why,
}

impl Refusal {
    /// `url`'s service refused for `why`.
    pub(crate) fn of(url: &str, why: Why) -> Refusal {
        Refusal {
            host: service(url),
            why,
        }
    }

    /// What it was, for a program: `status`, `challenge`, `rate_limited`,
    /// `earlier`.
    pub fn code(&self) -> &'static str {
        match self.why {
            Why::Status(_) => "status",
            Why::Challenge => "challenge",
            Why::RateLimited => "rate_limited",
            Why::Earlier => "earlier",
        }
    }

    /// The HTTP status it answered, if that was it.
    pub fn status(&self) -> Option<u16> {
        match self.why {
            Why::Status(n) => Some(n),
            _ => None,
        }
    }
}

impl std::fmt::Display for Refusal {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let h = &self.host;
        match self.why {
            Why::Status(n) => write!(f, "{h} refused BMSeeker (HTTP {n})"),
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

/// The file hosts: their 401, 403 and 451 are a file's (Google Drive's
/// quota, a file shared privately), not a refusal of BMSeeker, unless the
/// answer says so ([`blocked`]); their 429 is one.
const FILE_HOSTS: &[&str] = &[
    "drive.google.com",
    "mega.nz",
    "dropbox.com",
    "mediafire.com",
    "pixeldrain.com",
];

/// Whether `status` from `url` refuses BMSeeker itself: 429 anywhere; 401,
/// 403 and 451 but from a file host (there, one file's or link's
/// failure).
pub(crate) fn refuses(url: &str, status: u16) -> bool {
    match status {
        429 => true,
        401 | 403 | 451 => !FILE_HOSTS.contains(&service(url).as_str()),
        _ => false,
    }
}

/// The marks of a page that is a browser check whatever its status:
/// Cloudflare's challenge page ("Just a moment...", `_cf_chl_opt`) and
/// Google's "unusual traffic" CAPTCHA (`/sorry/`), Google refusing
/// automated requests. (Cloudflare's script on ordinary pages,
/// `/cdn-cgi/challenge-platform/`, is no check.)
const CHECKS: &[&[u8]] = &[
    b"<title>just a moment...</title>",
    b"_cf_chl_opt",
    b"our systems have detected unusual traffic",
    b"sending automated queries",
    b"/sorry/index",
];

/// A Turnstile widget's script: a check where an answer failed, but no
/// refusal on a page that came (beside a comment or login form).
const TURNSTILE: &[u8] = b"challenges.cloudflare.com";

/// Whether an answer that came (a success) is a browser check: a page of
/// [`CHECKS`].
pub(crate) fn checked(body: &[u8]) -> bool {
    html_has(body, CHECKS)
}

/// Whether an answer that failed refuses the client whatever its status:
/// Cloudflare's `cf-mitigated: challenge` header, a page of [`CHECKS`] or
/// with a Turnstile widget.
pub(crate) fn blocked(cf_mitigated: Option<&str>, body: &[u8]) -> bool {
    cf_mitigated.is_some_and(|v| v.trim().eq_ignore_ascii_case("challenge"))
        || checked(body)
        || html_has(body, &[TURNSTILE])
}

/// The error of an answer of `status` to `url` that is no success (or
/// flagged `cf-mitigated`), its redirects ending `at`, with its
/// `cf-mitigated` header and its first bytes `head`: a browser check is a
/// refusal of the service ([`refused_at`]); Google Drive's quota is said as
/// such; else the status.
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
    NetError::Status(status)
}

/// What refused when `url` was asked: where its redirects ended `at` (a
/// short link's host did not), but Google's CAPTCHA, which is the service
/// asked refusing.
fn refused_at<'a>(url: &'a str, at: Option<&'a str>) -> &'a str {
    at.filter(|a| !a.contains("google.com/sorry"))
        .unwrap_or(url)
}

/// The hosts of one service: a refusal by one is the others' too.
const SERVICES: &[(&str, &[&str])] = &[
    (
        "drive.google.com",
        &[
            "drive.google.com",
            "drive.usercontent.google.com",
            "docs.google.com",
        ],
    ),
    ("mega.nz", &["mega.nz", "mega.co.nz", "mega.io"]),
    (
        "dropbox.com",
        &["dropbox.com", "dropboxusercontent.com", "db.tt"],
    ),
    ("mediafire.com", &["mediafire.com"]),
    ("pixeldrain.com", &["pixeldrain.com", "pixeldrain.net"]),
    ("bmssearch.net", &["bmssearch.net"]),
];

/// The service a URL (or a host name) is on: its host, lower case,
/// without `www.` and a port; one name for the hosts of a service
/// BMSeeker knows (Google Drive's, MEGA's, Dropbox's, MediaFire's,
/// pixeldrain's, BMS SEARCH's).
pub(crate) fn service(url: &str) -> String {
    let rest = url.split_once("://").map_or(url, |(_, r)| r);
    let host = rest.split(['/', '?', '#']).next().unwrap_or("");
    let host = host.rsplit('@').next().unwrap_or(host);
    let host = host.split(':').next().unwrap_or(host).to_ascii_lowercase();
    let host = host.strip_prefix("www.").unwrap_or(&host);
    let under = |d: &str| host == d || host.ends_with(&format!(".{d}"));
    SERVICES
        .iter()
        .find(|(_, hosts)| hosts.iter().any(|d| under(d)))
        .map_or_else(|| host.to_string(), |(name, _)| name.to_string())
}

/// Whether `body` is a page (HTML) with one of `marks` (lower case) in its
/// first 256 KiB, in any case.
fn html_has(body: &[u8], marks: &[&[u8]]) -> bool {
    let page = body
        .strip_prefix(b"\xEF\xBB\xBF")
        .unwrap_or(body)
        .trim_ascii_start();
    if !page.starts_with(b"<") {
        return false;
    }
    let page = &page[..page.len().min(256 * 1024)];
    marks
        .iter()
        .any(|m| page.windows(m.len()).any(|w| w.eq_ignore_ascii_case(m)))
}

/// A run's (or a session's) services that refused. Shared through the
/// run's [`Context`](crate::hosts::Context); keep one context for as long
/// as refusals should be kept to.
#[derive(Debug, Default)]
pub struct Refusals(Mutex<Vec<(Refusal, bool)>>);

impl Refusals {
    /// Whether `url`'s service refused: how.
    fn of(&self, url: &str) -> Option<Refusal> {
        let host = service(url);
        let all = self.0.lock().unwrap();
        all.iter()
            .find(|(r, _)| r.host == host)
            .map(|(r, _)| r.clone())
    }

    /// `r` kept: its service is not asked again. The refusal kept for it
    /// (the first, when it had refused already).
    pub(crate) fn keep(&self, r: Refusal) -> Refusal {
        let mut all = self.0.lock().unwrap();
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
        let mut all = self.0.lock().unwrap();
        if !all.iter().any(|(k, _)| k.host == r.host) {
            all.push((r, true));
        }
    }

    /// Every refusal kept.
    #[cfg(test)]
    pub(crate) fn all(&self) -> Vec<Refusal> {
        self.0
            .lock()
            .unwrap()
            .iter()
            .map(|(r, _)| r.clone())
            .collect()
    }

    /// The refusals kept since the last call (to tell a person once).
    fn news(&self) -> Vec<Refusal> {
        let mut all = self.0.lock().unwrap();
        all.iter_mut()
            .filter(|(_, told)| !*told)
            .map(|(r, told)| {
                *told = true;
                r.clone()
            })
            .collect()
    }

    /// Before `effect` is done: what it gets at once when its service
    /// refused ([`NetError::Refused`], told to `progress`), else nothing
    /// (do it, then [`Refusals::after`]).
    pub fn before(&self, effect: &Effect, progress: Option<&Progress>) -> Option<Fed> {
        let (url, http) = match effect {
            Effect::Http(r) => (r.url(), true),
            Effect::Download { target, .. } => (target.url.as_str(), false),
            Effect::Sleep(_) => return None,
        };
        Some(failed(self.of(url)?, http, progress))
    }

    /// What came of asking `url` (a request, or a download): an answer
    /// that refuses (a status [`refuses`], a browser check) is kept and
    /// fails so, told to `progress`.
    pub fn after(&self, url: &str, fed: Fed, progress: Option<&Progress>) -> Fed {
        let of_error = |e: &NetError, from: &str| match e {
            NetError::Status(n) if refuses(from, *n) => Some(Refusal::of(from, Why::Status(*n))),
            NetError::Refused(r) => Some(r.clone()),
            _ => None,
        };
        let (refusal, http) = match &fed {
            Fed::Http(Err(e)) => (of_error(e, url), true),
            Fed::Http(Ok(body)) => (
                checked(body).then(|| Refusal::of(url, Why::Challenge)),
                true,
            ),
            Fed::Downloaded(Err(u)) => {
                (of_error(&u.error, refused_at(url, u.at.as_deref())), false)
            }
            Fed::Downloaded(Ok(s)) => (
                checked(&s.head)
                    .then(|| Refusal::of(refused_at(url, s.at.as_deref()), Why::Challenge)),
                false,
            ),
            Fed::Woke => (None, false),
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
    let e = NetError::Refused(r);
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
            progress: cx.progress(),
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
                let url = match &effect {
                    Effect::Http(r) => r.url().to_string(),
                    Effect::Download { target, .. } => target.url.clone(),
                    Effect::Sleep(_) => String::new(),
                };
                self.refusals.after(&url, self.net.perform(effect), p)
            }
        };
        self.tell();
        fed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hosts::Target;
    use crate::install::job::{Served, Unserved};
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
        // Google's CAPTCHA
        let sorry = br#"<html><head><title>https://drive.usercontent.google.com/download</title></head><body>
            <div>Our systems have detected unusual traffic from your computer network.</div>
            <form id="captcha-form" action="index" method="post"></form></body></html>"#;
        assert!(checked(sorry) && blocked(None, sorry));
        let at = "https://www.google.com/sorry/index?continue=x";
        assert!(matches!(
            answer_error("https://drive.google.com/x", at, 429, None, sorry),
            NetError::Refused(r) if r.host == "drive.google.com"
        ));
    }

    #[test]
    fn file_hosts_403_is_a_files() {
        let drive = "https://drive.usercontent.google.com/download?id=1";
        assert!(!refuses(drive, 403) && !refuses(drive, 401) && !refuses(drive, 451));
        assert!(refuses(drive, 429));
        for u in [
            "https://g.api.mega.co.nz/cs?id=0",
            "https://dl.dropboxusercontent.com/s/a",
            "https://download1.mediafire.com/x/a.rar",
            "https://pixeldrain.net/api/filesystem/x",
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
        Effect::Http(Request::Get(url.into()))
    }

    fn download(url: &str) -> Effect {
        Effect::Download {
            target: Target::url(url),
            slot: "slot".into(),
            any: false,
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
            g.perform(download("https://fine.test/1.zip")),
            Fed::Downloaded(Ok(_))
        ));
        // a page that came with a Turnstile widget (a form) is no refusal
        assert!(matches!(
            g.perform(get("https://widget.test/p")),
            Fed::Http(Ok(_))
        ));
        assert!(matches!(
            g.perform(get("https://widget.test/q")),
            Fed::Http(Ok(_))
        ));
        assert_eq!(
            *net.asked.borrow(),
            [
                "https://busy.test/1.zip",
                "https://checked.test/1.zip",
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
}
