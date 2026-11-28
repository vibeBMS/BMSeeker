package main

import (
	"fmt"
	"slices"
	"strings"
	"sync"
	"time"

	"github.com/egoist/mygo"
	"github.com/egoist/mygo/ui"
)

// A chart a table lists that the library lacks.
type missing struct {
	Label, Level, Title, Artist, MD5, SHA256, URL, URLDiff string
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
	win *mygo.Window
	// post runs a change of the state on the window's goroutine (the
	// window's Update; a test's own).
	post     func(func())
	settings Settings
	tab      int
	// The tab the last frame showed.
	shownTab int

	// Tables
	table string
	level string
	// The charts the library lacks, of the table missingFrom.
	missing     []missing
	missingFrom string
	rows        ui.ListState
	row         int
	newTable    string

	// Link
	link, title, artist string

	// Waiting
	waiting []waiting
	wrows   ui.ListState

	// Pages for a browser
	pages []page
	prows ui.ListState
	prow  int
	// What a page's window runs on each page (the CLI's: pageScript).
	scriptOnce sync.Once
	script     string

	// Asking before third-party sources are turned on
	askThirdParty bool
	// Third-party sources are on in the CLI's settings file: on whatever
	// the window says. Its notice is then said once (the window did not
	// ask).
	configThirdParty bool
	toldThirdParty   bool

	// The licences (the CLI's `licenses`), read once a session; shown in
	// Settings in place of the settings while licencesOpen.
	licences     string
	licencesOpen bool

	// Hosts that refused BMSeeker in this session: the CLI skips them
	// (--skip-host) in every later run.
	refused []string

	// What is running, and what was said
	busy    map[string]bool
	log     []string
	logRows ui.ListState
	mu      sync.Mutex
}

func newApp(library string) *app {
	a := &app{settings: Settings{Library: library}, busy: map[string]bool{}}
	a.rows.Selected = &a.row
	a.prows.Selected = &a.prow
	a.logRows.FollowEnd = true
	return a
}

const readingSettings = "Reading the settings"

// readSettings takes what the window starts from from the CLI's settings
// in effect (`config show`): the tables, the downloads folder, whether
// third-party sources are on. Until they are read, opening Settings or
// starting a job reads them again.
func (a *app) readSettings() {
	a.job(readingSettings, func() error {
		return a.cli([]string{"config", "show"}, "", func(r Record) {
			c, ok := r["config"].(map[string]any)
			if !ok {
				return
			}
			var tables []string
			list, _ := c["tables"].([]any)
			for _, t := range list {
				if t, ok := t.(string); ok {
					tables = append(tables, t)
				}
			}
			downloads, _ := c["downloads"].(string)
			on, _ := c["third_party"].(bool)
			a.update(func() {
				if a.settings.Tables == nil {
					a.settings.Tables = tables
				}
				if a.table == "" && len(a.settings.Tables) > 0 {
					a.table = a.settings.Tables[0]
				}
				a.settings.Downloads, a.configThirdParty = downloads, on
				a.settings.Read = true
			})
		})
	})
}

