//! HTTP natively: one agent for requests (a host's connection kept for
//! its next request), one for a driver's downloads ([`downloads`]), each
//! streamed to its file on the calling thread, ended by its progress'
//! cancel, by [`STALL_MS`] of silence or by coming too slowly
//! ([`SLOW`]). Redirects are followed here, one at a time, each kept to
//! what the first address is kept to ([`Rules`]).

use crate::fs::Blob;
use crate::hosts::{Link, NOT_WEB, STALL_MS};
use crate::net::job::{Served, Target, Unserved, HEAD};
use crate::net::{disposition_name, Answer, NetError, Request};
use crate::progress::Progress;
use crate::refusal::Refusals;
use crate::unpack::BUDGET;
use std::io::{Read, Write};
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};
use ureq::unversioned::resolver::{DefaultResolver, ResolvedSocketAddrs, Resolver};
use ureq::unversioned::transport::{
    time, Buffers, ConnectionDetails, Connector, DefaultConnector, NextTimeout, Transport,
};
use ureq::{Agent, Timeout};

/// How long a page or an API answer may take to come.
const ANSWER: Duration = Duration::from_secs(30);

/// How long a server may take to accept a connection: one that is gone
/// (or behind a dead route) fails in this time, not the OS's (75 s and
/// more).
const CONNECT: Duration = Duration::from_secs(15);

/// How long a server may take to begin its answer (a page's or an API's;
/// a download's: [`FIRST_BYTES`]) once asked.
const ANSWER_STARTS: Duration = Duration::from_secs(20);

/// How long a download's server may take to begin its answer.
const FIRST_BYTES: Duration = Duration::from_secs(30);

/// How often a download that waits for data looks at its progress.
const TICK: Duration = Duration::from_millis(200);

const STALL: Duration = Duration::from_millis(STALL_MS);

/// A download's least speed: one that brings fewer than [`SLOW`] bytes in
/// a [`WINDOW`] (4 KiB a second) is given up, so a server that sends a
/// byte now and then (never silent for [`STALL`]) holds a run no longer
/// than the file's size at that speed.
const WINDOW: Duration = Duration::from_secs(60);
const SLOW: u64 = 240 << 10;

/// Redirects followed for one request or download.
const HOPS: usize = 10;

/// What a request keeps to on its way, at its own address and at each
/// its redirects lead to: who asks, whether hosts of this machine and
/// its networks are asked ([`Config::local_hosts`]), and the run's
/// refusals (a redirect to a service that refused is not followed).
///
/// [`Config::local_hosts`]: crate::config::Config::local_hosts
pub(super) struct Rules<'a> {
    pub(super) user_agent: &'a str,
    pub(super) local_hosts: bool,
    pub(super) refused: &'a Refusals,
}

/// What asking a host of this machine or its networks comes to.
const LOCAL: &str = "not asked: an address of this machine or its own network \
    (the setting `local_hosts` allows them)";

/// A name that resolved to an address of this machine or its networks
/// ([`Public`]).
#[derive(Debug)]
struct Local;

impl std::fmt::Display for Local {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.write_str(LOCAL)
    }
}

impl std::error::Error for Local {}

/// Names resolved as ureq resolves them, but to public addresses only: a
/// name that gives one of this machine or its networks
/// ([`local_address`](crate::net::local_address)) is not connected to,
/// whatever it is called and whenever it is looked up (each connection is
/// to what was resolved for it). A proxy the request is set to go through
/// is the user's own, wherever it is (and it resolves the hosts asked
/// through it itself).
#[derive(Debug)]
struct Public(DefaultResolver);

impl Resolver for Public {
    fn resolve(
        &self,
        uri: &ureq::http::Uri,
        config: &ureq::config::Config,
        timeout: NextTimeout,
    ) -> Result<ResolvedSocketAddrs, ureq::Error> {
        let addrs = self.0.resolve(uri, config, timeout)?;
        let proxy = config.proxy().is_some_and(|p| {
            Some(p.host()) == uri.host() && uri.port_u16().is_none_or(|port| port == p.port())
        });
        if !proxy && addrs.iter().any(|a| crate::net::local_address(a.ip())) {
            return Err(ureq::Error::Other(Box::new(Local)));
        }
        Ok(addrs)
    }
}

