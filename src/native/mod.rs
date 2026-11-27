//! Natively (desktop, iOS, Android): the transport of jobs ([`Native`]:
//! requests through one HTTP agent, a link's file streamed to disk on the
//! calling thread, waits slept), and the blocking calls built on it.

pub(crate) mod http;

use crate::hosts::{Context, Fetch, Want};
use crate::install::job::{Acquire, FileLink, Files, Got, Missed};
use crate::net::job::{run_job, Driver, Effect, Fed};
use crate::net::NetError;
use crate::progress::Progress;
use crate::refusal::Guarded;
use std::path::Path;
use std::sync::{Arc, OnceLock};
use std::time::Duration;

/// The native transport, as a job's [`Driver`]: requests through one
/// agent (a host's connection kept), files streamed to their slots through
/// another (ended by the progress' cancel,
/// [`STALL_MS`](crate::hosts::STALL_MS) of silence or coming too slowly; a
/// MEGA file decrypted as it comes and checked when it is whole), waits
/// slept (ended by the cancel). Once the progress is cancelled, requests
/// are answered [`NetError::Cancelled`]. It asks no host of this machine
/// or its networks unless the settings say so
/// ([`Config::local_hosts`](crate::config::Config::local_hosts)), and
/// follows no redirect from https to http. It keeps to no refusals itself
/// for the address asked: run it under [`Guarded`]; where a redirect
/// leads is asked here, so there it keeps to the run's.
pub struct Native {
    user_agent: String,
    local_hosts: bool,
    refused: Arc<crate::refusal::Refusals>,
    progress: Option<Arc<Progress>>,
    /// The downloads' agent, made for the first.
    downloads: OnceLock<ureq::Agent>,
}

impl Native {
    /// With `cx`'s settings and progress.
    pub fn new(cx: &Context) -> Native {
        Native {
            user_agent: crate::config::native_user_agent(&cx.config.user_agent),
            local_hosts: cx.config.local_hosts,
            refused: cx.refused.clone(),
            progress: cx.progress.clone(),
            downloads: OnceLock::new(),
        }
    }

    fn cancelled(&self) -> bool {
        self.progress.as_ref().is_some_and(|p| p.cancelled())
    }
}

impl Driver for Native {
    fn perform(&self, effect: Effect) -> Fed {
        let rules = http::Rules {
            user_agent: &self.user_agent,
            local_hosts: self.local_hosts,
            refused: &self.refused,
        };
        match effect {
            Effect::Http(_) if self.cancelled() => Fed::Http(Err(NetError::Cancelled)),
            Effect::Http(r) => Fed::Http(http::send(&r, &rules)),
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
                    .get_or_init(|| http::downloads(self.progress.clone(), self.local_hosts));
                let p = self.progress.as_deref();
                Fed::Downloaded(http::download(agent, &target, &slot, &rules, p))
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
