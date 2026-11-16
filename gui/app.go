package main

import (
	"fmt"
	"strings"
	"sync"
	"time"

	"github.com/egoist/mygo"
	"github.com/egoist/mygo/ui"
)

// A chart a table lists that the library lacks.
type missing struct {
	Label, Level, Title, Artist, MD5, URL, URLDiff string
}

// A link that needs a person in a browser.
type page struct {
	Title, Link string
}

// A chart waiting in the library for its song.
type waiting struct {
	Chart, State, Best string
}

type app struct {
	win      *mygo.Window
	settings Settings
	tab      int

	// Tables
	table    string
	level    string
	missing  []missing
	rows     ui.ListState
	row      int
	newTable string

	// Link
	link, title, artist string

	// Waiting
	waiting []waiting
	wrows   ui.ListState

	// Pages for a browser
	pages []page
	prows ui.ListState
	prow  int

	// What is running, and what was said
	busy    map[string]bool
	log     []string
	logRows ui.ListState
	mu      sync.Mutex
}

func newApp(library string) *app {
	a := &app{settings: loadSettings(), busy: map[string]bool{}}
	if library != "" {
		a.settings.Library = library
	}
	if len(a.settings.Tables) > 0 {
		a.table = a.settings.Tables[0]
	}
	a.rows.Selected = &a.row
	a.prows.Selected = &a.prow
	a.logRows.FollowEnd = true
	return a
}

// update changes the state from another goroutine.
func (a *app) update(f func()) {
	if a.win != nil {
		a.win.Update(f)
	} else {
		f()
	}
}

func (a *app) say(format string, args ...any) {
	line := time.Now().Format("15:04:05 ") + fmt.Sprintf(format, args...)
	a.update(func() { a.log = append(a.log, line) })
}

// job runs f on a goroutine of its own, marked busy as `name`.
func (a *app) job(name string, f func() error) {
	if a.busy[name] {
		return
	}
	a.busy[name] = true
	go func() {
		err := f()
		a.update(func() {
			delete(a.busy, name)
			if err != nil {
				a.log = append(a.log, time.Now().Format("15:04:05 ")+name+": "+err.Error())
			}
		})
	}()
}

// cli runs a command, noting what it says in the log.
func (a *app) cli(args []string, onRecord func(Record)) error {
	if a.settings.ThirdParty {
		args = append([]string{"--third-party"}, args...)
	}
	return runCLI(args, onRecord, func(note string) { a.say("%s", note) })
}

func (a *app) view(c *ui.Context) {
	ui.Column(c).Fill().Padding(12).Gap(10).Children(func() {
		ui.Row(c).Gap(8).Children(func() {
			ui.Text(c, "BMSeeker").FontSize(18).Bold()
			ui.Spacer(c)
			if len(a.busy) > 0 {
				ui.Spinner(c)
				names := make([]string, 0, len(a.busy))
				for n := range a.busy {
					names = append(names, n)
				}
				ui.Text(c, strings.Join(names, ", ")).SingleLine()
			}
		})
		if a.settings.Library == "" {
			ui.Text(c, "Choose your BMS folder in Settings first.")
		}
		labels := []string{"Tables", "Link", "Waiting", "Settings"}
		if len(a.pages) > 0 {
			labels = append(labels, fmt.Sprintf("Browser (%d)", len(a.pages)))
		}
		ui.Tabs(c, &a.tab, labels...)
		ui.Column(c).Grow(1).Gap(8).Children(func() {
			switch a.tab {
			case 0:
				a.tablesView(c)
			case 1:
				a.linkView(c)
			case 2:
				a.waitingView(c)
			case 3:
				a.settingsView(c)
			case 4:
				a.pagesView(c)
			}
		})
		ui.Divider(c)
		ui.List(c, &a.logRows, len(a.log), func(i int) {
			ui.Text(c, a.log[i]).SingleLine()
		}).Height(120)
	})
}

