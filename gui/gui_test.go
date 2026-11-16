//go:build !windows

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
