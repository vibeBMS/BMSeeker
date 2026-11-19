//go:build !js

package main

import (
	"os"
	"path/filepath"
	"time"

	"github.com/egoist/mygo"
	"github.com/egoist/mygo/ui"
)

const noLibrary = "Choose your BMS folder in Settings first."

// libraryView chooses the BMS folder.
func (a *app) libraryView(c *ui.Context) {
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
					})
				}
			}()
		}
	})
}

// connect: the library is the folder chosen.
func (a *app) connect() {}

// thirdPartyFile records that the user said OK to thirdPartyWarning.
func thirdPartyFile() (string, error) {
	dir, err := os.UserConfigDir()
	if err != nil {
		return "", err
	}
	return filepath.Join(dir, "bmseeker", "third-party-acknowledged"), nil
}

// thirdPartyAcknowledged reports whether the user said OK to
// thirdPartyWarning before (on this computer).
func thirdPartyAcknowledged() bool {
	f, err := thirdPartyFile()
	if err != nil {
		return false
	}
	_, err = os.Stat(f)
	return err == nil
}

// acknowledgeThirdParty remembers that the user said OK.
func acknowledgeThirdParty() error {
	f, err := thirdPartyFile()
	if err != nil {
		return err
	}
	if err := os.MkdirAll(filepath.Dir(f), 0o755); err != nil {
		return err
	}
	return os.WriteFile(f, []byte(time.Now().UTC().Format(time.RFC3339)+"\n"), 0o644)
}
