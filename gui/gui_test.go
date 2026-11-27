//go:build !windows && !js

package main

import (
	"crypto/sha256"
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"slices"
	"strings"
	"sync"
	"testing"
	"time"

	"github.com/egoist/mygo/ui"
)

// The protocol's fixture: runs of the CLI, each its command line, the
// lines it wrote in order and its exit code, as the CLI's own test writes
// them (src/cli/mod.rs, the_records_the_window_reads).
const fixture = "../testdata/records.jsonl"

// A run of the CLI the fixture holds.
type run struct {
	Args  []string `json:"args"`
	Lines []struct {
		Record json.RawMessage `json:"record"`
		Note   *string         `json:"note"`
	} `json:"lines"`
	Exit int `json:"exit"`
}

// Started with $BMSEEKER_REPLAY (the fixture's path), this test binary is
// the CLI as the fixture has it.
func TestMain(m *testing.M) {
	if f := os.Getenv("BMSEEKER_REPLAY"); f != "" {
		os.Exit(replay(f, os.Args[1:]))
	}
	os.Exit(m.Run())
}

// replay writes what the CLI wrote for args in the fixture file's run of
// them, and ends as it did. A command line it has no run of fails.
func replay(file string, args []string) int {
	data, err := os.ReadFile(file)
	if err != nil {
		fmt.Fprintln(os.Stderr, "bmseeker:", err)
		return 2
	}
	for _, l := range strings.Split(strings.TrimSpace(string(data)), "\n") {
		var r run
		if err := json.Unmarshal([]byte(l), &r); err != nil {
			fmt.Fprintln(os.Stderr, "bmseeker:", err)
			return 2
		}
		if !slices.Equal(r.Args, args) {
			continue
		}
		for _, x := range r.Lines {
			if x.Note != nil {
				fmt.Fprintln(os.Stderr, *x.Note)
			} else {
				fmt.Println(string(x.Record))
			}
		}
		return r.Exit
	}
	fmt.Fprintf(os.Stderr, "bmseeker: the fixture has no run of %q\n", args)
	return 2
}

func until(t *testing.T, ok func() bool) {
	t.Helper()
	for i := 0; i < 200 && !ok(); i++ {
		time.Sleep(10 * time.Millisecond)
	}
	if !ok() {
		t.Fatal("timed out")
	}
}

// fakeApp is a window on the library /lib, in a home of its own, whose CLI
// answers as the fixture says.
func fakeApp(t *testing.T) *app {
	dir := t.TempDir()
	t.Setenv("XDG_CONFIG_HOME", dir)
	t.Setenv("HOME", dir)
	self, err := os.Executable()
	if err != nil {
		t.Fatal(err)
	}
	file, err := filepath.Abs(fixture)
	if err != nil {
		t.Fatal(err)
	}
	t.Setenv("BMSEEKER_CLI", self)
	t.Setenv("BMSEEKER_REPLAY", file)
	cliOnce = sync.Once{}
	return newApp("/lib")
}

// The window on the fixture's runs: its settings, a table's missing
// charts, those shown downloaded in one run (one into the library, one a
// page for the Browser tab, one whose source refused), and a later run
// that skips the host that refused, with third-party sources on, whose
// notice it does not repeat.
func TestTablesToBrowser(t *testing.T) {
	a := fakeApp(t)
	a.readSettings()
	until(t, func() bool { return len(a.busy) == 0 })
	if !slices.Equal(a.settings.Tables, []string{"https://t.example/table.json"}) || a.table != a.settings.Tables[0] ||
		a.settings.downloads() != filepath.Join("/lib", ".bmseeker") || a.configThirdParty {
		t.Fatalf("settings %+v, table %q, log %q", a.settings, a.table, a.log)
	}
	tt := ui.NewTester(a.view, 980, 720)
	if err := tt.Click("Missing charts"); err != nil {
		t.Fatal(err)
	}
	until(t, func() bool { return len(a.missing) == 3 && len(a.busy) == 0 })
	tt.Frame()
	if !tt.HasText("Song A") || !tt.HasText("3 missing") {
		t.Fatalf("texts %q", tt.Texts())
	}
	if err := tt.Click("Download all shown"); err != nil {
		t.Fatal(err)
	}
	until(t, func() bool { return len(a.busy) == 0 && len(a.pages) == 1 })
	if len(a.missing) != 2 || a.missing[0].Title != "Song B" || a.missing[1].Title != "Song C" {
		t.Fatalf("missing %v, log %q", a.missing, a.log)
	}
	tt.Frame()
	if !tt.HasText("Browser (1)") {
		t.Fatalf("texts %q", tt.Texts())
	}
	joined := strings.Join(a.log, "\n")
	for _, said := range []string{"3 of 3 charts missing", "Song A: added /lib/Song A",
		"Song B: needs a browser: https://b.example/b", "bmssearch.net refused BMSeeker (HTTP 403)", "Song C: no link"} {
		if !strings.Contains(joined, said) {
			t.Fatalf("%q not in the log %q", said, joined)
		}
	}
	if !slices.Equal(a.refused, []string{"bmssearch.net"}) {
		t.Fatalf("refused %q", a.refused)
	}
	// later runs skip it; the third-party setting goes to the CLI, which
	// the fixture answers only for this command line
	a.settings.ThirdParty = true
	got := 0
	if err := a.cli([]string{"library", "pending", "--library=/lib"}, func(Record) { got++ }); err != nil {
		t.Fatal(err)
	}
	joined = strings.Join(a.log, "\n")
	if got != 0 || !strings.Contains(joined, "0 charts waiting") || strings.Contains(joined, "third-party sources are on") {
		t.Fatalf("records %d, log %q", got, joined)
	}
	// every tab draws
	for _, tab := range []string{"Link", "Waiting", "Settings", "Browser (1)"} {
		if err := tt.Click(tab); err != nil {
			t.Fatal(tab, err)
		}
		tt.Frame()
	}
}

