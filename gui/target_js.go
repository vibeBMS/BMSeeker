//go:build js

package main

import (
	"fmt"
	"syscall/js"

	"github.com/egoist/mygo"
	"github.com/egoist/mygo/ui"
)

const noLibrary = "Choose where songs go in Settings first."

// What the page says of where songs go (web/www/bmseeker.js' state).
type target struct {
	kind, label, library, folder, err string
	ready, needsPermission            bool
	files                             int
	supports                          map[string]bool
}

var (
	where target
	// the target chosen in Settings (the page's, once it has opened it)
	choice string
)

var targets = []struct{ kind, label, about string }{
	{"vibe", "vibeBMS web library (this browser)",
		"Songs go into vibeBMS's library in this browser: open vibeBMS pages see them at once."},
	{"folder", "A folder on this computer (Chrome/Edge)",
		"Songs go into a folder of yours: LR2's or beatoraja's songs. What changes there meanwhile is seen once it is chosen again."},
	{"zip", "Download as zip",
		"Songs gather here while the page is open, then download as one zip."},
}

func readTarget() target {
	s := shim().Call("state")
	t := target{
		kind: s.Get("kind").String(), label: s.Get("label").String(),
		library: s.Get("library").String(), folder: s.Get("folder").String(),
		err: s.Get("error").String(), ready: s.Get("ready").Bool(),
		needsPermission: s.Get("needsPermission").Bool(), files: s.Get("files").Int(),
		supports: map[string]bool{},
	}
	for _, x := range targets {
		t.supports[x.kind] = s.Get("supports").Get(x.kind).Bool()
	}
	return t
}

// connect follows the page's target: the library the commands work on.
func (a *app) connect() {
	sync := func() {
		t := readTarget()
		a.update(func() {
			where, choice = t, t.kind
			a.settings.Library = ""
			if t.ready {
				a.settings.Library = t.library
			}
		})
	}
	// (not in the callback: JavaScript waits for it)
	shim().Set("onchange", js.FuncOf(func(js.Value, []js.Value) any {
		go sync()
		return nil
	}))
	sync()
}

// libraryView chooses where songs go.
func (a *app) libraryView(c *ui.Context) {
	ui.Text(c, "Where songs go").Bold()
	ui.RadioGroup(c, func() {
		for _, x := range targets {
			if ui.Radio(c, &choice, x.kind, x.label).Disabled(!where.supports[x.kind]).Changed() {
				// (now: the browser shows a folder picker only on a click)
				p := shim().Call("choose", x.kind)
				go func() {
					_, _ = await(p)
					a.update(func() { choice = where.kind })
				}()
			}
		}
	}).Label("Where songs go")
	for _, x := range targets {
		if x.kind == where.kind {
			ui.Text(c, x.about)
		}
	}
	ui.Row(c).Gap(8).Children(func() {
		switch {
		case where.needsPermission:
			if ui.PrimaryButton(c, "Allow "+where.folder+" again").Clicked() {
				shim().Call("allow")
			}
		case where.ready && where.folder != "":
			ui.Text(c, fmt.Sprintf("%s: %d files", where.folder, where.files)).SingleLine()
		case where.ready:
			ui.Text(c, fmt.Sprintf("%d files", where.files))
		case where.kind != "":
			ui.Spinner(c)
		}
		ui.Spacer(c)
		switch where.kind {
		case "vibe":
			if ui.Button(c, "Open vibeBMS").Clicked() {
				if err := mygo.Shell.OpenExternal(shim().Get("vibeUrl").String()); err != nil {
					a.log = append(a.log, err.Error())
				}
			}
		case "zip":
			if ui.PrimaryButton(c, "Download the zip").Disabled(!where.ready || where.files == 0).Clicked() {
				p := shim().Call("downloadZip")
				a.job("Zipping", func() error {
					n, err := await(p)
					if err == nil {
						a.say("zip downloaded: %.0f KB", n.Float()/1e3)
					}
					return err
				})
			}
		}
	})
	if where.err != "" {
		ui.Text(c, where.err)
	}
}
