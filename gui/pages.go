package main

import (
	"os"
	"path/filepath"

	"github.com/egoist/mygo"
	"github.com/egoist/mygo/ui"
)

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

func (a *app) pagesView(c *ui.Context) {
	ui.Text(c, "These links give a page, not a file: open one, and its download comes into the library.")
	ui.Row(c).Gap(8).Children(func() {
		ok := a.prow < len(a.pages) && a.settings.Library != ""
		if ui.PrimaryButton(c, "Open").Disabled(!ok).Clicked() {
			a.openPage(a.pages[a.prow])
		}
		if ui.Button(c, "Forget").Disabled(!ok).Clicked() {
			a.pages = append(a.pages[:a.prow], a.pages[a.prow+1:]...)
		}
	})
	cols := []ui.TableColumn{{Title: "Song", Width: 220}, {Title: "Link"}}
	if ui.Table(c, &a.prows, cols, len(a.pages), func(row, col int) {
		switch col {
		case 0:
			ui.Text(c, a.pages[row].Title).SingleLine()
		case 1:
			ui.Text(c, a.pages[row].Link).SingleLine()
		}
	}).Grow(1).Submitted() && a.prow < len(a.pages) {
		a.openPage(a.pages[a.prow])
	}
}

// openPage shows p's link in a window of its own; what it downloads goes
// into the library's downloads folder and is imported, and the window
// closes.
func (a *app) openPage(p page) {
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
			err := a.cli([]string{"library", "import", d.Path, "-L", lib}, func(r Record) {
				got = true
				a.say("%s: %s %s", p.Title, r.S("state"), r.S("path"))
			})
			if got {
				a.update(func() {
					for i, x := range a.pages {
						if x == p {
							a.pages = append(a.pages[:i], a.pages[i+1:]...)
							break
						}
					}
				})
			}
			return err
		})
	})
}
