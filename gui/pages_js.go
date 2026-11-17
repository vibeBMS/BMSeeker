//go:build js

package main

import (
	"github.com/egoist/mygo"
	"github.com/egoist/mygo/ui"
)

const pagesHint = "These links give a page, not a file: open one (a new tab), download its file there, then bring it in with Import."

// openPage opens p's link in a tab of its own.
func (a *app) openPage(p page) {
	if err := mygo.Shell.OpenExternal(p.Link); err != nil {
		a.log = append(a.log, err.Error())
	}
}

// importButton brings in a file the user downloaded (a page's, chosen
// row's, which leaves the list once it came in).
func (a *app) importButton(c *ui.Context) {
	if !ui.Button(c, "Import a downloaded file…").Disabled(a.settings.Library == "").Clicked() {
		return
	}
	var from *page
	if a.prow < len(a.pages) {
		p := a.pages[a.prow]
		from = &p
	}
	// (asked now: the browser opens its file chooser only on a click)
	chosen := shim().Call("pickFile")
	lib := a.settings.Library
	go func() {
		name, err := await(chosen)
		if err != nil || name.String() == "" {
			return
		}
		a.update(func() {
			a.job("Importing", func() error {
				got := false
				err := a.cli([]string{"library", "import", "{file}", "-L", lib}, func(r Record) {
					got = true
					a.say("%s: %s %s", name.String(), r.S("state"), r.S("path"))
				})
				if got && from != nil {
					a.update(func() { a.forgetPage(*from) })
				}
				return err
			})
		})
	}()
}