func TestThirdPartyAsksFirst(t *testing.T) {
	a := fakeApp(t)
	tt := ui.NewTester(a.view, 980, 720)
	if err := tt.Click("Settings"); err != nil {
		t.Fatal(err)
	}
	tt.Frame()
	box := "Third-party sources (mirrors, Ginger, Konmai, the community archive)"
	// asked first: Cancel leaves it off
	if err := tt.Click(box); err != nil {
		t.Fatal(err)
	}
	tt.Frame()
	if a.settings.ThirdParty || !tt.HasText(thirdPartyTitle) || !tt.HasText("your responsibility") {
		t.Fatalf("on %v, texts %q", a.settings.ThirdParty, tt.Texts())
	}
	if err := tt.Click("Cancel"); err != nil {
		t.Fatal(err)
	}
	tt.Frame()
	if a.settings.ThirdParty || thirdPartyAcknowledged() {
		t.Fatal("turned on without an OK")
	}
	// OK turns it on, and is remembered
	if err := tt.Click(box); err != nil {
		t.Fatal(err)
	}
	tt.Frame()
	if err := tt.Click("Turn on"); err != nil {
		t.Fatal(err)
	}
	tt.Frame()
	if !a.settings.ThirdParty || !thirdPartyAcknowledged() {
		t.Fatalf("on %v, remembered %v, log %q", a.settings.ThirdParty, thirdPartyAcknowledged(), a.log)
	}
	// off, and on again: not asked again
	for _, want := range []bool{false, true} {
		if err := tt.Click(box); err != nil {
			t.Fatal(err)
		}
		tt.Frame()
		if a.settings.ThirdParty != want || tt.HasText(thirdPartyTitle) {
			t.Fatalf("want %v: on %v, texts %q", want, a.settings.ThirdParty, tt.Texts())
		}
	}
}

// wordsCLI notes each argument it was given on a line of its own, and an
// empty line after a command's last.
const wordsCLI = `#!/bin/sh
{ for a in "$@"; do printf '%s\n' "$a"; done; echo; } >> "$(dirname "$0")/args"
`

// What a table or a server says (a title, an artist, an MD5, links, a
// table's URL, a refused host's name) reaches the CLI as an option's
// value or after "--", never as an argument the CLI could read as an
// option.
func TestSaidValuesAreNoOptions(t *testing.T) {
	a := fakeApp(t)
	bin := t.TempDir()
	if err := os.WriteFile(filepath.Join(bin, "bmseeker"), []byte(wordsCLI), 0o755); err != nil {
		t.Fatal(err)
	}
	t.Setenv("BMSEEKER_CLI", filepath.Join(bin, "bmseeker"))
	cliOnce = sync.Once{}
	m := missing{Title: "-ERROR", Artist: "--reinstall", MD5: "--third-party", URL: "--config=/x.json", URLDiff: "-h"}
	said := []string{m.Title, m.Artist, m.MD5, m.URL, m.URLDiff, "-x.evil.example", "--help"}
	a.refused = []string{"-x.evil.example"}
	a.missingFrom = "--help"
	a.download([]missing{m})
	until(t, func() bool { return len(a.busy) == 0 })
	args, _ := os.ReadFile(filepath.Join(bin, "args"))
	cmds := strings.Split(strings.TrimSpace(string(args)), "\n\n")
	words := strings.Split(cmds[0], "\n")
	end := slices.Index(words, "--")
	if len(cmds) != 1 || end < 0 || !slices.Equal(words[end+1:], []string{"--help"}) {
		t.Fatalf("not one command, its table after \"--\": %q", cmds)
	}
	if !slices.Contains(words[:end], "--skip-host=-x.evil.example") || !slices.Contains(words[:end], "--md5=--third-party") {
		t.Errorf("values not attached to their options: %q", words)
	}
	// the Link tab's command
	link := installArgs("/lib", m.Title, m.Artist, m.MD5, []string{"--own-links"}, m.URL)
	at := slices.Index(link, "--")
	if at < 0 || !slices.Equal(link[at+1:], []string{m.URL}) || !slices.Contains(link[:at], "--title=-ERROR") {
		t.Fatalf("link %q", link)
	}
	for _, w := range append(words[:end:end], link[:at]...) {
		if slices.Contains(said, w) {
			t.Errorf("%q is an argument of its own: %q, %q", w, words, link)
		}
	}
}

