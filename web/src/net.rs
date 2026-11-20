//! The network in a web page: a job's requests and downloads as the
//! browser's own (synchronous `XMLHttpRequest`, which a worker may make),
//! through the vibeBMS CORS proxy but to hosts that send CORS headers
//! themselves, as vibeBMS's web build does (`bmseeker::web`).
//!
//! A page cannot set a request's `User-Agent` (browsers forbid it), and
//! this does not try: a host that sends CORS headers gets the browser's
//! own, one through the proxy the proxy's. A host's refusal (its status, a
//! browser check) is kept by the commands' run (`bmseeker::refusal`); the
//! proxy's own refusals (this page's origin not allowed: no
//! `X-Final-Url`) are no host's.
//!
//! A synchronous request has one time limit and gives its body whole: an
//! answer has `ANSWER_MS`, a download `DOWNLOAD_MS`, and what came larger
//! than it may be (an answer's kind, `DOWNLOAD_MAX`) is dropped before it
//! is copied into the page's memory.

pub use bmseeker::web::{proxied, sends_cors, PROXY};

#[cfg(target_arch = "wasm32")]
pub use xhr::Xhr;

#[cfg(target_arch = "wasm32")]
mod xhr {
    use super::{proxied, sends_cors};
    use bmseeker::install::job::{Driver, Effect, Fed, Unserved};
    use bmseeker::net::disposition_name;
    use bmseeker::net::{NetError, Request};
    use bmseeker::web::{served, unserved, ANSWER_MS, DOWNLOAD_MAX, DOWNLOAD_MS};
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

    impl Got {
        fn header(&self, name: &str) -> Option<String> {
            self.xhr.get_response_header(name).ok().flatten()
        }
    }

    /// `url`'s answer within `timeout_ms`, its body if it has `limit`
    /// bytes at most (a larger one is left where the browser has it).
    fn send(
        method: &str,
        url: &str,
        body: Option<&[u8]>,
        timeout_ms: u32,
        limit: usize,
    ) -> Result<Got, NetError> {
        let fail = |e: wasm_bindgen::JsValue| {
            let said = |key: &str| {
                let v = js_sys::Reflect::get(&e, &key.into()).ok();
                v.and_then(|m| m.as_string())
            };
            NetError::Other(match said("name").as_deref() {
                Some("TimeoutError") => {
                    format!("took longer than {} s: given up", timeout_ms / 1000)
                }
                _ => said("message").unwrap_or_else(|| "the connection failed".into()),
            })
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
        let bytes = xhr
            .response()
            .ok()
            .filter(|r| !r.is_null() && !r.is_undefined())
            .map(|r| js_sys::Uint8Array::new(&r));
        let body = match bytes {
            Some(b) if b.length() as usize > limit => {
                return Err(bmseeker::net::too_large(limit));
            }
            Some(b) => b.to_vec(),
            None => Vec::new(),
        };
        Ok(Got { status, body, xhr })
    }

    impl Driver for Xhr {
        fn perform(&self, effect: Effect) -> Fed {
            match effect {
                Effect::Http(r) => {
                    let url = r.url();
                    let asked = proxied(&self.proxy, url);
                    let limit = r.limit();
                    let got = match &r {
                        Request::Get(..) => send("GET", &asked, None, ANSWER_MS, limit),
                        Request::Post { body, .. } => {
                            send("POST", &asked, Some(body), ANSWER_MS, limit)
                        }
                    };
                    let through = !sends_cors(url);
                    Fed::Http(got.and_then(|g| match g.status {
                        200..=299 => Ok(g.body),
                        n => Err(unserved(
                            url,
                            n,
                            through,
                            g.header("X-Final-Url").as_deref(),
                            &g.body,
                        )),
                    }))
                }
                Effect::Download { target, .. } => {
                    let asked = proxied(&self.proxy, &target.url);
                    let got = send("GET", &asked, None, DOWNLOAD_MS, DOWNLOAD_MAX as usize);
                    Fed::Downloaded(match got {
                        Ok(g) => {
                            let at = g.header("X-Final-Url");
                            if (200..300).contains(&g.status) {
                                let name = g
                                    .header("Content-Disposition")
                                    .and_then(|h| disposition_name(&h));
                                served(&target, g.body, name, at.clone())
                                    .map_err(|error| Unserved { error, at })
                            } else {
                                let through = !sends_cors(&target.url);
                                let error = unserved(
                                    &target.url,
                                    g.status,
                                    through,
                                    at.as_deref(),
                                    &g.body,
                                );
                                Err(Unserved { error, at })
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
