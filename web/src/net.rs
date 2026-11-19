//! The network in a web page: a job's requests and downloads as the
//! browser's own (synchronous `XMLHttpRequest`, which a worker may make),
//! through the vibeBMS CORS proxy but to hosts that send CORS headers
//! themselves, as vibeBMS's web build does.
//!
//! A page cannot set a request's `User-Agent` (browsers forbid it), and
//! this does not try: a host that sends CORS headers gets the browser's
//! own, one through the proxy the proxy's. A host's refusal (its status, a
//! browser check) is kept by the commands' run (`bmseeker::refusal`); the
//! proxy's own refusals (this page's origin not allowed: no
//! `X-Final-Url`) are no host's.

/// The proxy the web builds reach download hosts and difficulty tables
/// through (vibeBMS/proxy `worker.js`): it serves the vibebms.github.io
/// pages and localhost.
pub const PROXY: &str = "https://proxy.vibebms.workers.dev/";

/// Whether `url`'s host sends CORS headers (a page reads it itself): BMS
/// SEARCH's API, GitHub's raw files, Konmai's.
pub fn sends_cors(url: &str) -> bool {
    let host = url.split('/').nth(2).unwrap_or("").to_ascii_lowercase();
    matches!(
        host.as_str(),
        "api.bmssearch.net" | "raw.githubusercontent.com" | "bms.alvorna.com"
    )
}

/// `url` as the page asks for it: through `proxy` (a URL ending in `/`),
/// but a host that sends CORS headers itself.
pub fn proxied(proxy: &str, url: &str) -> String {
    if sends_cors(url) {
        url.to_string()
    } else {
        format!("{proxy}?url={}", bmseeker::net::url_encode(url))
    }
}

#[cfg(target_arch = "wasm32")]
pub use xhr::Xhr;

#[cfg(target_arch = "wasm32")]
mod xhr {
    use super::{proxied, sends_cors};
    use bmseeker::hosts::MegaCipher;
    use bmseeker::install::job::{Driver, Effect, Fed, Served, Unserved};
    use bmseeker::net::disposition_name;
    use bmseeker::net::{NetError, Request};
    use bmseeker::refusal::{Refusal, Why};
    use web_sys::{XmlHttpRequest, XmlHttpRequestResponseType};

    /// The job's effects done in a worker, one at a time, blocking it:
    /// requests and downloads with synchronous `XMLHttpRequest`, waits
    /// waited out.
    pub struct Xhr {
        pub proxy: String,
    }

    /// An answer: its status, body and the header asked for.
    struct Got {
        status: u16,
        body: Vec<u8>,
        xhr: XmlHttpRequest,
    }

    fn send(
        method: &str,
        url: &str,
        body: Option<&[u8]>,
        timeout_ms: u32,
    ) -> Result<Got, NetError> {
        let fail = |e: wasm_bindgen::JsValue| {
            NetError::Other(
                js_sys::Reflect::get(&e, &"message".into())
                    .ok()
                    .and_then(|m| m.as_string())
                    .unwrap_or_else(|| "the connection failed".into()),
            )
        };
        let xhr = XmlHttpRequest::new().map_err(fail)?;
        xhr.open_with_async(method, url, false).map_err(fail)?;
        // (a worker's synchronous request may read bytes and time out)
        xhr.set_response_type(XmlHttpRequestResponseType::Arraybuffer);
        xhr.set_timeout(timeout_ms);
        let sent = match body {
            Some(b) => {
                xhr.set_request_header("Content-Type", "application/json")
                    .map_err(fail)?;
                xhr.send_with_opt_u8_array(Some(b))
            }
            None => xhr.send(),
        };
        sent.map_err(fail)?;
        let status = xhr.status().map_err(fail)?;
        if status == 0 {
            return Err(NetError::Other(
                "the connection failed (or the proxy refused it)".into(),
            ));
        }
        let body = xhr
            .response()
            .ok()
            .filter(|r| !r.is_null() && !r.is_undefined())
            .map(|r| js_sys::Uint8Array::new(&r).to_vec())
            .unwrap_or_default();
        Ok(Got { status, body, xhr })
    }

    /// The error of an answer to `url` with no success.
    fn failed(url: &str, proxied: bool, g: &Got) -> NetError {
        let from_host = !proxied
            || g.xhr
                .get_response_header("X-Final-Url")
                .ok()
                .flatten()
                .is_some();
        if !from_host {
            return NetError::Other(format!(
                "the CORS proxy refused (HTTP {}): {}",
                g.status,
                String::from_utf8_lossy(&g.body[..g.body.len().min(200)])
            ));
        }
        if bmseeker::refusal::challenge(None, &g.body) {
            return NetError::Refused(Refusal::of(url, Why::Challenge));
        }
        NetError::Status(g.status)
    }

    impl Driver for Xhr {
        fn perform(&self, effect: Effect) -> Fed {
            match effect {
                Effect::Http(r) => {
                    let url = r.url();
                    let got = match &r {
                        Request::Get(u) => send("GET", &proxied(&self.proxy, u), None, 60_000),
                        Request::Post { url, body } => {
                            send("POST", &proxied(&self.proxy, url), Some(body), 60_000)
                        }
                    };
                    let through = !sends_cors(url);
                    Fed::Http(got.and_then(|g| match g.status {
                        200..=299 => Ok(g.body),
                        _ => Err(failed(url, through, &g)),
                    }))
                }
                Effect::Download { target, .. } => {
                    // no time limit but the browser's: a song may be large
                    let got = send("GET", &proxied(&self.proxy, &target.url), None, 0);
                    Fed::Downloaded(match got {
                        Ok(g) => {
                            let at = g.xhr.get_response_header("X-Final-Url").ok().flatten();
                            if (200..300).contains(&g.status) {
                                let name = g
                                    .xhr
                                    .get_response_header("Content-Disposition")
                                    .ok()
                                    .flatten()
                                    .and_then(|h| disposition_name(&h));
                                let mut b = g.body;
                                if let Some(k) = &target.mega_key {
                                    MegaCipher::new(k).apply(&mut b);
                                }
                                Ok(Served::bytes(b, name, at))
                            } else {
                                let through = !sends_cors(&target.url);
                                Err(Unserved {
                                    error: failed(
                                        at.as_deref().unwrap_or(&target.url),
                                        through,
                                        &g,
                                    ),
                                    at,
                                })
                            }
                        }
                        Err(error) => Err(Unserved { error, at: None }),
                    })
                }
                // a worker has nothing to sleep with but a busy wait (a
                // few seconds before a busy server is asked again)
                Effect::Sleep(d) => {
                    let end = js_sys::Date::now() + d.as_millis() as f64;
                    while js_sys::Date::now() < end {}
                    Fed::Woke
                }
            }
        }
    }
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
}
