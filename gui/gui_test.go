//go:build !windows && !js

package main

import (
	"crypto/sha256"
	"os"
	"path/filepath"
	"slices"
	"strings"
	"sync"
	"testing"
	"time"

	"github.com/egoist/mygo/ui"
)

// fakeCLI answers as the CLI would: `table missing` lists two charts,
// `library install` brings the first in and finds a page for the second.
// It notes the arguments it was given.
const fakeCLI = `#!/bin/sh
echo "$*" >> "$(dirname "$0")/args"
case "$*" in
  *"table missing"*)
    echo '{"level":"1","label":"★1","title":"Song A","artist":"x","md5":"a","url":"https://d/a.zip","url_diff":""}'
    echo '{"level":"2","label":"★2","title":"Song B","artist":"y","md5":"b","url":"https://1drv.ms/b","url_diff":""}'
    echo "2 of 9 charts missing" >&2 ;;
  *"link page-script"*) echo '{"script":"/* by the library */"}' ;;
  *"Song A"*) echo '{"title":"Song A","state":"added","path":"/lib/Song A","files":3}' ;;
  *"Song B"*) echo '{"title":"Song B","state":"page","link":"https://1drv.ms/b"}' ;;
  *"Song C"*)
    echo "bmseeker: third-party sources are on (...)" >&2
    echo '{"refused":"api.bmssearch.net","code":"status","status":403,"error":"api.bmssearch.net refused BMSeeker (HTTP 403): skipped for the rest of this run"}'
    echo '{"title":"Song C","state":"failed","code":"no_link","error":"no link"}' ;;
esac
`

func until(t *testing.T, ok func() bool) {
	t.Helper()
	for i := 0; i < 200 && !ok(); i++ {
		time.Sleep(10 * time.Millisecond)
	}
	if !ok() {
		t.Fatal("timed out")
	}
}

func TestTablesToBrowser(t *testing.T) {
	dir := t.TempDir()
	t.Setenv("XDG_CONFIG_HOME", dir)
	t.Setenv("HOME", dir)
	cli := filepath.Join(dir, "bmseeker")
	if err := os.WriteFile(cli, []byte(fakeCLI), 0o755); err != nil {
		t.Fatal(err)
	}
	t.Setenv("BMSEEKER_CLI", cli)
	cliOnce = sync.Once{}

	a := newApp(dir)
	tt := ui.NewTester(a.view, 980, 720)
	if err := tt.Click("Missing charts"); err != nil {
		t.Fatal(err)
	}
	until(t, func() bool { return len(a.missing) == 2 && len(a.busy) == 0 })
	tt.Frame()
	if !tt.HasText("Song A") || !tt.HasText("2 missing") {
		t.Fatalf("texts %q", tt.Texts())
	}
	if err := tt.Click("Download all shown"); err != nil {
		t.Fatal(err)
	}
	until(t, func() bool { return len(a.busy) == 0 && len(a.pages) == 1 })
	if len(a.missing) != 1 || a.missing[0].Title != "Song B" {
		t.Fatalf("missing %v", a.missing)
	}
	tt.Frame()
	if !tt.HasText("Browser (1)") {
		t.Fatalf("texts %q", tt.Texts())
	}
	joined := strings.Join(a.log, "\n")
	if !strings.Contains(joined, "2 of 9 charts missing") || !strings.Contains(joined, "Song A: added") {
		t.Fatalf("log %q", joined)
	}
	// the third-party setting goes to the CLI
	a.settings.ThirdParty = true
	if err := a.cli([]string{"library", "pending", "-L", "/lib"}, nil); err != nil {
		t.Fatal(err)
	}
	args, _ := os.ReadFile(filepath.Join(dir, "args"))
	if !strings.Contains(string(args), "--json --third-party library pending") ||
		strings.Contains(strings.SplitN(string(args), "\n", 2)[0], "--third-party") {
		t.Fatalf("args %q", args)
	}
	// every tab draws
	for _, tab := range []string{"Link", "Waiting", "Settings", "Browser (1)"} {
		if err := tt.Click(tab); err != nil {
			t.Fatal(tab, err)
		}
		tt.Frame()
	}
}