func (a *app) tablesView(c *ui.Context) {
	ui.Row(c).Gap(8).Children(func() {
		ui.Select(c, &a.table, a.settings.Tables).Grow(1)
		if ui.PrimaryButton(c, "Missing charts").Disabled(a.settings.Library == "").Clicked() {
			a.loadMissing()
		}
	})
	levels := []string{"All levels"}
	seen := map[string]bool{}
	for _, m := range a.missing {
		if !seen[m.Label] {
			seen[m.Label] = true
			levels = append(levels, m.Label)
		}
	}
	if a.level == "" {
		a.level = levels[0]
	}
	shown := a.shown()
	ui.Row(c).Gap(8).Children(func() {
		ui.Select(c, &a.level, levels)
		ui.Text(c, fmt.Sprintf("%d missing", len(shown)))
		ui.Spacer(c)
		if ui.Button(c, "Download chosen").Disabled(len(shown) == 0).Clicked() && a.row < len(shown) {
			a.download([]missing{shown[a.row]})
		}
		if ui.PrimaryButton(c, "Download all shown").Disabled(len(shown) == 0).Clicked() {
			a.download(shown)
		}
	})
	cols := []ui.TableColumn{{Title: "Level", Width: 70}, {Title: "Title"}, {Title: "Artist"}}
	if ui.Table(c, &a.rows, cols, len(shown), func(row, col int) {
		m := shown[row]
		switch col {
		case 0:
			ui.Text(c, m.Label).SingleLine()
		case 1:
			ui.Text(c, m.Title).SingleLine()
		case 2:
			ui.Text(c, m.Artist).SingleLine()
		}
	}).Grow(1).Submitted() && a.row < len(shown) {
		a.download([]missing{shown[a.row]})
	}
}

// shown are the missing charts of the level chosen.
func (a *app) shown() []missing {
	if a.level == "" || a.level == "All levels" {
		return a.missing
	}
	var out []missing
	for _, m := range a.missing {
		if m.Label == a.level {
			out = append(out, m)
		}
	}
	return out
}

func (a *app) loadMissing() {
	url, lib := a.table, a.settings.Library
	a.job("Reading the table", func() error {
		var got []missing
		err := a.cli([]string{"table", "missing", url, "-L", lib}, func(r Record) {
			got = append(got, missing{
				Label: r.S("label"), Level: r.S("level"), Title: r.S("title"), Artist: r.S("artist"),
				MD5: r.S("md5"), URL: r.S("url"), URLDiff: r.S("url_diff"),
			})
		})
		a.update(func() {
			a.missing, a.level, a.row = got, "", 0
		})
		return err
	})
}

// download installs each chart's song, then its separate chart, one after
// another.
func (a *app) download(ms []missing) {
	lib := a.settings.Library
	ms = append([]missing(nil), ms...)
	a.job("Downloading", func() error {
		for i, m := range ms {
			a.say("(%d/%d) %s", i+1, len(ms), m.Title)
			args := []string{"library", "install", "-L", lib, "--title", m.Title, "--artist", m.Artist, "--md5", m.MD5}
			if m.URL != "" {
				args = append(args, m.URL)
			}
			got := a.install(m.Title, args)
			if m.URLDiff != "" {
				a.install(m.Title, []string{"library", "install", "-L", lib, "--diff", "--md5", m.MD5,
					"--title", m.Title, "--artist", m.Artist, m.URLDiff})
			}
			if got {
				done := m
				a.update(func() { a.forget(done) })
			}
		}
		return nil
	})
}

// install runs one `library install`; links that need a browser go to the
// Browser tab. Whether it brought something in.
func (a *app) install(title string, args []string) bool {
	got := false
	_ = a.cli(args, func(r Record) {
		switch r.S("state") {
		case "added", "placed", "waiting":
			got = true
			a.say("%s: %s %s", title, r.S("state"), r.S("path"))
		case "page":
			link := r.S("link")
			a.say("%s: needs a browser: %s", title, link)
			a.update(func() { a.pages = append(a.pages, page{Title: title, Link: link}) })
		case "failed":
			a.say("%s: %s", title, r.S("error"))
		}
	})
	return got
}

func (a *app) forget(m missing) {
	for i, x := range a.missing {
		if x == m {
			a.missing = append(a.missing[:i], a.missing[i+1:]...)
			return
		}
	}
}