/// An agent's settings: statuses are answers, redirects are answers too
/// (followed by [`ask`]), the OS's certificates are trusted on desktops
/// (corporate proxies, custom CAs).
fn config() -> ureq::config::ConfigBuilder<ureq::typestate::AgentScope> {
    let tls = ureq::tls::TlsConfig::builder();
    #[cfg(any(windows, target_os = "macos", target_os = "linux"))]
    let tls = tls.root_certs(ureq::tls::RootCerts::PlatformVerifier);
    Agent::config_builder()
        .http_status_as_error(false)
        .max_redirects(0)
        .tls_config(tls.build())
}

/// An agent of `config` through `connector`, connecting to public
/// addresses only unless `local_hosts`.
fn agent(config: ureq::config::Config, connector: impl Connector, local_hosts: bool) -> Agent {
    match local_hosts {
        true => Agent::with_parts(config, connector, DefaultResolver::default()),
        false => Agent::with_parts(config, connector, Public(DefaultResolver::default())),
    }
}

/// The agent of every request (not of downloads: [`downloads`]), by
/// whether it asks local hosts.
fn requests(local_hosts: bool) -> &'static Agent {
    static AGENTS: [std::sync::LazyLock<Agent>; 2] = [
        std::sync::LazyLock::new(|| requests_agent(false)),
        std::sync::LazyLock::new(|| requests_agent(true)),
    ];
    &AGENTS[local_hosts as usize]
}

fn requests_agent(local_hosts: bool) -> Agent {
    let config = config()
        .timeout_connect(Some(CONNECT))
        .timeout_recv_response(Some(ANSWER_STARTS))
        .timeout_recv_body(Some(ANSWER))
        .build();
    agent(config, DefaultConnector::new(), local_hosts)
}

/// A failure of ureq's as it is said.
fn said(e: ureq::Error) -> NetError {
    match e {
        ureq::Error::Other(e) if e.is::<Local>() => NetError::Other(LOCAL.into()),
        e => NetError::Other(e.to_string()),
    }
}

/// Why `url` is not asked under `rules`, if it is not: a host of this
/// machine or its networks, by its name or its address as written
/// ([`local_host`](crate::net::local_host); what a name resolves to:
/// [`Public`]).
fn barred(url: &str, rules: &Rules) -> Option<NetError> {
    let uri: ureq::http::Uri = url.parse().ok()?;
    let local = !rules.local_hosts && crate::net::local_host(uri.host()?);
    local.then(|| NetError::Other(LOCAL.into()))
}

/// Where the redirect `res` (the answer of `from`) leads, if it is one
/// that is followed: to a web address, not from https to http, not to a
/// service that refused in this run.
fn redirect(
    from: &str,
    res: &ureq::http::Response<ureq::Body>,
    rules: &Rules,
) -> Option<Result<String, NetError>> {
    if !matches!(res.status().as_u16(), 301 | 302 | 303 | 307 | 308) {
        return None;
    }
    let to = header_text(res.headers().get("location")?.as_bytes());
    let to = url::Url::parse(from).and_then(|u| u.join(to.trim())).ok();
    let Some(to) = to.map(String::from).filter(|u| Link::parse(u, 0).is_some()) else {
        return Some(Err(format!("a redirect that is {NOT_WEB}").into()));
    };
    let https = |u: &str| u.get(..6).is_some_and(|s| s.eq_ignore_ascii_case("https:"));
    if https(from) && !https(&to) {
        return Some(Err("a redirect from https to http: not followed".into()));
    }
    Some(match rules.refused.of(&to) {
        Some(r) => Err(NetError::Refused(r)),
        None => Ok(to),
    })
}

/// `url`'s answer through `agent` (`post`: the body of a POST, whose
/// redirect is its answer; else a GET), its redirects followed, each
/// address kept to `rules`: the answer and where it came from.
fn ask(
    agent: &Agent,
    url: &str,
    post: Option<&[u8]>,
    rules: &Rules,
    failed: &dyn Fn(ureq::Error) -> NetError,
) -> Result<(ureq::http::Response<ureq::Body>, String), NetError> {
    let mut at = url.to_string();
    for _ in 0..=HOPS {
        if let Some(e) = barred(&at, rules) {
            return Err(e);
        }
        let res = match post {
            Some(body) => agent
                .post(&at)
                .header("User-Agent", rules.user_agent)
                .header("Content-Type", "application/json")
                .send(body),
            None => agent.get(&at).header("User-Agent", rules.user_agent).call(),
        }
        .map_err(failed)?;
        match redirect(&at, &res, rules).filter(|_| post.is_none()) {
            Some(to) => at = to?,
            None => return Ok((res, at)),
        }
    }
    Err(format!("more than {HOPS} redirects").into())
}

