//go:build !js

package main

import (
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
