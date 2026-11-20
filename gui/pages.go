package main

import (
	"net/url"

	"github.com/egoist/mygo/ui"
)

func (a *app) pagesView(c *ui.Context) {
	ui.Text(c, pagesHint)
	ui.Row(c).Gap(8).Children(func() {
		ok := a.prow < len(a.pages) && a.settings.Library != ""
		if ui.PrimaryButton(c, "Open").Disabled(!ok).Clicked() {
			a.openPage(a.pages[a.prow])
		}
		a.importButton(c)
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

// browsable reports whether p's link is one a browser is given: http or
// https, with a host. Any other (a table may say anything where a link
// goes: a file's path, a script) is noted in the log and never opened.
func (a *app) browsable(p page) bool {
	u, err := url.Parse(p.Link)
	if err == nil && (u.Scheme == "http" || u.Scheme == "https") && u.Host != "" {
		return true
	}
	a.log = append(a.log, p.Title+": not a web link, not opened: "+p.Link)
	return false
}

// forgetPage takes p off the list of pages.
func (a *app) forgetPage(p page) {
	for i, x := range a.pages {
		if x == p {
			a.pages = append(a.pages[:i], a.pages[i+1:]...)
			return
		}
	}
}
