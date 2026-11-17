//go:build js

package main

import (
	"encoding/json"
	"errors"
	"fmt"
	"strings"
	"sync"
	"syscall/js"
)

// In a browser the commands are bmseeker-web's, in a worker (web/www): the
// page's `bmseeker` runs them and hands their lines over as the CLI writes
// them.

func isCLI([]string) bool { return false }

func passThrough([]string) {}

// shim is the page's side of BMSeeker web (web/www/bmseeker.js).
func shim() js.Value { return js.Global().Get("bmseeker") }

// await waits for the promise p: its value, or why it failed. Not on the
// goroutine that draws.
func await(p js.Value) (js.Value, error) {
	return awaitLines(func(js.Value) js.Value { return p }, nil)
}

// awaitLines waits for the promise start makes, given a callback that
// takes lines (kind, text); each line goes to onLine on this goroutine as
// it comes. JavaScript waits for the callbacks it calls, so they only
// queue: one that blocked (as an update of the window may) would stop the
// page.
func awaitLines(start func(each js.Value) js.Value, onLine func(kind, text string)) (js.Value, error) {
	type result struct {
		v   js.Value
		err error
	}
	done := make(chan result, 1)
	wake := make(chan struct{}, 1)
	var mu sync.Mutex
	var queued [][2]string
	each := js.FuncOf(func(_ js.Value, v []js.Value) any {
		mu.Lock()
		queued = append(queued, [2]string{v[0].String(), v[1].String()})
		mu.Unlock()
		select {
		case wake <- struct{}{}:
		default:
		}
		return nil
	})
	ok := js.FuncOf(func(_ js.Value, args []js.Value) any {
		v := js.Undefined()
		if len(args) > 0 {
			v = args[0]
		}
		done <- result{v: v}
		return nil
	})
	fail := js.FuncOf(func(_ js.Value, args []js.Value) any {
		msg := "failed"
		if len(args) > 0 {
			if m := args[0].Get("message"); m.Type() == js.TypeString {
				msg = m.String()
			} else {
				msg = args[0].String()
			}
		}
		done <- result{err: errors.New(msg)}
		return nil
	})
	defer each.Release()
	defer ok.Release()
	defer fail.Release()
	start(each.Value).Call("then", ok, fail)
	drain := func() {
		mu.Lock()
		lines := queued
		queued = nil
		mu.Unlock()
		for _, l := range lines {
			onLine(l[0], l[1])
		}
	}
	for {
		select {
		case <-wake:
			drain()
		case r := <-done:
			drain()
			return r.v, r.err
		}
	}
}

// runCLI runs `bmseeker --json args...` in the page: each record it writes
// goes to onRecord and each note to onNote, as they come; then it returns
// how the command ended.
func runCLI(args []string, onRecord func(Record), onNote func(string)) error {
	list := make([]any, 0, len(args)+1)
	list = append(list, "--json")
	for _, a := range args {
		list = append(list, a)
	}
	var last string
	code, err := awaitLines(func(each js.Value) js.Value {
		return shim().Call("run", list, each)
	}, func(kind, text string) {
		var r Record
		switch {
		case kind != "record":
			last = text
			if onNote != nil {
				onNote(text)
			}
		case json.Unmarshal([]byte(text), &r) == nil && onRecord != nil:
			onRecord(r)
		}
	})
	switch {
	case err != nil:
		return err
	case code.Int() == 0:
		return nil
	case last != "":
		return errors.New(strings.TrimPrefix(last, "bmseeker: "))
	default:
		return fmt.Errorf("exit status %d", code.Int())
	}
}
