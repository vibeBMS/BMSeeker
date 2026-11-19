//! Natively (desktop, iOS, Android): the transport of jobs ([`Native`]:
//! requests through one HTTP agent, a link's file streamed to disk on the
//! calling thread, waits slept), and the blocking calls built on it.

pub(crate) mod http;

use crate::hosts::{Context, Fetch, Want};
use crate::install::job::{run_job, Acquire, Driver, Effect, Fed, FileLink, Files, Got, Missed};
use crate::net::NetError;
use crate::progress::Progress;
use crate::refusal::Guarded;
use std::path::Path;
use std::sync::{Arc, OnceLock};
use std::time::Duration;

/// The native transport, as a job's [`Driver`]: requests through one
/// agent (a host's connection kept), files streamed to their slots through
/// another (ended by the progress' cancel or
/// [`STALL_MS`](crate::hosts::STALL_MS) of silence; a MEGA file decrypted
/// as it comes), waits slept (ended by the cancel). Once the progress is
/// cancelled, requests are answered [`NetError::Cancelled`]. It keeps to no
/// refusals itself: run it under [`Guarded`].
pub struct Native {
    user_agent: String,
    progress: Option<Arc<Progress>>,
    /// The downloads' agent, made for the first.
    downloads: OnceLock<ureq::Agent>,
}

impl Native {
    /// With `cx`'s settings and progress.
    pub fn new(cx: &Context) -> Native {
        Native {
            user_agent: crate::config::native_user_agent(&cx.config.user_agent),
            progress: cx.progress(),
            downloads: OnceLock::new(),
        }
    }

    fn cancelled(&self) -> bool {
        self.progress.as_ref().is_some_and(|p| p.cancelled())
    }
}

impl Driver for Native {
    fn perform(&self, effect: Effect) -> Fed {
        match effect {
            Effect::Http(_) if self.cancelled() => Fed::Http(Err(NetError::Cancelled)),
            Effect::Http(r) => Fed::Http(http::send(&r, &self.user_agent)),
            Effect::Sleep(d) => {
                let end = std::time::Instant::now() + d;
                while !self.cancelled() {
                    let left = end.saturating_duration_since(std::time::Instant::now());
                    if left.is_zero() {
                        break;
                    }
                    std::thread::sleep(left.min(Duration::from_millis(100)));
                }
                Fed::Woke
            }
            Effect::Download { target, slot, .. } => {
                let agent = self
                    .downloads
                    .get_or_init(|| http::downloads(self.progress.clone()));
                let p = self.progress.as_deref();
                Fed::Downloaded(
                    http::download(agent, &target, &slot, &self.user_agent, p).map_err(Into::into),
                )
            }
        }
    }
}

/// Download the archive `link` leads to into `out` through `net` (a
/// [`Guarded`] transport): its file name where it came from (the folder's
/// listing, MEGA, the mirror, the server, the link). With `any`, a file
/// that is not an archive is kept too if it is not a page: a chart or a
/// movie posted on its own (an event's add-on).
pub fn fetch(
    link: &str,
    want: Option<Want>,
    out: &Path,
    any: bool,
    cx: &Context,
    net: &dyn Driver,
) -> Result<String, Fetch> {
    let mut job = Acquire::new(link, want, any, out.to_path_buf(), cx);
    match run_job(&mut job, net) {
        Ok(got) => Ok(got.name),
        Err(e) => {
            // (a page, or what did not come)
            let _ = std::fs::remove_file(out);
            Err(e)
        }
    }
}

/// The files of `links`, one after another, into files named after
/// `slots` ([`Files::slot`]), kept to `cx`'s refusals: all of them, or
/// which did not come and why (the files that came are left for the caller
/// to remove).
pub fn fetch_files(links: Vec<FileLink>, slots: &Path, cx: &Context) -> Result<Vec<Got>, Missed> {
    let native = Native::new(cx);
    run_job(
        &mut Files::new(links, slots.to_path_buf(), cx),
        &Guarded::new(&native, cx),
    )
}

#[cfg(test)]
mod tests;
