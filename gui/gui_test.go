//go:build !windows && !js

package main

import (
	"os"
	"path/filepath"
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
		!strings.Contains(lines[1], "--json --third-party --skip-host api.bmssearch.net library pending") {
		t.Fatalf("args %q", args)
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