func (a *app) linkView(c *ui.Context) {
	ui.Text(c, "A link to a song's archive (Google Drive, MEGA, a manbow work, ...):")
	ui.TextInput(c, &a.link).Placeholder("https://")
	ui.Row(c).Gap(8).Children(func() {
		ui.TextInput(c, &a.title).Placeholder("Title (a folder of many songs: which)").Grow(1)
		ui.TextInput(c, &a.artist).Placeholder("Artist").Grow(1)
	})
	ready := a.settings.Library != "" && strings.HasPrefix(a.link, "http")
	if ui.PrimaryButton(c, "Into the library").Disabled(!ready).Clicked() {
		link, title, artist, lib := a.link, a.title, a.artist, a.settings.Library
		name := title
		if name == "" {
			name = link
		}
		a.job("Downloading", func() error {
			a.install(name, []string{"library", "install", "-L", lib, "--title", title, "--artist", artist, "--own-links", link})
			return nil
		})
	}
}

func (a *app) waitingView(c *ui.Context) {
	ui.Row(c).Gap(8).Children(func() {
		ui.Text(c, "Charts that came before their song:")
		ui.Spacer(c)
		if ui.Button(c, "Look again").Disabled(a.settings.Library == "").Clicked() {
			lib := a.settings.Library
			a.job("Looking at waiting charts", func() error {
				var got []waiting
				err := a.cli([]string{"library", "pending", "-L", lib}, func(r Record) {
					best := ""
					if b, ok := r["best"].(map[string]any); ok {
						best = fmt.Sprintf("%.0f%% %v", toF(b["coverage"])*100, b["label"])
					}
					got = append(got, waiting{Chart: r.S("chart"), State: r.S("state"), Best: best})
				})
				a.update(func() { a.waiting = got })
				return err
			})
		}
	})
	cols := []ui.TableColumn{{Title: "Chart"}, {Title: "State", Width: 90}, {Title: "Best folder"}}
	ui.Table(c, &a.wrows, cols, len(a.waiting), func(row, col int) {
		w := a.waiting[row]
		switch col {
		case 0:
			ui.Text(c, w.Chart).SingleLine()
		case 1:
			ui.Text(c, w.State)
		case 2:
			ui.Text(c, w.Best).SingleLine()
		}
	}).Grow(1)
}

func (a *app) settingsView(c *ui.Context) {
	ui.Text(c, "BMS folder").Bold()
	ui.Row(c).Gap(8).Children(func() {
		lib := a.settings.Library
		if lib == "" {
			lib = "(none)"
		}
		ui.Text(c, lib).Grow(1).SingleLine()
		if ui.Button(c, "Choose…").Clicked() {
			go func() {
				paths, err := mygo.Dialog.Open(mygo.OpenDialogOptions{Parent: a.win, Title: "BMS folder", Directory: true})
				if err == nil && len(paths) > 0 {
					a.update(func() {
						a.settings.Library = paths[0]
						a.saveSettings()
					})
				}
			}()
		}
	})
	ui.Text(c, "Downloads").Bold()
	if ui.Checkbox(c, &a.settings.ThirdParty, "Third-party mirrors (darksabun.club, bms.hexlataia.xyz, the bms.kyouko.moe archive)").Changed() {
		a.saveSettings()
	}
	ui.Text(c, "Other people's copies of dead sites and uploaders. What is found by name goes in only if it holds the chart (MD5).")
	ui.Text(c, "Difficulty tables").Bold()
	for i, t := range a.settings.Tables {
		ui.Row(c).Gap(8).Children(func() {
			ui.Text(c, t).Grow(1).SingleLine()
			if ui.Button(c, "Remove").Clicked() {
				a.settings.Tables = append(a.settings.Tables[:i:i], a.settings.Tables[i+1:]...)
				a.saveSettings()
			}
		})
	}
	ui.Row(c).Gap(8).Children(func() {
		add := ui.TextInput(c, &a.newTable).Placeholder("A table's URL").Grow(1).Submitted()
		if (ui.Button(c, "Add").Clicked() || add) && strings.HasPrefix(a.newTable, "http") {
			a.settings.Tables = append(a.settings.Tables, strings.TrimSpace(a.newTable))
			a.newTable = ""
			a.saveSettings()
		}
	})
}

func (a *app) saveSettings() {
	if err := a.settings.save(); err != nil {
		a.log = append(a.log, "settings: "+err.Error())
	}
}

func toF(v any) float64 {
	f, _ := v.(float64)
	return f
}
