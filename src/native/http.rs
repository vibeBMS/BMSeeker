//! HTTP natively: one agent for requests (a host's connection kept for
//! its next request), and a download streamed to its file on the calling
//! thread, ended by its progress' cancel or by [`STALL_MS`] of silence.

use crate::hosts::{Target, STALL_MS};
use crate::net::{disposition_name, Answer, NetError, Request};
use crate::progress::Progress;
use std::io::{Read, Write};
use std::path::Path;
use std::time::{Duration, Instant};
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

/// An agent's settings: statuses are answers, the OS's certificates are
/// trusted on desktops (corporate proxies, custom CAs).
fn config() -> ureq::config::ConfigBuilder<ureq::typestate::AgentScope> {
    let tls = ureq::tls::TlsConfig::builder();
    #[cfg(any(windows, target_os = "macos", target_os = "linux"))]
    let tls = tls.root_certs(ureq::tls::RootCerts::PlatformVerifier);
    Agent::config_builder()
        .http_status_as_error(false)
        .tls_config(tls.build())
}

/// The agent of every request (not of downloads: [`download`]).
static AGENT: std::sync::LazyLock<Agent> = std::sync::LazyLock::new(|| {
    config()
        .timeout_connect(Some(CONNECT))
        .timeout_recv_response(Some(ANSWER_STARTS))
        .timeout_recv_body(Some(ANSWER))
        .build()
        .into()
});

/// `r`'s answer, once, sent with `user_agent`.
pub(super) fn send(r: &Request, user_agent: &str) -> Answer {
    let mut res = match r {
        Request::Get(u) => AGENT.get(u).header("User-Agent", user_agent).call(),
        Request::Post { url, body } => AGENT
            .post(url)
            .header("User-Agent", user_agent)
            .header("Content-Type", "application/json")
            .send(&body[..]),
    }
    .map_err(|e| e.to_string())?;
    if !res.status().is_success() {
        return Err(NetError::Status(res.status().as_u16()));
    }
    Ok(res
        .body_mut()
        .with_config()
        .read_to_vec()
        .map_err(|e| e.to_string())?)
}

/// `target` into `out` (a MEGA file decrypted as it comes) with
/// `user_agent`, reporting to `progress`: what the server said of it. Each
/// download has an agent of its own, whose connections watch the progress.
pub(super) fn download(
    target: &Target,
    out: &Path,
    user_agent: &str,
    progress: Option<&std::sync::Arc<Progress>>,
) -> Result<Served, NetError> {
    let p = progress.map(|p| &**p);
    let failed = |e: ureq::Error| match e {
        _ if p.is_some_and(Progress::cancelled) => NetError::Cancelled,
        ureq::Error::Timeout(Timeout::PerRead) => NetError::Stalled(STALL.as_secs()),
        ureq::Error::Timeout(Timeout::RecvResponse) => NetError::Stalled(FIRST_BYTES.as_secs()),
        e => NetError::Other(e.to_string()),
    };
    let agent = Agent::with_parts(
        config()
            .timeout_connect(Some(CONNECT))
            .timeout_recv_response(Some(FIRST_BYTES))
            .build(),
        DefaultConnector::new().chain(Watch(progress.cloned())),
        ureq::unversioned::resolver::DefaultResolver::default(),
    );
    let mut req = agent.get(&target.url);
    // Dropbox answers a browser with its page instead of the file
    let link = crate::hosts::Link::parse(&target.url, 0);
    if !link.is_some_and(|l| l.on("dropbox.com") || l.on("dropboxusercontent.com")) {
        req = req.header("User-Agent", user_agent);
    }
    let mut res = req.call().map_err(failed)?;
    if !res.status().is_success() {
        return Err(NetError::Status(res.status().as_u16()));
    }
    let header = |k: &str| res.headers().get(k).map(|v| header_text(v.as_bytes()));
    let served = Served {
        // (MEGA names its files itself; no redirects)
        name: header("content-disposition")
            .and_then(|d| disposition_name(&d))
            .filter(|_| target.mega_key.is_none()),
        at: target
            .mega_key
            .is_none()
            .then(|| ureq::ResponseExt::get_uri(&res).to_string()),
    };
    if let Some(p) = p {
        p.start_file(header("content-length").and_then(|l| l.trim().parse().ok()));
    }
    let mut mega = target
        .mega_key
        .as_deref()
        .map(crate::hosts::MegaCipher::new);
    let file = std::fs::File::create(out).map_err(|e| format!("{}: {e}", out.display()))?;
    let mut w = std::io::BufWriter::new(file);
    let mut body = res.body_mut().as_reader();
    let mut buf = vec![0u8; 1 << 16];
    loop {
        let n = match &mut mega {
            // (whole buffers, so its chunks stay on 16-byte blocks)
            Some(c) => read_full(&mut body, &mut buf).inspect(|&n| c.apply(&mut buf[..n])),
            None => body.read(&mut buf),
        }
        .map_err(|e| failed(e.into()))?;
        if n == 0 {
            break;
        }
        w.write_all(&buf[..n]).map_err(|e| e.to_string())?;
        if let Some(p) = p {
            p.add(n as u64);
        }
    }
    w.flush().map_err(|e| e.to_string())?;
    Ok(served)
}

/// What a download's server said of it.
pub(super) struct Served {
    /// The file name it gave (`Content-Disposition`).
    pub(super) name: Option<String>,
    /// Where its redirects ended.
    pub(super) at: Option<String>,
}

/// Connections that, while they wait for data, give up when the progress is
/// cancelled or no data came for [`STALL`].
#[derive(Debug)]
struct Watch(Option<std::sync::Arc<Progress>>);

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
    progress: Option<std::sync::Arc<Progress>>,
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
pub(super) fn read_full(r: &mut impl Read, buf: &mut [u8]) -> std::io::Result<usize> {
    let mut n = 0;
    while n < buf.len() {
        match r.read(&mut buf[n..])? {
            0 => break,
            k => n += k,
        }
    }
    Ok(n)
}
