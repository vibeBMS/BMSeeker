package main

import (
	"encoding/json"
	"os"
	"path/filepath"
)

// Settings are the GUI's own choices, kept in <config folder>/bmseeker/gui.json.
// Where bmseeker looks (endpoints, sources) is the CLI's config.json.
type Settings struct {
	// The BMS library: a folder of song folders.
	Library string `json:"library"`
	// Difficulty tables to choose from.
	Tables []string `json:"tables"`
	// Downloads may use third parties' copies (the CLI's --third-party):
	// other people's mirrors of dead sites and uploaders, the community
	// archive.
	ThirdParty bool `json:"third_party"`
}

var defaultTables = []string{
	"https://darksabun.club/table/archive/insane1/",
	"https://darksabun.club/table/archive/normal1/",
	"https://stellabms.xyz/sl/table.html",
	"https://stellabms.xyz/st/table.html",
}

func settingsFile() string {
	dir, err := os.UserConfigDir()
	if err != nil {
		dir = "."
	}
	return filepath.Join(dir, "bmseeker", "gui.json")
}

func loadSettings() Settings {
	s := Settings{Tables: append([]string(nil), defaultTables...)}
	if b, err := os.ReadFile(settingsFile()); err == nil {
		_ = json.Unmarshal(b, &s)
	}
	if len(s.Tables) == 0 {
		s.Tables = append([]string(nil), defaultTables...)
	}
	return s
}

func (s Settings) save() error {
	f := settingsFile()
	if err := os.MkdirAll(filepath.Dir(f), 0o755); err != nil {
		return err
	}
	b, err := json.MarshalIndent(s, "", "  ")
	if err != nil {
		return err
	}
	return os.WriteFile(f, b, 0o644)
}

// downloads is the folder in a library where downloads wait (the CLI's
// default `downloads` setting).
func (s Settings) downloads() string {
	return filepath.Join(s.Library, ".bmseeker")
}
