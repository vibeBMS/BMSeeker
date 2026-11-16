// BMSeeker's window: difficulty tables' missing charts, songs from their
// links into a BMS library, charts waiting for their songs, and pages that
// need a browser, on top of the bmseeker CLI (bundled in this binary).
//
//	bmseeker-gui                 the window
//	bmseeker-gui <BMS folder>    the window, with that library
//	bmseeker-gui <CLI command>   the CLI itself (link, song, table, library, event, --help ...)
package main

import (
	"log"
	"os"

	"github.com/egoist/mygo"
	"github.com/egoist/mygo/ui"
)

func main() {
	args := os.Args[1:]
	if isCLI(args) {
		passThrough(args)
		return
	}
	library := ""
	if len(args) > 0 {
		if st, err := os.Stat(args[0]); err == nil && st.IsDir() {
			library = args[0]
		}
	}
	a := newApp(library)
	mygo.App.WhenReady(func() {
		a.win = mygo.NewWindow(mygo.WindowOptions{
			Title:     "BMSeeker",
			Width:     980,
			Height:    720,
			MinWidth:  640,
			MinHeight: 480,
			Content:   ui.View(a.view),
		})
	})
	if err := mygo.App.Run(); err != nil {
		log.Fatal(err)
	}
}