// The Browser tab opens web links only: a table may say anything where a
// link goes.
func TestOnlyWebLinksOpen(t *testing.T) {
	a := fakeApp(t)
	for _, l := range []string{"file://evil.example/share/x.html", "javascript:alert(1)", "search-ms:query=x", "https:///x", "--help", ""} {
		a.log = nil
		a.openPage(page{Title: "Song", Link: l})
		if len(a.log) != 1 || !strings.Contains(a.log[0], "not a web link") {
			t.Errorf("%q: log %q", l, a.log)
		}
	}
	for _, l := range []string{"https://1drv.ms/b", "HTTP://example.com/a?b#c"} {
		if !a.browsable(page{Link: l}) {
			t.Errorf("%q is a web link", l)
		}
	}
}

// A page's download is kept under a name of the window's making, beside
// what the downloads folder holds, whatever the page names it; the name
// goes to the CLI as a value. The script its window runs is the CLI's.
func TestPageDownloads(t *testing.T) {
	a := fakeApp(t)
	dir := t.TempDir()
	held := map[string]string{"scan.json": "[]", "waiting.bms": "#TITLE A", "page-1.part": "x"}
	for n, body := range held {
		if err := os.WriteFile(filepath.Join(dir, n), []byte(body), 0o644); err != nil {
			t.Fatal(err)
		}
	}
	one, err := savePath(dir)
	if err != nil {
		t.Fatal(err)
	}
	two, err := savePath(dir)
	if err != nil {
		t.Fatal(err)
	}
	for _, p := range []string{one, two} {
		st, err := os.Stat(p)
		if err != nil || filepath.Dir(p) != dir || held[filepath.Base(p)] != "" || st.Mode().Perm() != 0o600 {
			t.Errorf("%s: %v, %v", p, st, err)
		}
	}
	if one == two {
		t.Errorf("one name for two downloads: %s", one)
	}
	for n, body := range held {
		if got, _ := os.ReadFile(filepath.Join(dir, n)); string(got) != body {
			t.Errorf("%s is now %q", n, got)
		}
	}
	args := importArgs("-lib", "--reinstall", one)
	end := slices.Index(args, "--")
	if end < 0 || !slices.Equal(args[end+1:], []string{one}) ||
		!slices.Contains(args[:end], "--name=--reinstall") || !slices.Contains(args[:end], "--library=-lib") {
		t.Errorf("args %q", args)
	}
	if got := a.pageScript(); !strings.Contains(got, "location.hostname") {
		t.Errorf("script %q, log %q", got, a.log)
	}
}

// The bundled CLI is run from the cache folder only as the bytes it was
// built with, kept for its user alone; with no cache folder (no home),
// from nowhere: not from a folder others can write.
func TestBundledCLI(t *testing.T) {
	home := t.TempDir()
	t.Setenv("HOME", home)
	t.Setenv("XDG_CACHE_HOME", "")
	data := []byte("#!/bin/sh\necho the CLI\n")
	file, err := bundledPath("bmseeker", data)
	if err != nil {
		t.Fatal(err)
	}
	st, err := os.Stat(file)
	if err != nil || !strings.HasPrefix(file, home) || st.Mode().Perm() != 0o700 {
		t.Fatalf("%s: %v, %v", file, st, err)
	}
	if dir, _ := os.Stat(filepath.Dir(file)); dir.Mode().Perm() != 0o700 {
		t.Errorf("its folder: %v", dir.Mode())
	}
	// another program there, of the same size: not run
	other := []byte("#!/bin/sh\necho another\n")
	if len(other) != len(data) {
		t.Fatal("the test's programs differ in size")
	}
	if err := os.WriteFile(file, other, 0o755); err != nil {
		t.Fatal(err)
	}
	if again, err := bundledPath("bmseeker", data); err != nil || again != file {
		t.Fatal(again, err)
	}
	if got, _ := os.ReadFile(file); string(got) != string(data) {
		t.Errorf("run as %q", got)
	}
	if left, _ := filepath.Glob(filepath.Join(filepath.Dir(file), "*.tmp")); len(left) != 0 {
		t.Errorf("left behind: %q", left)
	}
	if hashOf(file) != sha256.Sum256(data) || hashOf(file+".none") == sha256.Sum256(nil) {
		t.Error("hashOf")
	}
	t.Setenv("HOME", "")
	if p, err := bundledPath("bmseeker", data); err == nil {
		t.Errorf("without a home it is kept in %s", p)
	}
}

func TestCLIWords(t *testing.T) {
	for _, a := range [][]string{{"link", "get"}, {"--help"}, {"--json", "table"}, {"config", "show"}} {
		if !isCLI(a) {
			t.Errorf("%v is the CLI", a)
		}
	}
	for _, a := range [][]string{nil, {"/home/me/bms"}, {"Song"}} {
		if isCLI(a) {
			t.Errorf("%v is the window", a)
		}
	}
}
