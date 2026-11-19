//! Services that refused BMSeeker. A server that answers 401, 403, 429 or
//! 451, or with a browser check (a Cloudflare challenge page, a Turnstile
//! widget), has refused it: its service is not asked again for the rest of
//! the run (no other query, no other URL on it, no other headers), and the
//! run goes on with the other sources and hosts. A `Retry-After` is kept
//! to by not asking again in this run at all.
//!
//! [`Refusals`] is a run's memory of them, in its
//! [`Context`](crate::hosts::Context), so every source, host and job of the
//! run sees it. [`Refusals::guard`] keeps a driver's effects to it:
//! `native::Native` does natively, [`Guarded`] wraps any other driver (a
//! web page's). What refused is told to the run's progress and, by the
//! command-line tool, written as a `refused` record.

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
    pub fn of(url: &str, why: Why) -> Refusal {
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
            Why::Challenge => write!(f, "{h} answered with a browser check (Cloudflare)"),
            Why::RateLimited => write!(f, "{h} limits BMSeeker's requests"),
            Why::Earlier => write!(f, "{h} refused BMSeeker earlier"),
        }?;
        f.write_str(": skipped for the rest of this run")
    }
}

/// Whether an HTTP status refuses (401, 403, 429, 451).
pub fn refusing(status: u16) -> bool {
    matches!(status, 401 | 403 | 429 | 451)
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
pub fn service(url: &str) -> String {
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

/// Whether an answer is a browser check: Cloudflare's `cf-mitigated:
/// challenge` header, or a page with its challenge ("Just a moment...",
/// `_cf_chl_opt`) or a Turnstile widget (`challenges.cloudflare.com`).
/// (Cloudflare's script on ordinary pages, `/cdn-cgi/challenge-platform/`,
/// is no check.)
pub fn challenge(cf_mitigated: Option<&str>, body: &[u8]) -> bool {
    if cf_mitigated.is_some_and(|v| v.trim().eq_ignore_ascii_case("challenge")) {
        return true;
    }
    let page = body
        .strip_prefix(b"\xEF\xBB\xBF")
        .unwrap_or(body)
        .trim_ascii_start();
    if !page.starts_with(b"<") {
        return false;
    }
    let page = &page[..page.len().min(256 * 1024)];
    [
        &b"challenges.cloudflare.com"[..],
        b"<title>just a moment...</title>",
        b"_cf_chl_opt",
    ]
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
    pub fn of(&self, url: &str) -> Option<Refusal> {
        let host = service(url);
        let all = self.0.lock().unwrap();
        all.iter()
            .find(|(r, _)| r.host == host)
            .map(|(r, _)| r.clone())
    }

    /// `r` kept: its service is not asked again. The refusal kept for it
    /// (the first, when it had refused already).
    pub fn keep(&self, r: Refusal) -> Refusal {
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
    pub fn all(&self) -> Vec<Refusal> {
        self.0
            .lock()
            .unwrap()
            .iter()
            .map(|(r, _)| r.clone())
            .collect()
    }

    /// The refusals kept since the last call (to tell a person once).
    pub fn news(&self) -> Vec<Refusal> {
        let mut all = self.0.lock().unwrap();
        all.iter_mut()
            .filter(|(_, told)| !*told)
            .map(|(r, told)| {
                *told = true;
                r.clone()
            })
            .collect()
    }

    /// `effect` done by `raw`, kept to the refusals: one for a service
    /// that refused is not done but fails at once ([`NetError::Refused`]);
    /// an answer that refuses (a status of [`refusing`], a browser check)
    /// is kept and fails so. Either is told to `progress`.
    pub fn guard(
        &self,
        effect: Effect,
        progress: Option<&Progress>,
        raw: impl FnOnce(Effect) -> Fed,
    ) -> Fed {
        let (url, http) = match &effect {
            Effect::Http(r) => (r.url().to_string(), true),
            Effect::Download { target, .. } => (target.url.clone(), false),
            Effect::Sleep(_) => return raw(effect),
        };
        let failed = |r: Refusal| {
            if let Some(p) = progress {
                p.refused(&r.host);
            }
            let e = NetError::Refused(r);
            match http {
                true => Fed::Http(Err(e)),
                false => Fed::Downloaded(Err(e.into())),
            }
        };
        if let Some(r) = self.of(&url) {
            return failed(r);
        }
        let fed = raw(effect);
        let of_error = |e: &NetError| match e {
            NetError::Status(n) if refusing(*n) => Some(Refusal::of(&url, Why::Status(*n))),
            NetError::Refused(r) => Some(r.clone()),
            _ => None,
        };
        let refusal = match &fed {
            Fed::Http(Err(e)) => of_error(e),
            Fed::Http(Ok(body)) => challenge(None, body).then(|| Refusal::of(&url, Why::Challenge)),
            // (where the download's redirects ended refused)
            Fed::Downloaded(Err(u)) => of_error(&u.error).map(|r| match (&u.error, &u.at) {
                (NetError::Status(_), Some(at)) => Refusal::of(at, r.why),
                _ => r,
            }),
            Fed::Downloaded(Ok(s)) => challenge(None, &s.head)
                .then(|| Refusal::of(s.at.as_deref().unwrap_or(&url), Why::Challenge)),
            Fed::Woke => None,
        };
        match refusal {
            Some(r) => failed(self.keep(r)),
            None => fed,
        }
    }
}

/// A driver kept to a run's refusals ([`Refusals::guard`]), each new one
/// handed to `told` as it comes (the command-line tool's `refused`
/// records).
pub struct Guarded<'a> {
    pub net: &'a dyn Driver,
    pub refusals: Arc<Refusals>,
    pub progress: Option<Arc<Progress>>,
    pub told: Box<dyn Fn(&Refusal) + 'a>,
}

impl Driver for Guarded<'_> {
    fn perform(&self, effect: Effect) -> Fed {
        let fed = self
            .refusals
            .guard(effect, self.progress.as_deref(), |e| self.net.perform(e));
        for r in self.refusals.news() {
            (self.told)(&r);
        }
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

    #[test]
    fn challenge_pages() {
        let page = br#"<!DOCTYPE html><html lang="en-US"><head><title>Just a moment...</title>
            <script>window._cf_chl_opt={cvId: '3'}</script></head></html>"#;
        assert!(challenge(None, page));
        let turnstile = br#"<html><body><div class="cf-turnstile"></div>
            <script src="https://challenges.cloudflare.com/turnstile/v0/api.js" async></script></body></html>"#;
        assert!(challenge(None, turnstile));
        assert!(challenge(Some("challenge"), b"{}"));
        // an ordinary page that Cloudflare serves (its script in it)
        let ordinary = br#"<html><head><meta name="bmstable" content="header.json"></head><body>
            <script>var u='/cdn-cgi/challenge-platform/scripts/jsd/main.js?ray=a5'</script></body></html>"#;
        assert!(!challenge(None, ordinary));
        assert!(!challenge(Some("managed"), ordinary));
        // an archive or JSON that names it is no page
        assert!(!challenge(None, b"PK\x03\x04 challenges.cloudflare.com"));
        assert!(!challenge(None, br#"{"t":"Just a moment..."}"#));
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
        let progress = Progress::new();
        let g = Guarded {
            net: &net,
            refusals: Arc::default(),
            progress: Some(progress.clone()),
            told: Box::new(|r| told.borrow_mut().push(r.to_string())),
        };
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
                _ => Fed::Downloaded(Ok(Served::bytes(b"PK\x03\x04".to_vec(), None, None))),
            },
        };
        let refusals = Refusals::default();
        let guard = |e| refusals.guard(e, None, |e| net.perform(e));
        let error = |f: Fed| match f {
            Fed::Downloaded(Err(u)) => u.error,
            f => panic!("{f:?}"),
        };
        assert!(matches!(
            error(guard(download("https://busy.test/1.zip"))),
            NetError::Refused(Refusal {
                why: Why::Status(429),
                ..
            })
        ));
        assert!(matches!(
            error(guard(download("https://busy.test/2.zip"))),
            NetError::Refused(_)
        ));
        assert!(matches!(
            error(guard(download("https://checked.test/1.zip"))),
            NetError::Refused(Refusal {
                why: Why::Challenge,
                ..
            })
        ));
        assert!(matches!(
            guard(get("https://checked.test/api")),
            Fed::Http(Err(NetError::Refused(_)))
        ));
        assert!(matches!(
            guard(download("https://fine.test/1.zip")),
            Fed::Downloaded(Ok(_))
        ));
        assert_eq!(
            *net.asked.borrow(),
            [
                "https://busy.test/1.zip",
                "https://checked.test/1.zip",
                "https://fine.test/1.zip"
            ]
        );
        // a refusal from an earlier run: skipped, not told again
        refusals.earlier("other.test");
        assert!(refusals.of("https://other.test/x").is_some());
        assert_eq!(refusals.news().len(), 2);
        assert!(refusals.news().is_empty());
    }
}