/// `r`'s answer, once, kept to `rules`: no more of it than its kind may
/// have ([`Request::limit`]).
pub(super) fn send(r: &Request, rules: &Rules) -> Answer {
    let post = match r {
        Request::Get(..) => None,
        Request::Post { body, .. } => Some(&body[..]),
    };
    let agent = requests(rules.local_hosts);
    let (mut res, at) = ask(agent, r.url(), post, rules, &said)?;
    if !res.status().is_success() || challenged(&res) {
        return Err(refused(r.url(), &at, &mut res));
    }
    let limit = r.limit();
    let body = res.body_mut().with_config().limit(limit as u64);
    body.read_to_vec().map_err(|e| match e {
        ureq::Error::BodyExceedsLimit(_) => crate::net::too_large(limit),
        e => said(e),
    })
}

/// The agent of a driver's downloads (a host's connection kept for its
/// next file), whose connections watch `progress`.
pub(super) fn downloads(progress: Option<Arc<Progress>>, local_hosts: bool) -> Agent {
    let config = config()
        .timeout_connect(Some(CONNECT))
        .timeout_recv_response(Some(FIRST_BYTES))
        .build();
    agent(
        config,
        DefaultConnector::new().chain(Watch(progress)),
        local_hosts,
    )
}

/// A download that came slower than it may ([`Paced`]).
#[derive(Debug)]
struct TooSlow;

impl std::fmt::Display for TooSlow {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.write_str("the download came too slowly (under 4 KiB a second for a minute): given up")
    }
}

impl std::error::Error for TooSlow {}

/// A body read no slower than `least` bytes in each `window`: a read that
/// ends a window that brought fewer fails ([`TooSlow`]).
struct Paced<R> {
    inner: R,
    window: Duration,
    least: u64,
    since: Instant,
    got: u64,
}

impl<R> Paced<R> {
    fn new(inner: R, window: Duration, least: u64) -> Paced<R> {
        Paced {
            inner,
            window,
            least,
            since: Instant::now(),
            got: 0,
        }
    }
}

impl<R: Read> Read for Paced<R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let n = self.inner.read(buf)?;
        self.got += n as u64;
        // (the end is no slow window: what is left may be little)
        if n > 0 && self.since.elapsed() >= self.window {
            if self.got < self.least {
                return Err(std::io::Error::other(TooSlow));
            }
            (self.since, self.got) = (Instant::now(), 0);
        }
        Ok(n)
    }
}

/// The failure of a download larger than an archive may unpack to.
fn too_large() -> NetError {
    crate::unpack::Error::TooLarge(BUDGET).to_string().into()
}

/// `out` made for this download alone, with the mode any file made there
/// has (as an unpacked one: its folder, the run's own, keeps it from
/// others while it comes): what is there under its name (an earlier try's
/// file) is removed, never written through, so a link made ready there
/// leads the bytes nowhere else; what is neither a file nor a link (a
/// device, a pipe) is left as it is.
fn create(out: &Path) -> std::io::Result<std::fs::File> {
    match std::fs::symlink_metadata(out) {
        Ok(m) if m.is_file() || m.file_type().is_symlink() => std::fs::remove_file(out)?,
        Ok(_) => return Err(std::io::Error::other("not a file")),
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(e),
        Err(_) => {}
    }
    std::fs::File::options()
        .write(true)
        .create_new(true)
        .open(out)
}

