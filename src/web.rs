//! What a web page's drivers share (this crate's page, `bmseeker-web`, and
//! an app's web build): which requests go through the CORS proxy, and what
//! an answer that came is ([`served`]: no larger than it may be, a MEGA
//! file the one its key was made for). No I/O: each keeps its own
//! transport, kept to the run's refusals ([`crate::refusal`]) and to what
//! an answer may have ([`Request::limit`](crate::net::Request::limit)).

use crate::hosts::MegaCipher;
use crate::net::job::{Served, Target};
use crate::net::NetError;

/// How long a request's answer may take to come, whole (ms): a driver's
/// time limit.
pub const ANSWER_MS: u32 = 60_000;

/// How long a download may take, whole (ms), where a driver cannot watch
/// it come (a worker's synchronous request has one time limit and no
/// progress): a file of [`DOWNLOAD_MAX`] at 600 KB a second. A driver
/// that sees the bytes come ends one that stalls
/// ([`STALL_MS`](crate::hosts::STALL_MS)) instead.
pub const DOWNLOAD_MS: u32 = 30 * 60_000;

/// The largest file a page downloads (it is held in memory, beside what
/// it unpacks to): what an archive may unpack to there.
pub const DOWNLOAD_MAX: u64 = crate::unpack::BUDGET;

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

/// `target`'s file that came: `body` (a MEGA file's decrypted here, and
/// the file its key was made for), the name the server gave it, where its
/// redirects ended (the proxy's `X-Final-Url`); none larger than
/// [`DOWNLOAD_MAX`].
pub fn served(
    target: &Target,
    mut body: Vec<u8>,
    name: Option<String>,
    at: Option<String>,
) -> Result<Served, NetError> {
    if body.len() as u64 > DOWNLOAD_MAX {
        return Err(crate::unpack::Error::TooLarge(DOWNLOAD_MAX)
            .to_string()
            .into());
    }
    if let Some(k) = &target.mega_key {
        let mut cipher = MegaCipher::new(k);
        cipher.apply(&mut body);
        if !cipher.verified() {
            return Err(crate::hosts::MEGA_CHANGED.into());
        }
    }
    Ok(Served::bytes(body, name, at))
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

    /// A MEGA file a page downloaded is the one its key was made for, or
    /// no file.
    #[test]
    fn a_mega_files_check() {
        let plain = b"PK\x03\x04 a song's archive".to_vec();
        let (key, sent) = crate::hosts::mega_file([1; 16], [2; 8], &plain);
        let target = Target {
            mega_key: Some(key),
            ..Target::url("https://gfs.example/dl/x")
        };
        let got = served(&target, sent.clone(), None, None).unwrap();
        assert_eq!(got.head, plain);
        let mut changed = sent;
        changed[5] ^= 1;
        assert_eq!(
            served(&target, changed, None, None).map(|s| s.head),
            Err(NetError::Other(crate::hosts::MEGA_CHANGED.into()))
        );
        // any other file is as it came
        let plain = Target::url("https://a.test/a.zip");
        assert_eq!(
            served(&plain, b"PK".to_vec(), None, None).unwrap().head,
            b"PK"
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
