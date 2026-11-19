package main

import "path/filepath"

// Settings are the window's choices, for as long as it is open (where
// bmseeker looks, endpoints and sources, is the CLI's config.json).
type Settings struct {
	// The BMS library: a folder of song folders.
	Library string
	// Difficulty tables to choose from.
	Tables []string
	// Downloads may use third parties' copies (the CLI's --third-party):
	// other people's mirrors of dead sites and uploaders, Ginger, Konmai,
	// the community archive. Turned on only once the user has said OK to
	// thirdPartyWarning (remembered: thirdPartyAcknowledged).
	ThirdParty bool
}

// What the window asks before third-party sources are turned on the first
// time.
const (
	thirdPartyTitle   = "Use third-party sources?"
	thirdPartyWarning = "Third-party sources are copies other people keep (mirrors of dead sites and uploaders, " +
		"Ginger, Konmai, the community archive), not the links the authors gave. BMSeeker neither hosts nor " +
		"vouches for them. Keeping to those sites' terms and to the copyright holders' rights is your responsibility."
)

// thirdPartyNotice starts the CLI's note that third-party sources are on,
// which the window, having asked, leaves out of its log.
const thirdPartyNotice = "bmseeker: third-party sources are on"

var defaultTables = []string{
	"https://darksabun.club/table/archive/insane1/",
	"https://darksabun.club/table/archive/normal1/",
	"https://stellabms.xyz/sl/table.html",
	"https://stellabms.xyz/st/table.html",
}

func defaultSettings() Settings {
	return Settings{Tables: append([]string(nil), defaultTables...)}
}

// downloads is the folder in a library where downloads wait (the CLI's
// default `downloads` setting).
func (s Settings) downloads() string {
	return filepath.Join(s.Library, ".bmseeker")
}
