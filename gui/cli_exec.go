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
	"link": true, "song": true, "table": true, "library": true, "event": true, "config": true,
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
// (kept in the user's cache folder: bundledPath), else `bmseeker` on the
// PATH.
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
		cliFile, cliErr = bundledPath(name, data)
	})
	return cliFile, cliErr
}

// bundledPath is the bundled CLI (data) as the file name in a folder of the
// user's cache folder named after its hash. Without a cache folder (no
// home) there is none: the temp folder is everyone's, and a program found
// there would be run as this user.
func bundledPath(name string, data []byte) (string, error) {
	cache, err := os.UserCacheDir()
	if err != nil {
		return "", fmt.Errorf("no cache folder to keep the bundled bmseeker in (%v); $BMSEEKER_CLI names one to run instead", err)
	}
	sum := sha256.Sum256(data)
	return placed(filepath.Join(cache, "bmseeker", "cli-"+hex.EncodeToString(sum[:6])), name, data, sum)
}

// placed puts data (whose SHA-256 is sum) in dir as the program name, its
// user's alone, and returns its path. A file already there is run only
// when it is the same byte for byte, by its hash: its size says nothing.
func placed(dir, name string, data []byte, sum [sha256.Size]byte) (string, error) {
	file := filepath.Join(dir, name)
	if hashOf(file) == sum {
		return file, nil
	}
	if err := os.MkdirAll(dir, 0o700); err != nil {
		return "", err
	}
	// (a new file of a name of its own, 0600 until it is whole)
	tmp, err := os.CreateTemp(dir, name+".*.tmp")
	if err != nil {
		return "", err
	}
	defer os.Remove(tmp.Name())
	_, err = tmp.Write(data)
	if cerr := tmp.Close(); err == nil {
		err = cerr
	}
	if err == nil {
		err = os.Chmod(tmp.Name(), 0o700)
	}
	if err == nil {
		err = os.Rename(tmp.Name(), file)
	}
	if err != nil {
		return "", err
	}
	return file, nil
}

// hashOf is the SHA-256 of the file at path (zero when it cannot be read).
func hashOf(path string) (sum [sha256.Size]byte) {
	f, err := os.Open(path)
	if err != nil {
		return sum
	}
	defer f.Close()
	h := sha256.New()
	if _, err := io.Copy(h, f); err != nil {
		return sum
	}
	h.Sum(sum[:0])
	return sum
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

// The CLI runs going on: ended with the window (stopCLIs).
var running = struct {
	sync.Mutex
	cmds map[*exec.Cmd]bool
}{cmds: map[*exec.Cmd]bool{}}

// stopCLIs ends the CLI runs going on: the window quits, and what it
// started (a table's downloads) goes with it.
func stopCLIs() {
	running.Lock()
	defer running.Unlock()
	for c := range running.cmds {
		_ = c.Process.Kill()
	}
}

// runCLI runs `bmseeker --json args...`, stdin what it reads for "-": each
// record it writes goes to onRecord and each note (stderr) to onNote, as
// they come, on the goroutine that reads them; then it returns how the
// command ended. A line too long to read ends it.
func runCLI(args []string, stdin string, onRecord func(Record), onNote func(string)) error {
	p, err := cliPath()
	if err != nil {
		return err
	}
	cmd := exec.Command(p, append([]string{"--json"}, args...)...)
	hideConsole(cmd)
	cmd.Stdin = strings.NewReader(stdin)
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
	running.Lock()
	running.cmds[cmd] = true
	running.Unlock()
	defer func() {
		running.Lock()
		delete(running.cmds, cmd)
		running.Unlock()
	}()
	var (
		wg      sync.WaitGroup
		last    string
		failed  sync.Once
		readErr error
	)
	// (the other reader then ends too: the CLI is gone)
	unread := func(err error) {
		failed.Do(func() {
			readErr = fmt.Errorf("a line the CLI wrote is too long to read (%v)", err)
			_ = cmd.Process.Kill()
		})
	}
	wg.Add(2)
	go func() {
		defer wg.Done()
		if err := lines(errs, func(l string) {
			last = l
			if onNote != nil {
				onNote(l)
			}
		}); err != nil {
			unread(err)
		}
	}()
	go func() {
		defer wg.Done()
		if err := lines(out, func(l string) {
			var r Record
			if json.Unmarshal([]byte(l), &r) == nil && onRecord != nil {
				onRecord(r)
			}
		}); err != nil {
			unread(err)
		}
	}()
	wg.Wait()
	err = cmd.Wait()
	var exit *exec.ExitError
	switch {
	case readErr != nil:
		return readErr
	case err == nil:
		return nil
	// (the CLI's own ends: what failed, or a command line it does not take)
	case errors.As(err, &exit) && (exit.ExitCode() == 1 || exit.ExitCode() == 2) && last != "":
		return errors.New(strings.TrimPrefix(last, "bmseeker: "))
	case last != "":
		return fmt.Errorf("the CLI stopped (%v); it last said: %s", err, last)
	default:
		return fmt.Errorf("the CLI stopped (%v)", err)
	}
}

// lines gives f each line of r but blank ones, trimmed; the error that
// ended them, if not the end (a line over 16 MiB).
func lines(r io.Reader, f func(string)) error {
	s := bufio.NewScanner(r)
	s.Buffer(make([]byte, 64*1024), 16*1024*1024)
	for s.Scan() {
		if l := strings.TrimSpace(s.Text()); l != "" {
			f(l)
		}
	}
	return s.Err()
}
