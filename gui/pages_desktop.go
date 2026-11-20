//go:build !js

package main

import (
	"os"
	"path/filepath"

	"github.com/egoist/mygo"
	"github.com/egoist/mygo/ui"
)

const pagesHint = "These links give a page, not a file: open one, and its download comes into the library."

// pressDownload presses a share page's Download button (or link) once, as
// a person would: OneDrive's pages build theirs by script, so it is looked
// for again for a minute.
const pressDownload = `(function () {
  if (window.top !== window || window.__bmseekerPressed) return;
  var name = /^\s*(download|ダウンロード|다운로드|下载|下載|普通下载)(\s*\(.*\))?\s*$/i;
  var tries = 0;
  var timer = setInterval(function () {
    if (++tries > 120) return clearInterval(timer);
    var all = document.querySelectorAll('button, [role="button"], [role="menuitem"], a, #downloadButton');
    for (var i = 0; i < all.length; i++) {
      var b = all[i];
      var label = (b.getAttribute("aria-label") || b.textContent || "").trim();
      if ((b.id === "downloadButton" || name.test(label)) && !b.disabled && b.offsetParent !== null) {
        window.__bmseekerPressed = true;
        clearInterval(timer);
        b.click();
        return;
      }
    }
  }, 500);
})();`

// openPage shows p's link in a window of its own; what it downloads goes
// into the library's downloads folder and is imported, and the window
// closes.
func (a *app) openPage(p page) {
	if !a.browsable(p) {
		return
	}
	lib := a.settings.Library
	dir := a.settings.downloads()
	if err := os.MkdirAll(dir, 0o755); err != nil {
		a.log = append(a.log, err.Error())
		return
	}
	win := mygo.NewWindow(mygo.WindowOptions{
		Title:  p.Title,
		URL:    p.Link,
		Width:  1100,
		Height: 800,
		Page:   mygo.PageOptions{PreloadScript: pressDownload},
	})
	pg := win.Page()
	if pg == nil {
		a.log = append(a.log, "this system has no web view to show the page")
		win.Close()
		return
	}
	pg.OnWillDownload(func(e *mygo.DownloadEvent) {
		e.Path = filepath.Join(dir, filepath.Base(e.SuggestedName))
		a.say("%s: downloading %s", p.Title, e.SuggestedName)
	})
	pg.OnDownloadDone(func(d *mygo.Download) {
		if d.Err != nil {
			a.say("%s: %v", p.Title, d.Err)
			return
		}
		mygo.RunOnMain(func() { win.Close() })
		a.job("Importing", func() error {
			got := false
			err := a.cli([]string{"library", "import", "--library=" + lib, "--", d.Path}, func(r Record) {
				got = true
				a.say("%s: %s %s", p.Title, r.S("state"), r.S("path"))
			})
			if got {
				a.update(func() { a.forgetPage(p) })
			}
			return err
		})
	})
}

// importButton: a page's download comes in by itself here.
func (a *app) importButton(*ui.Context) {}