// fakeApp is a window on the fake CLI, in a home of its own.
func fakeApp(t *testing.T) (*app, string) {
	dir := t.TempDir()
	t.Setenv("XDG_CONFIG_HOME", dir)
	t.Setenv("HOME", dir)
	// (beside the config folder's bmseeker/, not in its place)
	bin := filepath.Join(dir, "bin")
	if err := os.Mkdir(bin, 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(bin, "bmseeker"), []byte(fakeCLI), 0o755); err != nil {
		t.Fatal(err)
	}
	t.Setenv("BMSEEKER_CLI", filepath.Join(bin, "bmseeker"))
	cliOnce = sync.Once{}
	return newApp(dir), bin
}

func TestThirdPartyAsksFirst(t *testing.T) {
	a, _ := fakeApp(t)
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

func TestRefusedHostsSkipped(t *testing.T) {
	a, dir := fakeApp(t)
	a.settings.ThirdParty = true
	got := 0
	err := a.cli([]string{"library", "install", "--title", "Song C"}, func(Record) { got++ })
	if err != nil {
		t.Fatal(err)
	}
	// the refusal is logged, not handed on; the CLI's third-party note left out
	joined := strings.Join(a.log, "\n")
	if got != 1 || !strings.Contains(joined, "api.bmssearch.net refused BMSeeker (HTTP 403)") ||
		strings.Contains(joined, "third-party sources are on") {
		t.Fatalf("records %d, log %q", got, joined)
	}
	// later runs skip it
	if err := a.cli([]string{"library", "pending", "-L", "/lib"}, nil); err != nil {
		t.Fatal(err)
	}
	args, _ := os.ReadFile(filepath.Join(dir, "args"))
	lines := strings.Split(strings.TrimSpace(string(args)), "\n")
	if strings.Contains(lines[0], "--skip-host") ||
		!strings.Contains(lines[1], "--json --third-party --skip-host=api.bmssearch.net library pending") {
		t.Fatalf("args %q", args)
	}
}

// wordsCLI notes each argument it was given on a line of its own, and an
// empty line after a command's last.
const wordsCLI = `#!/bin/sh
{ for a in "$@"; do printf '%s\n' "$a"; done; echo; } >> "$(dirname "$0")/args"
`

// What a table or a server says (a title, an artist, an MD5, links, a
// refused host's name) reaches the CLI as an option's value or after "--",
// never as an argument the CLI could read as an option.
func TestSaidValuesAreNoOptions(t *testing.T) {
	a, bin := fakeApp(t)
	if err := os.WriteFile(filepath.Join(bin, "bmseeker"), []byte(wordsCLI), 0o755); err != nil {
		t.Fatal(err)
	}
	m := missing{Title: "-ERROR", Artist: "--reinstall", MD5: "--third-party", URL: "--config=/x.json", URLDiff: "-h"}
	said := []string{m.Title, m.Artist, m.MD5, m.URL, m.URLDiff, "-x.evil.example"}
	a.refused = []string{"-x.evil.example"}
	a.download([]missing{m})
	until(t, func() bool { return len(a.busy) == 0 })
	args, _ := os.ReadFile(filepath.Join(bin, "args"))
	var links []string
	for _, cmd := range strings.Split(strings.TrimSpace(string(args)), "\n\n") {
		words := strings.Split(cmd, "\n")
		end := slices.Index(words, "--")
		if end < 0 {
			t.Fatalf("no \"--\" before the links: %q", words)
		}
		for _, w := range words[:end] {
			if slices.Contains(said, w) {
				t.Errorf("%q is an argument of its own: %q", w, words)
			}
		}
		if !slices.Contains(words[:end], "--skip-host=-x.evil.example") || !slices.Contains(words[:end], "--title=-ERROR") {
			t.Errorf("values not attached to their options: %q", words)
		}
		links = append(links, words[end+1:]...)
	}
	if !slices.Equal(links, []string{m.URL, m.URLDiff}) {
		t.Fatalf("links %q", links)
	}
}

// The Browser tab opens web links only: a table may say anything where a
// link goes.
func TestOnlyWebLinksOpen(t *testing.T) {
	a, _ := fakeApp(t)
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
	a, _ := fakeApp(t)
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
	if got := a.pageScript(); got != "/* by the library */" {
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
	for _, a := range [][]string{{"link", "get"}, {"--help"}, {"--json", "table"}} {
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
