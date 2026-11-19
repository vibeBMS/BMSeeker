//! The last hosts asked: those whose links only a browser gets anything
//! from (OneDrive's pages are built by script; uploaders want a click, a
//! password or an account), and any other server, which is taken to send
//! the file itself.

use super::{Host, Link, Resolved, Target, Want};
use crate::config::Config;
use crate::net::{BoxTask, Ready};

/// Hosts whose links always need a browser.
const PAGES: &[&str] = &[
    "1drv.ms",
    "onedrive",
    "sharepoint.com",
    "baidu",
    "lanzou",
    "lanzn",
    "gigafile",
    "box.com",
];

pub(crate) struct BrowserOnly;

impl Host for BrowserOnly {
    fn name(&self) -> &'static str {
        "browser"
    }

    fn accepts(&self, link: &Link) -> bool {
        PAGES.iter().any(|p| link.host.contains(p))
    }

    fn open(&self, _: &Link, _: Option<&Want>, _: &Config) -> BoxTask<Resolved> {
        Ready::boxed(Resolved::Page)
    }
}

pub(crate) struct Direct;

impl Host for Direct {
    fn name(&self) -> &'static str {
        "direct"
    }

    fn accepts(&self, _: &Link) -> bool {
        true
    }

    fn open(&self, link: &Link, _: Option<&Want>, _: &Config) -> BoxTask<Resolved> {
        Ready::boxed(Resolved::File(Target::url(link.url.clone())))
    }
}

/// For a browser showing a [`Fetch::Page`]: on a OneDrive share (a file,
/// or a folder, which comes as a zip) it presses the page's Download button
/// once, as a person would. OneDrive has no download URL to fetch without a
/// browser any more (its API wants an app's token; its pages are built by
/// script). Run it on every page load (wry: `with_initialization_script`;
/// Android: `evaluateJavascript` when the page has finished).
pub const PRESS_DOWNLOAD_JS: &str = r#"(function () {
  if (window.top !== window || window.__bmseekerPressed) return;
  if (!/(^|\.)(onedrive\.live\.com|1drv\.ms|sharepoint\.com)$/i.test(location.hostname)) return;
  var name = /^\s*(download|ダウンロード|다운로드|下载|下載)\s*$/i;
  var tries = 0;
  var timer = setInterval(function () {
    if (++tries > 120) return clearInterval(timer);
    var all = document.querySelectorAll('button, [role="button"], [role="menuitem"]');
    for (var i = 0; i < all.length; i++) {
      var b = all[i];
      var label = b.getAttribute("aria-label") || b.textContent || "";
      if (name.test(label) && !b.disabled && b.offsetParent !== null) {
        window.__bmseekerPressed = true;
        clearInterval(timer);
        b.click();
        return;
      }
    }
  }, 500);
})();"#;
