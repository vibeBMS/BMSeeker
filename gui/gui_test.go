//go:build !windows && !js

package main

import (
	"crypto/sha256"
	"encoding/json"
	"fmt"
	"io"
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
	Stdin string   `json:"stdin"`
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

// replay writes what the CLI wrote for args (and what it read on stdin)
// in the fixture file's run of them, and ends as it did. A command line it
// has no run of fails.
func replay(file string, args []string) int {
	data, err := os.ReadFile(file)
	if err != nil {
		fmt.Fprintln(os.Stderr, "bmseeker:", err)
		return 2
	}
	stdin, err := io.ReadAll(os.Stdin)
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
		if !slices.Equal(r.Args, args) || r.Stdin != string(stdin) {
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
	fmt.Fprintf(os.Stderr, "bmseeker: the fixture has no run of %q (stdin %q)\n", args, stdin)
	return 2
}

// posted are the changes of the state an app (fakeApp) posted for the
// window's goroutine; the test's goroutine runs them (pump).
var posted = map[*app]*struct {
	sync.Mutex
	fs []func()
}{}

// pump runs what a posted so far, as the window would.
func pump(a *app) {
	q := posted[a]
	q.Lock()
	fs := q.fs
	q.fs = nil
	q.Unlock()
	for _, f := range fs {
		f()
	}
}

// until waits for ok, running what a posts meanwhile.
func until(t *testing.T, a *app, ok func() bool) {
	t.Helper()
	for i := 0; i < 500; i++ {
		pump(a)
		if ok() {
			return
		}
		time.Sleep(10 * time.Millisecond)
	}
	t.Fatal("timed out")
}

// fakeApp is a window on the library /lib, in a home of its own, whose CLI
// answers as the fixture says; its settings taken as read (a job does not
// read them first).
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
	a := newApp("/lib")
	a.settings.Read = true
	q := &struct {
		sync.Mutex
		fs []func()
	}{}
	posted[a] = q
	a.post = func(f func()) {
		q.Lock()
		q.fs = append(q.fs, f)
		q.Unlock()
	}
	return a
}

// fakeCLI makes the CLI the shell script script, in a folder of its own,
// which it returns.
func fakeCLI(t *testing.T, script string) string {
	bin := t.TempDir()
	if err := os.WriteFile(filepath.Join(bin, "bmseeker"), []byte(script), 0o755); err != nil {
		t.Fatal(err)
	}
	t.Setenv("BMSEEKER_CLI", filepath.Join(bin, "bmseeker"))
	cliOnce = sync.Once{}
	return bin
}

// The window on the fixture's runs: its settings, a table's missing
// charts, those shown downloaded in one run (one into the library, one a
// page for the Browser tab, one whose source refused, one the table names
// by its SHA-256 alone, a separate chart whose song came and whose own
// link is dead, the two charts of a pack, one by its SHA-256 without a
// link, one the table no longer lists): those the library has now leave
// the list, the others stay; and a later run that skips the host that
// refused, with third-party sources on, whose notice it does not repeat.
func TestTablesToBrowser(t *testing.T) {
	a := fakeApp(t)
	a.readSettings()
	until(t, a, func() bool { return len(a.busy) == 0 })
	if !slices.Equal(a.settings.Tables, []string{"https://t.example/table.json"}) || a.table != a.settings.Tables[0] ||
		a.settings.downloads() != filepath.Join("/lib", ".bmseeker") || a.configThirdParty {
		t.Fatalf("settings %+v, table %q, log %q", a.settings, a.table, a.log)
	}
	tt := ui.NewTester(a.view, 980, 720)
	if err := tt.Click("Missing charts"); err != nil {
		t.Fatal(err)
	}
	until(t, a, func() bool { return len(a.missing) == 8 && len(a.busy) == 0 })
	tt.Frame()
	if !tt.HasText("Song A") || !tt.HasText("8 missing") || a.missing[3].MD5 != "" || a.missing[3].SHA256 == "" {
		t.Fatalf("texts %q", tt.Texts())
	}
	// (a row of an older listing: the table has it no more)
	a.missing = append(a.missing, missing{Title: "Gone", MD5: strings.Repeat("f", 32)})
	if err := tt.Click("Download all shown"); err != nil {
		t.Fatal(err)
	}
	until(t, a, func() bool { return len(a.busy) == 0 && len(a.pages) == 1 })
	var left []string
	for _, m := range a.missing {
		left = append(left, m.Title)
	}
	if !slices.Equal(left, []string{"Song B", "Song C", "Song A [EX]", "Song G", "Gone"}) {
		t.Fatalf("missing %q, log %q", left, a.log)
	}
	tt.Frame()
	if !tt.HasText("Browser (1)") {
		t.Fatalf("texts %q", tt.Texts())
	}
	joined := strings.Join(a.log, "\n")
	for _, said := range []string{"8 of 8 charts missing", "(1/9) Gone: the table lists no such chart",
		"Song A: added /lib/Song A", "Song D: added /lib/Song D", "Pack [1]: added /lib/Pack", "Pack [2]: owned /lib/Pack",
		"Song B: needs a browser: https://b.example/b", "bmssearch.net refused BMSeeker (HTTP 403)", "Song C: no link",
		"Song G: no link", "(9/9) "} {
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
	if err := a.cli([]string{"library", "pending", "--library=/lib"}, "", func(Record) { got++ }); err != nil {
		t.Fatal(err)
	}
	pump(a)
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
// empty line after a command's last; and what it read on stdin.
const wordsCLI = `#!/bin/sh
{ for a in "$@"; do printf '%s\n' "$a"; done; echo; } >> "$(dirname "$0")/args"
cat >> "$(dirname "$0")/stdin"
`

// What a table or a server says (a title, an artist, an MD5, links, a
// table's URL, a refused host's name) reaches the CLI as an option's
// value, after "--" or on stdin, never as an argument the CLI could read
// as an option.
func TestSaidValuesAreNoOptions(t *testing.T) {
	a := fakeApp(t)
	bin := fakeCLI(t, wordsCLI)
	m := missing{Title: "-ERROR", Artist: "--reinstall", MD5: "--third-party", URL: "--config=/x.json", URLDiff: "-h"}
	bySHA := missing{Title: "-V", SHA256: "--name-folders"}
	said := []string{m.Title, m.Artist, m.MD5, m.URL, m.URLDiff, bySHA.Title, bySHA.SHA256, "-x.evil.example", "--help"}
	a.refused = []string{"-x.evil.example"}
	a.missingFrom = "--help"
	a.download([]missing{m, bySHA})
	until(t, a, func() bool { return len(a.busy) == 0 })
	args, _ := os.ReadFile(filepath.Join(bin, "args"))
	cmds := strings.Split(strings.TrimSpace(string(args)), "\n\n")
	words := strings.Split(cmds[0], "\n")
	end := slices.Index(words, "--")
	if len(cmds) != 1 || end < 0 || !slices.Equal(words[end+1:], []string{"--help"}) {
		t.Fatalf("not one command, its table after \"--\": %q", cmds)
	}
	if !slices.Contains(words[:end], "--skip-host=-x.evil.example") || !slices.Contains(words[:end], "--chosen=-") {
		t.Errorf("values not attached to their options: %q", words)
	}
	if stdin, _ := os.ReadFile(filepath.Join(bin, "stdin")); string(stdin) != "--third-party\n--name-folders\n" {
		t.Errorf("stdin %q", stdin)
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

// A download of thousands of charts is one command line of the size of
// one's: the charts go on stdin (a command line has a limit: Windows'
// 32,767 characters are some 800 MD5s).
func TestManyChartsOneCommandLine(t *testing.T) {
	a := fakeApp(t)
	bin := fakeCLI(t, wordsCLI)
	a.missingFrom = "https://t.example/table.json"
	var ms []missing
	for i := 0; i < 3000; i++ {
		ms = append(ms, missing{Title: fmt.Sprint(i), MD5: fmt.Sprintf("%032x", i)})
	}
	a.download(ms[:1])
	until(t, a, func() bool { return len(a.busy) == 0 })
	a.download(ms)
	until(t, a, func() bool { return len(a.busy) == 0 })
	args, _ := os.ReadFile(filepath.Join(bin, "args"))
	cmds := strings.Split(strings.TrimSpace(string(args)), "\n\n")
	if len(cmds) != 2 || cmds[0] != cmds[1] {
		t.Fatalf("command lines %q", cmds)
	}
	stdin, _ := os.ReadFile(filepath.Join(bin, "stdin"))
	if lines := strings.Split(strings.TrimSpace(string(stdin)), "\n"); len(lines) != 3001 || lines[3000] != ms[2999].MD5 {
		t.Fatalf("%d lines on stdin", len(lines))
	}
}

// A CLI that dies in a run is told as that (how it ended, its last note),
// not as its last note; what it said came in before stays done.
func TestTheCLIDiesInARun(t *testing.T) {
	a := fakeApp(t)
	md5 := strings.Repeat("a", 32)
	fakeCLI(t, `#!/bin/sh
echo "2 of 2 charts missing" >&2
echo '{"title":"A","md5":"`+md5+`","state":"added","path":"/lib/A","have":true}'
kill -9 $$
`)
	a.missing = []missing{{Title: "A", MD5: md5}, {Title: "B", MD5: strings.Repeat("b", 32)}}
	a.download(a.missing)
	until(t, a, func() bool { return len(a.busy) == 0 })
	joined := strings.Join(a.log, "\n")
	if len(a.missing) != 1 || !strings.Contains(joined, "Downloading: the CLI stopped (signal: killed); it last said: 2 of 2 charts missing") {
		t.Fatalf("missing %v, log %q", a.missing, joined)
	}
}

// A line too long to read ends the run with an error, not a job that
// never ends.
func TestALineTooLong(t *testing.T) {
	fakeCLI(t, "#!/bin/sh\nexec head -c 17000000 /dev/zero\n")
	done := make(chan error, 1)
	go func() { done <- runCLI([]string{"config", "show"}, "", nil, nil) }()
	select {
	case err := <-done:
		if err == nil || !strings.Contains(err.Error(), "too long to read") {
			t.Fatal(err)
		}
	case <-time.After(20 * time.Second):
		t.Fatal("the run did not end")
	}
}

// The window quitting ends the CLI's runs.
func TestQuittingEndsTheCLI(t *testing.T) {
	fakeCLI(t, "#!/bin/sh\nexec sleep 60\n")
	done := make(chan error, 1)
	go func() { done <- runCLI([]string{"table", "missing"}, "", nil, nil) }()
	for i := 0; i < 500; i++ {
		running.Lock()
		n := len(running.cmds)
		running.Unlock()
		if n == 1 {
			break
		}
		time.Sleep(10 * time.Millisecond)
	}
	stopCLIs()
	select {
	case err := <-done:
		if err == nil || !strings.Contains(err.Error(), "killed") {
			t.Fatal(err)
		}
	case <-time.After(10 * time.Second):
		t.Fatal("the run did not end")
	}
}

// Settings that could not be read are said, and read again when a job
// starts; what they say of the downloads folder is taken as the CLI takes
// it.
func TestSettingsReadAgain(t *testing.T) {
	a := fakeApp(t)
	a.settings.Read = false
	bin := fakeCLI(t, "#!/bin/sh\necho 'bmseeker: config.json: no such setting' >&2\nexit 2\n")
	a.readSettings()
	until(t, a, func() bool { return len(a.busy) == 0 })
	if a.settings.Read || !strings.Contains(strings.Join(a.log, "\n"), "Reading the settings: config.json: no such setting") {
		t.Fatalf("settings %+v, log %q", a.settings, a.log)
	}
	if err := os.WriteFile(filepath.Join(bin, "bmseeker"), []byte(`#!/bin/sh
echo '{"config":{"tables":["https://t.example/t.json"],"downloads":"/elsewhere/dl","third_party":false}}'
`), 0o755); err != nil {
		t.Fatal(err)
	}
	a.job("Something", func() error { return nil })
	until(t, a, func() bool { return len(a.busy) == 0 })
	if !a.settings.Read || a.table != "https://t.example/t.json" || a.settings.downloads() != "/elsewhere/dl" {
		t.Fatalf("settings %+v, table %q, log %q", a.settings, a.table, a.log)
	}
	a.settings.Downloads = ""
	if a.settings.downloads() != "/lib" {
		t.Errorf("downloads %q", a.settings.downloads())
	}
}

// Third-party sources the settings file turns on (the window never asked)
// are told once a session.
func TestThirdPartyFromTheSettingsFile(t *testing.T) {
	a := fakeApp(t)
	fakeCLI(t, "#!/bin/sh\necho '{\"notice\":\"third_party\",\"text\":\"third-party sources are on\"}'\n")
	for i := 0; i < 2; i++ {
		if err := a.cli([]string{"library", "pending", "--library=/lib"}, "", nil); err != nil {
			t.Fatal(err)
		}
	}
	pump(a)
	if n := strings.Count(strings.Join(a.log, "\n"), "third-party sources are on"); n != 1 {
		t.Fatalf("told %d times: %q", n, a.log)
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
	for _, a := range [][]string{{"link", "get"}, {"--help"}, {"--json", "table"}, {"config", "show"}, {"licenses"}} {
		if !isCLI(a) {
			t.Errorf("%v is the CLI", a)
		}
	}
	for _, a := range [][]string{nil, {"/home/me/bms"}, {"Song"}, {"./licenses"}} {
		if isCLI(a) {
			t.Errorf("%v is the window", a)
		}
	}
}

// Settings' Licences: the CLI's `licenses` read once, its text shown in a
// text area that keeps drawing frames quickly with a text of the real
// one's size (some 330 KB, 6,000 lines and more), scrolled too; Back
// leads to the settings again.
func TestLicences(t *testing.T) {
	a := fakeApp(t)
	var text strings.Builder
	text.WriteString("MIT License\n\nCopyright (c) 2026 vibeBMS contributors\n")
	for i := 0; text.Len() < 340_000; i++ {
		fmt.Fprintf(&text, "- `crate-%d` (MIT OR Apache-2.0): https://example.com/a/rather/long/address/of/crate-%d\n", i, i)
	}
	text.WriteString("- `github.com/egoist/mygo`: https://pkg.go.dev/github.com/egoist/mygo\n")
	record, err := json.Marshal(map[string]any{"licenses": []string{"LICENSE-MIT"}, "text": text.String()})
	if err != nil {
		t.Fatal(err)
	}
	bin := fakeCLI(t, wordsCLI+`[ "$2" = licenses ] && cat "$(dirname "$0")/record"`+"\n")
	if err := os.WriteFile(filepath.Join(bin, "record"), append(record, '\n'), 0o644); err != nil {
		t.Fatal(err)
	}
	tt := ui.NewTester(a.view, 980, 720)
	if err := tt.Click("Settings"); err != nil {
		t.Fatal(err)
	}
	tt.Frame()
	if err := tt.Click("Licences"); err != nil {
		t.Fatal(err)
	}
	until(t, a, func() bool { return len(a.busy) == 0 })
	start := time.Now()
	tt.Frame()
	opened := time.Since(start)
	area, ok := tt.AccessByRole(ui.RoleTextField, "Licences")
	if !ok || area.Value != text.String() || a.licences != text.String() {
		t.Fatalf("area %v (%d bytes), log %q", ok, len(area.Value), a.log)
	}
	// scrolled through, a frame at a time: what it shows moves
	top := sha256.Sum256(tt.Image().Pix)
	start = time.Now()
	const frames = 50
	for i := 0; i < frames; i++ {
		tt.Scroll(area.Bounds.X+area.Bounds.W/2, area.Bounds.Y+area.Bounds.H/2, 0, 2000)
		tt.Frame()
	}
	scrolled := time.Since(start) / frames
	if sha256.Sum256(tt.Image().Pix) == top {
		t.Error("scrolling shows the same")
	}
	t.Logf("opened in %v, a frame scrolling %v", opened, scrolled)
	if opened > 5*time.Second || scrolled > time.Second {
		t.Errorf("opened in %v, a frame scrolling %v", opened, scrolled)
	}
	// read once a session: Back, and Licences again, runs no CLI
	if err := tt.Click("Back"); err != nil {
		t.Fatal(err)
	}
	tt.Frame()
	if !tt.HasText("Difficulty tables") {
		t.Fatalf("texts %q", tt.Texts())
	}
	if err := tt.Click("Licences"); err != nil {
		t.Fatal(err)
	}
	tt.Frame()
	args, _ := os.ReadFile(filepath.Join(bin, "args"))
	if cmds := strings.Split(strings.TrimSpace(string(args)), "\n\n"); len(cmds) != 1 || cmds[0] != "--json\nlicenses" {
		t.Fatalf("command lines %q", cmds)
	}
	if _, ok := tt.AccessByRole(ui.RoleTextField, "Licences"); !ok {
		t.Fatal("not shown again")
	}
}