/// `target` into `out` (a MEGA file decrypted as it comes, and checked
/// when it is whole) through `agent` ([`downloads`]) kept to `rules`,
/// reporting to `progress`: what the server said of it, and its first
/// bytes; or why not, and where its redirects ended. A file that did not
/// come whole is removed.
pub(super) fn download(
    agent: &Agent,
    target: &Target,
    out: &Path,
    rules: &Rules,
    progress: Option<&Progress>,
) -> Result<Served, Unserved> {
    let failed = |e: ureq::Error| match e {
        _ if progress.is_some_and(Progress::cancelled) => NetError::Cancelled,
        ureq::Error::Timeout(Timeout::PerRead) => NetError::Stalled(STALL.as_secs()),
        ureq::Error::Timeout(Timeout::RecvResponse) => NetError::Stalled(FIRST_BYTES.as_secs()),
        e => said(e),
    };
    // (Dropbox sends the file to BMSeeker's own user agent; a browser's
    // gets its page)
    let (mut res, at) = ask(agent, &target.url, None, rules, &failed)?;
    if !res.status().is_success() || challenged(&res) {
        let error = refused(&target.url, &at, &mut res);
        return Err(Unserved {
            error,
            at: Some(at),
        });
    }
    let header = |k: &str| res.headers().get(k).map(|v| header_text(v.as_bytes()));
    // (MEGA names its files itself; no redirects)
    let name = header("content-disposition")
        .and_then(|d| disposition_name(&d))
        .filter(|_| target.mega_key.is_none());
    // (no larger than an archive may unpack to)
    let length = header("content-length").and_then(|l| l.trim().parse().ok());
    if length.is_some_and(|l| l > BUDGET) {
        return Err(too_large().into());
    }
    if let Some(p) = progress {
        p.start_file(length);
    }
    let mega = target
        .mega_key
        .as_deref()
        .map(crate::hosts::MegaCipher::new);
    let body = Paced::new(res.body_mut().as_reader(), WINDOW, SLOW);
    let file = create(out).map_err(|e| NetError::from(format!("{}: {e}", out.display())))?;
    let head = receive(body, mega, file, progress, &failed).inspect_err(|_| {
        let _ = std::fs::remove_file(out);
    })?;
    Ok(Served {
        blob: Blob::File(out.to_path_buf()),
        name,
        at: target.mega_key.is_none().then_some(at),
        head,
    })
}

/// `body` into the new `file` (through `mega`, if it is a MEGA file's:
/// decrypted as it comes, and at its end the file its key was made for),
/// reporting to `progress`: its first bytes.
fn receive(
    mut body: impl Read,
    mut mega: Option<crate::hosts::MegaCipher>,
    file: std::fs::File,
    progress: Option<&Progress>,
    failed: &dyn Fn(ureq::Error) -> NetError,
) -> Result<Vec<u8>, NetError> {
    let mut w = std::io::BufWriter::new(file);
    let mut buf = vec![0u8; 1 << 16];
    let mut head = Vec::with_capacity(HEAD);
    let mut got = 0;
    loop {
        let n = match &mut mega {
            // (whole buffers, so its chunks stay on 16-byte blocks)
            Some(c) => read_full(&mut body, &mut buf).inspect(|&n| c.apply(&mut buf[..n])),
            None => body.read(&mut buf),
        }
        .map_err(|e| match e.get_ref() {
            Some(slow) if slow.is::<TooSlow>() => NetError::Other(slow.to_string()),
            _ => failed(e.into()),
        })?;
        if n == 0 {
            break;
        }
        got += n as u64;
        if got > BUDGET {
            return Err(too_large());
        }
        let more = (HEAD - head.len()).min(n);
        head.extend_from_slice(&buf[..more]);
        w.write_all(&buf[..n]).map_err(|e| e.to_string())?;
        if let Some(p) = progress {
            p.add(n as u64);
        }
    }
    w.flush().map_err(|e| e.to_string())?;
    if mega.is_some_and(|c| !c.verified()) {
        return Err(crate::hosts::MEGA_CHANGED.into());
    }
    Ok(head)
}

/// The error of an answer that is no success (or [`challenged`]) to
/// `url` (where its redirects ended `at`).
fn refused(url: &str, at: &str, res: &mut ureq::http::Response<ureq::Body>) -> NetError {
    let mitigated = res
        .headers()
        .get("cf-mitigated")
        .map(|v| header_text(v.as_bytes()));
    let status = res.status().as_u16();
    // (a page's start is enough to tell)
    let mut head = Vec::new();
    let _ = res
        .body_mut()
        .as_reader()
        .take(64 * 1024)
        .read_to_end(&mut head);
    crate::refusal::answer_error(url, at, status, mitigated.as_deref(), &head)
}

/// Whether Cloudflare answered with a browser check (`cf-mitigated:
/// challenge`), whatever the status.
fn challenged(res: &ureq::http::Response<ureq::Body>) -> bool {
    res.headers()
        .get("cf-mitigated")
        .is_some_and(|v| v.as_bytes().eq_ignore_ascii_case(b"challenge"))
}

