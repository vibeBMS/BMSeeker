package main

import "path/filepath"

// Settings are the window's choices, for as long as it is open (where
// bmseeker looks, endpoints and sources, is the CLI's config.json; what
// the window starts from is the CLI's too: readSettings).
type Settings struct {
	// The BMS library: a folder of song folders.
	Library string
	// Difficulty tables to choose from (the CLI's `tables` setting, then
	// the user's).
	Tables []string
	// Downloads may use third parties' copies (the CLI's --third-party):
	// other people's mirrors of dead sites and uploaders, Ginger, Konmai,
	// the community archive. Turned on only once the user has said OK to
	// thirdPartyWarning (remembered: thirdPartyAcknowledged).
	ThirdParty bool
	// The folder in a library where downloads wait (the CLI's `downloads`
	// setting: in the library, or a folder of its own).
	Downloads string
	// Whether the CLI's settings are read.
	Read bool
}

// What the window asks before third-party sources are turned on the first
// time.
const (
	thirdPartyTitle   = "Use third-party sources?"
	thirdPartyWarning = "Third-party sources are copies other people keep (mirrors of dead sites and uploaders, " +
		"Ginger, Konmai, the community archive), not the links the authors gave. BMSeeker neither hosts nor " +
		"vouches for them. Keeping to those sites' terms and to the copyright holders' rights is your responsibility."
)

// downloads is the library's downloads folder, as the CLI makes it (a
// `downloads` setting that is absolute is that folder; "" is the library
// itself); "" while the CLI's settings are not read, or there is no
// library.
func (s Settings) downloads() string {
	switch {
	case s.Library == "" || !s.Read:
		return ""
	case filepath.IsAbs(s.Downloads):
		return s.Downloads
	}
	return filepath.Join(s.Library, s.Downloads)
}
