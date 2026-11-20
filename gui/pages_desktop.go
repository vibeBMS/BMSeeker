//go:build !js

package main

import (
	"os"

	"github.com/egoist/mygo"
	"github.com/egoist/mygo/ui"
)

const pagesHint = "These links give a page, not a file: open one, and its download comes into the library."

// pageScript is what a page's window runs on every page it shows: the
// CLI's (`link page-script`), which presses a OneDrive share's Download
// button as a person would and does nothing on any other host. The window
// has no script of its own: where a button is pressed for the user is the
// library's to say. Asked once; "" when the CLI has none to give (the
// button is then the user's to press).
func (a *app) pageScript() string {
	a.scriptOnce.Do(func() {
		err := runCLI([]string{"link", "page-script"}, func(r Record) { a.script = r.S("script") }, nil)
		if err != nil {
			a.log = append(a.log, "a page's Download button is not pressed for you: "+err.Error())
		}
	})
	return a.script
}

// savePath is where a page's download is kept until it is imported: a new
// file in dir (the library's downloads folder), under a name of the
// window's making. The page's name for it is no file's name here: it may
// be that of a file the folder has (the library's scan, a chart that
// waits for its song), which it would replace.
func savePath(dir string) (string, error) {
	f, err := os.CreateTemp(dir, "page-*.part")
	if err != nil {
		return "", err
	}
	return f.Name(), f.Close()
}

// importArgs is the CLI's command for the download at path, which its
// page named name: the name says what it is (an archive, a chart) and
// names a song's folder, as the CLI makes a name of it.
func importArgs(lib, name, path string) []string {
	return []string{"library", "import", "--library=" + lib, "--name=" + name, "--", path}
}

// openPage shows p's link in a window of its own; its download (the first:
// a page is opened for one song) goes into the library's downloads folder
// and is imported, and the window closes.
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
		Page:   mygo.PageOptions{PreloadScript: a.pageScript()},
	})
	pg := win.Page()
	if pg == nil {
		a.log = append(a.log, "this system has no web view to show the page")
		win.Close()
		return
	}
	// the download's name as its page gives it ("": none has begun)
	var name string
	pg.OnWillDownload(func(e *mygo.DownloadEvent) {
		if name != "" {
			e.PreventDefault()
			return
		}
		path, err := savePath(dir)
		if err != nil {
			e.PreventDefault()
			a.say("%s: %v", p.Title, err)
			return
		}
		name, e.Path = e.SuggestedName, path
		a.say("%s: downloading %s", p.Title, name)
	})
	pg.OnDownloadDone(func(d *mygo.Download) {
		named := name
		if d.Err != nil {
			// (another try may follow)
			name = ""
			os.Remove(d.Path)
			a.say("%s: %v", p.Title, d.Err)
			return
		}
		mygo.RunOnMain(func() { win.Close() })
		a.job("Importing "+named, func() error {
			// (what did not come in has no name to be kept under)
			defer os.Remove(d.Path)
			got := false
			err := a.cli(importArgs(lib, named, d.Path), func(r Record) {
				if r.S("state") == "other" {
					a.say("%s: %s is no archive or chart: not kept", p.Title, named)
					return
				}
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
