//! What a web page's drivers share (this crate's page, `bmseeker-web`, and
//! an app's web build): which requests go through the CORS proxy, and what
//! an answer that came is. No I/O: each keeps its own transport, kept to
//! the run's refusals ([`crate::refusal`]).

use crate::hosts::{MegaCipher, Target};
use crate::install::job::Served;
use crate::net::NetError;

/// The proxy the web builds reach download hosts and difficulty tables
/// through (vibeBMS/proxy `worker.js`): it serves the vibebms.github.io
/// pages and localhost.
pub const PROXY: &str = "https://proxy.vibebms.workers.dev/";

/// Whether `url`'s host sends CORS headers (a page reads it itself): BMS
/// SEARCH's API, GitHub's raw files (an event mirror's lists), Konmai's.
pub fn sends_cors(url: &str) -> bool {
    let host = url.split('/').nth(2).unwrap_or("").to_ascii_lowercase();
    matches!(
        host.as_str(),
        "api.bmssearch.net" | "raw.githubusercontent.com" | "bms.alvorna.com"
    )
}

/// `url` as a page asks for it: through `proxy` (a URL ending in `/`), but
/// a host that sends CORS headers itself.
pub fn proxied(proxy: &str, url: &str) -> String {
    if sends_cors(url) {
        url.to_string()
    } else {
        format!("{proxy}?url={}", crate::net::url_encode(url))
    }
}

/// `target`'s file that came: `body` (a MEGA file's decrypted here), the
/// name the server gave it, where its redirects ended (the proxy's
/// `X-Final-Url`).
pub fn served(
    target: &Target,
    mut body: Vec<u8>,
    name: Option<String>,
    at: Option<String>,
) -> Served {
    if let Some(k) = &target.mega_key {
        MegaCipher::new(k).apply(&mut body);
    }
    Served::bytes(body, name, at)
}

/// The error of an answer of `status` to `url` that is no success, its
/// body `body`: one the proxy gave itself (`through` it, with no
/// `X-Final-Url`: this page's origin not allowed) is no host's; else as
/// natively, where the redirects ended at `final_url` (a browser check
/// refuses the host, Google Drive's quota is said as such).
pub fn unserved(
    url: &str,
    status: u16,
    through: bool,
    final_url: Option<&str>,
    body: &[u8],
) -> NetError {
    if through && final_url.is_none() {
        return NetError::Other(format!(
            "the CORS proxy refused (HTTP {status}): {}",
            String::from_utf8_lossy(&body[..body.len().min(200)])
        ));
    }
    crate::refusal::answer_error(url, final_url.unwrap_or(url), status, None, body)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn proxied_but_hosts_with_cors() {
        assert_eq!(
            proxied(PROXY, "https://drive.google.com/uc?id=a&x=1"),
            "https://proxy.vibebms.workers.dev/?url=https%3A%2F%2Fdrive.google.com%2Fuc%3Fid%3Da%26x%3D1"
        );
        let api = "https://api.bmssearch.net/v1/bms/search?q=x";
        assert_eq!(proxied(PROXY, api), api);
        assert_eq!(
            proxied("http://localhost:8788/", "http://127.0.0.1:9/s.zip"),
            "http://localhost:8788/?url=http%3A%2F%2F127.0.0.1%3A9%2Fs.zip"
        );
    }

    #[test]
    fn errors() {
        let drive = "https://drive.usercontent.google.com/download?id=Q";
        // the proxy's own
        let e = unserved(drive, 403, true, None, b"origin not allowed");
        assert_eq!(
            e,
            NetError::Other("the CORS proxy refused (HTTP 403): origin not allowed".into())
        );
        // the host's: Drive's quota said as natively
        let quota = b"<html><head><title>Google Drive - Quota exceeded</title></head>\
            <body>Too many users have viewed or downloaded this file recently.</body></html>";
        assert_eq!(
            unserved(drive, 403, true, Some(drive), quota),
            NetError::Other(crate::hosts::DRIVE_QUOTA.into())
        );
        assert_eq!(
            unserved(drive, 404, false, None, b""),
            NetError::Status(404)
        );
        let check = b"<html><head><title>Just a moment...</title></head></html>";
        assert!(matches!(
            unserved("https://a.test/x", 403, true, Some("https://b.test/y"), check),
            NetError::Refused(r) if r.host == "b.test"
        ));
    }
}