/// Connections that, while they wait for data, give up when the progress is
/// cancelled or no data came for [`STALL`].
#[derive(Debug)]
struct Watch(Option<Arc<Progress>>);

impl Connector<Box<dyn Transport>> for Watch {
    type Out = Box<dyn Transport>;
    fn connect(
        &self,
        _: &ConnectionDetails,
        chained: Option<Box<dyn Transport>>,
    ) -> Result<Option<Self::Out>, ureq::Error> {
        Ok(chained.map(|inner| {
            Box::new(Watched {
                inner,
                progress: self.0.clone(),
            }) as Box<dyn Transport>
        }))
    }
}

#[derive(Debug)]
struct Watched {
    inner: Box<dyn Transport>,
    progress: Option<Arc<Progress>>,
}

impl Transport for Watched {
    fn buffers(&mut self) -> &mut dyn Buffers {
        self.inner.buffers()
    }

    fn transmit_output(&mut self, amount: usize, timeout: NextTimeout) -> Result<(), ureq::Error> {
        self.inner.transmit_output(amount, timeout)
    }

    /// Waits of [`TICK`] at most, the progress looked at between them.
    fn await_input(&mut self, timeout: NextTimeout) -> Result<bool, ureq::Error> {
        let since = Instant::now();
        loop {
            if self.progress.as_ref().is_some_and(|p| p.cancelled()) {
                return Err(ureq::Error::Io(std::io::ErrorKind::Interrupted.into()));
            }
            let stall = STALL.saturating_sub(since.elapsed());
            if stall.is_zero() {
                return Err(ureq::Error::Timeout(Timeout::PerRead));
            }
            let wait = TICK.min(stall);
            // (a shorter deadline of the request's own stands)
            if timeout.after <= time::Duration::from(wait) {
                return self.inner.await_input(timeout);
            }
            let tick = NextTimeout {
                after: wait.into(),
                reason: Timeout::PerRead,
                per_read: None,
                ..timeout
            };
            match self.inner.await_input(tick) {
                Err(ureq::Error::Timeout(_)) => {}
                Err(ureq::Error::Io(e))
                    if matches!(
                        e.kind(),
                        std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock
                    ) => {}
                r => return r,
            }
        }
    }

    fn is_open(&mut self) -> bool {
        self.inner.is_open()
    }

    fn is_tls(&self) -> bool {
        self.inner.is_tls()
    }
}

/// A header's value as text: UTF-8, else each byte a Latin-1 character.
fn header_text(v: &[u8]) -> String {
    match std::str::from_utf8(v) {
        Ok(t) => t.to_string(),
        Err(_) => v.iter().map(|&b| b as char).collect(),
    }
}

