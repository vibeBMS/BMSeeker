//go:build !js

package main

import (
	"bufio"
	"crypto/sha256"
	"embed"
	"encoding/hex"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"os"
	"os/exec"
	"path/filepath"
	"runtime"
	"strings"
	"sync"
)

// The CLI the GUI runs, put in bin/ by the build (build.sh) before go build.
// A build without it runs `bmseeker` from the PATH (or $BMSEEKER_CLI).
//
//go:embed all:bin
var bundled embed.FS

// cliWords are the first arguments that make this binary the CLI.
var cliWords = map[string]bool{
	"link": true, "song": true, "table": true, "library": true, "event": true,
	"help": true, "--help": true, "-h": true, "--version": true, "-V": true,
	"--json": true, "--config": true,
}

// isCLI reports whether args (without the program) are a CLI command.
func isCLI(args []string) bool {
	return len(args) > 0 && cliWords[args[0]]
}

var (
	cliOnce sync.Once
	cliFile string
	cliErr  error
)

// cliPath is the CLI's executable: $BMSEEKER_CLI, else the bundled one
// (written once into the user's cache folder), else `bmseeker` on the PATH.
func cliPath() (string, error) {
	cliOnce.Do(func() {
		if p := os.Getenv("BMSEEKER_CLI"); p != "" {
			cliFile = p
			return
		}
		name := "bmseeker"
		if runtime.GOOS == "windows" {
			name += ".exe"
		}
		data, err := bundled.ReadFile("bin/" + name)
		if err != nil {
			cliFile, cliErr = exec.LookPath(name)
			return
		}
		sum := sha256.Sum256(data)
		cache, err := os.UserCacheDir()
		if err != nil {
			cache = os.TempDir()
		}
		dir := filepath.Join(cache, "bmseeker", "cli-"+hex.EncodeToString(sum[:6]))
		file := filepath.Join(dir, name)
		if st, err := os.Stat(file); err == nil && st.Size() == int64(len(data)) {
			cliFile = file
			return
		}
		if err := os.MkdirAll(dir, 0o755); err != nil {
			cliErr = err
			return
		}
		tmp := file + ".tmp"
		if err := os.WriteFile(tmp, data, 0o755); err != nil {
			cliErr = err
			return
		}
		if err := os.Rename(tmp, file); err != nil {
			cliErr = err
			return
		}
		cliFile = file
	})
	return cliFile, cliErr
}

// passThrough runs the CLI with args, on this process's stdin, stdout and
// stderr, and exits with its code.
func passThrough(args []string) {
	p, err := cliPath()
	if err != nil {
		fmt.Fprintln(os.Stderr, "bmseeker:", err)
		os.Exit(2)
	}
	cmd := exec.Command(p, args...)
	cmd.Stdin, cmd.Stdout, cmd.Stderr = os.Stdin, os.Stdout, os.Stderr
	err = cmd.Run()
	var exit *exec.ExitError
	switch {
	case errors.As(err, &exit):
		os.Exit(exit.ExitCode())
	case err != nil:
		fmt.Fprintln(os.Stderr, "bmseeker:", err)
		os.Exit(2)
	}
	os.Exit(0)
}

// runCLI runs `bmseeker --json args...`: each record it writes goes to
// onRecord and each note (stderr) to onNote, as they come, on the
// goroutine that reads them; then it returns how the command ended.
func runCLI(args []string, onRecord func(Record), onNote func(string)) error {
	p, err := cliPath()
	if err != nil {
		return err
	}
	cmd := exec.Command(p, append([]string{"--json"}, args...)...)
	hideConsole(cmd)
	out, err := cmd.StdoutPipe()
	if err != nil {
		return err
	}
	errs, err := cmd.StderrPipe()
	if err != nil {
		return err
	}
	if err := cmd.Start(); err != nil {
		return err
	}
	var wg sync.WaitGroup
	wg.Add(2)
	var last string
	go func() {
		defer wg.Done()
		lines(errs, func(l string) {
			last = l
			if onNote != nil {
				onNote(l)
			}
		})
	}()
	go func() {
		defer wg.Done()
		lines(out, func(l string) {
			var r Record
			if json.Unmarshal([]byte(l), &r) == nil && onRecord != nil {
				onRecord(r)
			}
		})
	}()
	wg.Wait()
	if err := cmd.Wait(); err != nil {
		if last != "" {
			return errors.New(strings.TrimPrefix(last, "bmseeker: "))
		}
		return err
	}
	return nil
}

func lines(r io.Reader, f func(string)) {
	s := bufio.NewScanner(r)
	s.Buffer(make([]byte, 64*1024), 16*1024*1024)
	for s.Scan() {
		if l := strings.TrimSpace(s.Text()); l != "" {
			f(l)
		}
	}
}