// update changes the state from another goroutine.
func (a *app) update(f func()) {
	a.post(f)
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
	if !a.settings.Read && name != readingSettings {
		a.readSettings()
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

// cli runs a command (stdin what it reads for "-"), noting what it says in
// the log. A host that refuses BMSeeker (a `refused` record) is noted and
// skipped for the rest of the session; a `notice` record is noted (the
// third-party sources' only once, and only when the window did not ask
// before they were turned on: the settings file turned them on). Neither
// goes to onRecord.
//
// What a table, a server or a file's name says goes into args only as an
// option's value attached to it ("--title="+title) or after "--" (links,
// files): the CLI then reads none of it as an option, whatever it starts
// with.
func (a *app) cli(args []string, stdin string, onRecord func(Record)) error {
	var pre []string
	asked := a.settings.ThirdParty
	if asked {
		pre = append(pre, "--third-party")
	}
	a.mu.Lock()
	for _, h := range a.refused {
		pre = append(pre, "--skip-host="+h)
	}
	a.mu.Unlock()
	args = append(pre, args...)
	return runCLI(args, stdin, func(r Record) {
		if host := r.S("refused"); host != "" {
			a.mu.Lock()
			if !slices.Contains(a.refused, host) {
				a.refused = append(a.refused, host)
			}
			a.mu.Unlock()
			a.say("%s", r.S("error"))
			return
		}
		if notice := r.S("notice"); notice != "" {
			text := r.S("text")
			switch {
			case notice != "third_party":
				a.say("%s", text)
			case !asked:
				a.update(func() {
					if !a.toldThirdParty {
						a.toldThirdParty = true
						a.log = append(a.log, time.Now().Format("15:04:05 ")+text)
					}
				})
			}
			return
		}
		if onRecord != nil {
			onRecord(r)
		}
	}, func(note string) { a.say("%s", note) })
}

func (a *app) view(c *ui.Context) {
	ui.Column(c).Fill().Padding(12).Gap(10).Children(func() {
		ui.Row(c).Gap(8).Children(func() {
			ui.Text(c, name()).FontSize(18).Bold()
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
			ui.Text(c, noLibrary)
		}
		labels := []string{"Tables", "Link", "Waiting", "Settings"}
		if len(a.pages) > 0 {
			labels = append(labels, fmt.Sprintf("Browser (%d)", len(a.pages)))
		}
		ui.Tabs(c, &a.tab, labels...)
		if a.tab == 3 && a.shownTab != 3 && !a.settings.Read {
			a.readSettings()
		}
		a.shownTab = a.tab
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
		if ui.PrimaryButton(c, "Missing charts").Disabled(a.settings.Library == "" || a.table == "").Clicked() {
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
		err := a.cli([]string{"table", "missing", "--library=" + lib, "--", url}, "", func(r Record) {
			got = append(got, missing{
				Label: r.S("label"), Level: r.S("level"), Title: r.S("title"), Artist: r.S("artist"),
				MD5: r.S("md5"), SHA256: r.S("sha256"), URL: r.S("url"), URLDiff: r.S("url_diff"),
			})
		})
		a.update(func() {
			a.missing, a.missingFrom, a.level, a.row = got, url, "", 0
		})
		return err
	})
}

// download installs the charts ms of the table the list is of, as the CLI
// does a table's: one run of `table missing --get`, their MD5s (or SHA-256s
// where the table gives no MD5) on its stdin, one a line (`--chosen -`;
// thousands do not fit a command line): each song, then its separate
// chart, or the chart first when the library has its song; each file
// downloaded once. Each chart's last record says whether the library has
// it now (`have`): those it has leave the list, the others stay, whatever
// came of the downloads on the way.
func (a *app) download(ms []missing) {
	lib, url := a.settings.Library, a.missingFrom
	var chosen strings.Builder
	titles := map[string]string{}
	n := 0
	for _, m := range ms {
		hash := m.MD5
		if hash == "" {
			hash = m.SHA256
		}
		if hash == "" {
			// (the CLI lists none: a table's entry without a hash is no chart to it)
			a.say("%s: the table gives no hash for it: download it from the Link tab", m.Title)
			continue
		}
		chosen.WriteString(hash + "\n")
		titles[hash] = m.Title
		n++
	}
	if n == 0 {
		return
	}
	args := []string{"table", "missing", "--library=" + lib, "--get", "--chosen=-", "--", url}
	a.job("Downloading", func() error {
		done := 0
		return a.cli(args, chosen.String(), func(r Record) {
			hash := r.S("md5")
			if hash == "" {
				hash = r.S("sha256")
			}
			title := r.S("title")
			if title == "" {
				title = titles[hash]
			}
			have, last := r["have"].(bool)
			said := a.outcome(title, r)
			if !last {
				if said != "" {
					a.say("%s", said)
				}
				return
			}
			done++
			a.say("(%d/%d) %s", done, n, said)
			if have {
				a.update(func() { a.forget(hash) })
			}
		})
	})
}

// installArgs is a `library install` of a song (its title, artist and
// chart's MD5) from links, with flags.
func installArgs(lib, title, artist, md5 string, flags []string, links ...string) []string {
	args := []string{"library", "install", "--library=" + lib, "--title=" + title, "--artist=" + artist, "--md5=" + md5}
	args = append(args, flags...)
	args = append(args, "--")
	return append(args, links...)
}

// outcome is what a download came to (a record's "state"), for the log ("":
// a record of no download); a link that needs a browser goes to the
// Browser tab.
func (a *app) outcome(title string, r Record) string {
	switch r.S("state") {
	case "added", "owned", "placed", "waiting":
		return fmt.Sprintf("%s: %s %s", title, r.S("state"), r.S("path"))
	case "page":
		link := r.S("link")
		a.update(func() { a.pages = append(a.pages, page{Title: title, Link: link}) })
		return fmt.Sprintf("%s: needs a browser: %s", title, link)
	case "failed":
		return fmt.Sprintf("%s: %s", title, r.S("error"))
	}
	return ""
}

// forget takes the chart of this hash off the list: its MD5, else (a table
// that gives none) its SHA-256.
func (a *app) forget(hash string) {
	a.missing = slices.DeleteFunc(a.missing, func(m missing) bool {
		return hash != "" && (m.MD5 == hash || m.MD5 == "" && m.SHA256 == hash)
	})
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
			// (a link that brought nothing has said why)
			_ = a.cli(installArgs(lib, title, artist, "", []string{"--own-links"}, link), "", func(r Record) {
				if said := a.outcome(name, r); said != "" {
					a.say("%s", said)
				}
			})
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
				err := a.cli([]string{"library", "pending", "--library=" + lib}, "", func(r Record) {
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
	if a.licencesOpen {
		a.licencesView(c)
		return
	}
	a.libraryView(c)
	ui.Text(c, "Downloads").Bold()
	on := a.settings.ThirdParty || a.configThirdParty
	if ui.Checkbox(c, &on, "Third-party sources (mirrors, Ginger, Konmai, the community archive)").Changed() {
		switch {
		case a.configThirdParty:
			a.say("third-party sources are on in bmseeker's settings file (config.json): they are turned off there")
		case on && !thirdPartyAcknowledged():
			a.askThirdParty = true
		default:
			a.settings.ThirdParty = on
		}
	}
	if ui.AlertDialog(c, &a.askThirdParty, thirdPartyTitle, thirdPartyWarning, "Cancel", "Turn on") == 1 {
		if err := acknowledgeThirdParty(); err != nil {
			a.say("%v", err)
		}
		a.settings.ThirdParty = true
	}
	ui.Text(c, "Other people's copies of dead sites, uploaders and packages. What is found by name goes in only if it holds the chart (MD5).")
	ui.Text(c, "Difficulty tables").Bold()
	for i, t := range a.settings.Tables {
		ui.Row(c).Gap(8).Children(func() {
			ui.Text(c, t).Grow(1).SingleLine()
			if ui.Button(c, "Remove").Clicked() {
				a.settings.Tables = append(a.settings.Tables[:i:i], a.settings.Tables[i+1:]...)
			}
		})
	}
	ui.Row(c).Gap(8).Children(func() {
		add := ui.TextInput(c, &a.newTable).Placeholder("A table's URL").Grow(1).Submitted()
		if (ui.Button(c, "Add").Clicked() || add) && strings.HasPrefix(a.newTable, "http") {
			a.settings.Tables = append(a.settings.Tables, strings.TrimSpace(a.newTable))
			a.newTable = ""
		}
	})
	ui.Text(c, "About").Bold()
	ui.Row(c).Gap(8).Children(func() {
		ui.Text(c, name()+" "+version())
		if ui.Button(c, "Licences").Clicked() {
			a.openLicences()
		}
	})
}

const readingLicences = "Reading the licences"

// openLicences shows the licences: BMSeeker's, the notices of the works it
// is derived from, and the licences of the crates and Go modules it is
// built from (this window's modules too), as the CLI has them compiled in.
func (a *app) openLicences() {
	a.licencesOpen = true
	if a.licences != "" {
		return
	}
	a.job(readingLicences, func() error {
		return a.cli([]string{"licenses"}, "", func(r Record) {
			if _, ok := r["licenses"]; ok {
				text := r.S("text")
				a.update(func() { a.licences = text })
			}
		})
	})
}

// licencesView shows the licences in a text area, which lays out only the
// lines in view (the text is some 330 KB): read-only, its text can be
// selected and copied.
func (a *app) licencesView(c *ui.Context) {
	ui.Row(c).Gap(8).Children(func() {
		if ui.Button(c, "Back").Clicked() {
			a.licencesOpen = false
		}
		ui.Text(c, "Licences").Bold()
	})
	placeholder := ""
	if a.busy[readingLicences] {
		placeholder = readingLicences + "…"
	}
	ui.TextArea(c, &a.licences).ReadOnly(true).Label("Licences").Placeholder(placeholder).Grow(1)
}

func toF(v any) float64 {
	f, _ := v.(float64)
	return f
}