/// Fill `buf` (short only at the end).
fn read_full(r: &mut impl Read, buf: &mut [u8]) -> std::io::Result<usize> {
    let mut n = 0;
    while n < buf.len() {
        match r.read(&mut buf[n..])? {
            0 => break,
            k => n += k,
        }
    }
    Ok(n)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rules(refused: &Refusals) -> Rules<'_> {
        Rules {
            user_agent: "",
            local_hosts: false,
            refused,
        }
    }

    /// The answer `status` with a `Location`.
    fn moved(status: u16, to: &str) -> ureq::http::Response<ureq::Body> {
        ureq::http::Response::builder()
            .status(status)
            .header("Location", to)
            .body(ureq::Body::builder().data(""))
            .unwrap()
    }

    /// Where a redirect may lead: on the web, never from https to http,
    /// never to a service that refused.
    #[test]
    fn redirects() {
        let refused = Refusals::default();
        refused.earlier("refused.test");
        let r = rules(&refused);
        let to = |from: &str, status: u16, to: &str| redirect(from, &moved(status, to), &r);
        let from = "https://a.test/x/y?z";
        assert_eq!(
            to(from, 302, "https://b.test/f.zip"),
            Some(Ok("https://b.test/f.zip".into()))
        );
        assert_eq!(
            to(from, 301, "../f.zip"),
            Some(Ok("https://a.test/f.zip".into()))
        );
        assert_eq!(
            to(from, 308, "//b.test/f"),
            Some(Ok("https://b.test/f".into()))
        );
        // http may lead to https, and on to http
        let http = "http://a.test/x";
        assert_eq!(
            to(http, 307, "https://a.test/x"),
            Some(Ok("https://a.test/x".into()))
        );
        assert_eq!(
            to(http, 303, "http://b.test/"),
            Some(Ok("http://b.test/".into()))
        );
        for down in ["http://a.test/x/y", "http://b.test/f.zip", "HTTP://b.test/"] {
            for from in [from, "HTTPS://A.test/x"] {
                let e = to(from, 302, down).unwrap().unwrap_err();
                assert!(e.to_string().contains("https to http"), "{down}: {e}");
            }
        }
        for odd in [
            "file:///etc/passwd",
            "ftp://b.test/f",
            "https://u@b.test/f",
            "javascript:1",
        ] {
            let e = to(from, 302, odd).unwrap().unwrap_err();
            assert!(e.to_string().contains("not a web link"), "{odd}: {e}");
        }
        assert!(matches!(
            to(from, 302, "https://www.refused.test/f.zip"),
            Some(Err(NetError::Refused(r))) if r.host == "refused.test"
        ));
        // what is no redirect is the answer
        assert_eq!(to(from, 200, "https://b.test/"), None);
        assert_eq!(to(from, 304, "https://b.test/"), None);
        let bare = ureq::http::Response::builder().status(302);
        let bare = bare.body(ureq::Body::builder().data("")).unwrap();
        assert_eq!(redirect(from, &bare, &r), None);
    }

    /// An address of this machine or its networks is not asked as written
    /// (where a redirect leads no more than at first), and a name that
    /// resolves to one is not connected to.
    #[test]
    fn local_addresses() {
        let refused = Refusals::default();
        let r = rules(&refused);
        for u in [
            "http://127.0.0.1:8080/a",
            "http://localhost/a",
            "https://192.168.1.1/",
            "http://169.254.169.254/latest/meta-data/",
            "http://[::1]:9/",
            "http://[fd00::1]/",
            "http://0x7f.1/",
        ] {
            assert_eq!(barred(u, &r), Some(NetError::Other(LOCAL.into())), "{u}");
        }
        assert_eq!(barred("https://example.com/a", &r), None);
        let allowed = Rules {
            local_hosts: true,
            ..rules(&refused)
        };
        assert_eq!(barred("http://127.0.0.1:8080/a", &allowed), None);
        // a name of the hosts file for this machine
        let config = Agent::config_builder().build();
        let resolve = |host: &str| {
            let uri = format!("http://{host}:80/").parse().unwrap();
            Public(DefaultResolver::default()).resolve(&uri, &config, NextTimeout::default())
        };
        assert!(matches!(resolve("localhost"), Err(ureq::Error::Other(e)) if e.is::<Local>()));
        assert!(matches!(resolve("127.0.0.1"), Err(ureq::Error::Other(_))));
    }

    /// Bytes that come `step` apart, one at a time.
    struct Drip(std::time::Duration, usize);

    impl Read for Drip {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            if self.1 == 0 {
                return Ok(0);
            }
            std::thread::sleep(self.0);
            self.1 -= 1;
            buf[0] = b'x';
            Ok(1)
        }
    }

    /// A body that comes a byte now and then (never silent for long) ends
    /// with its first window; one that comes fast enough is read whole.
    #[test]
    fn too_slow() {
        let ms = Duration::from_millis;
        let mut slow = Paced::new(Drip(ms(10), 1000), ms(50), 100);
        let t = Instant::now();
        let e = std::io::copy(&mut slow, &mut std::io::sink()).unwrap_err();
        assert!(e.get_ref().is_some_and(|e| e.is::<TooSlow>()), "{e}");
        assert!(t.elapsed() < ms(2000));
        let mut fast = Paced::new(Drip(ms(1), 300), ms(50), 5);
        assert_eq!(std::io::copy(&mut fast, &mut std::io::sink()).unwrap(), 300);
        // (a short last window is the end, not slowness)
        let mut short = Paced::new(Drip(ms(60), 1), ms(50), 100);
        assert!(std::io::copy(&mut short, &mut std::io::sink()).is_err());
        let mut end = Paced::new(Drip(ms(1), 3), ms(50), 100);
        assert_eq!(std::io::copy(&mut end, &mut std::io::sink()).unwrap(), 3);
    }
}
