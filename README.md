# bmseeker

A BMS library tool: it reads difficulty tables, downloads the songs your library lacks from the links their authors
and the tables give, unpacks them, and puts separate charts (差分) into their songs' folders.
It is the library [vibeBMS](https://github.com/vibeBMS/vibe) uses, and comes as a command-line tool, a window and a
web page that work without the game.
It is inspired by [BeMusicSeeker](https://github.com/Neeted/bemusicseeker-unofficial-fork) (by rib_2_bit), whose
library rules it follows ([Credits](#credits)).

## Get it

The [nightly](https://github.com/vibeBMS/BMSeeker/releases/tag/nightly) release has `bmseeker-<system>` (the CLI),
`bmseeker-gui-<system>` (the window) for Linux, Windows and macOS, and `bmseeker-web.zip`. Or install the CLI:

```sh
cargo install --git https://github.com/vibeBMS/BMSeeker --features cli   # the bmseeker command
```

As a library: `bmseeker = { git = "https://github.com/vibeBMS/BMSeeker" }` (the CLI's dependency, `clap`, comes in
only with the `cli` feature). The web page is at <https://vibebms.github.io/BMSeeker/>.

## Command line

```text
bmseeker [--json] [--config FILE] [--third-party] [--name-folders] [--skip-host HOST]... <noun> <verb> ...

link resolve <link>...                   what a link leads to, without downloading
link get <link>... [-o folder|file]      download from the links in turn
song find [-t -a --md5] [--diff]         the links the sources know for a song
table show <URL>                         a difficulty table and its charts per level
table missing <URL> -L library [--get]   the charts the library lacks (--get: download them)
table export lr2|bmt <URL> <folder>      an LR2 custom folder / a beatoraja table cache
library import <file> -L library         a downloaded file into the library
library place <chart> -L library         the song folders a separate chart may go into
library pending -L library               separate charts waiting for their songs
library install <link>... -L library     a song (or its separate chart) from its links, then the sources'
event works | check                      an event's works and their links (JSON Lines)
config show                              the settings in effect
```

For example:

```sh
bmseeker table missing https://stellabms.xyz/sl/table.html -L ~/bms -l 0 --get
bmseeker library install https://example.com/song.zip -L ~/bms
bmseeker link get https://www.dropbox.com/s/.../song.zip -o ~/Downloads
```

With `--json`, every command writes JSON Lines, one record a line, for programs; notes for people go to stderr.
Every option, the records and the settings: [DESIGN.md](DESIGN.md#cli).

## How it works

- A song's links are tried in order, then the other places that know it (tables, Stella, BMS SEARCH).
- What a download holds is sorted before anything goes into the library.
- A song the library has already is not added a second time; only the files its folder lacks come in.
- A download replaces a file of the library only when it holds one of that song's charts (by its hash), or with `--reinstall`; a song found by its title and artist alone gets only the files its folder lacks.
- A separate chart goes into its song's folder, or waits in the downloads folder until the song comes.
- Each file is downloaded once a run, whatever link leads to it.
- A site that refuses BMSeeker is not asked again for the rest of the run.

The rules: [DESIGN.md](DESIGN.md#into-the-library).

## Third-party sources and disclaimer

Where BMSeeker downloads from, by default:

- **The authors' and the services' own:** the links difficulty tables give (the tables' pages and data, and the
  links in them to where authors put their songs: Google Drive, Dropbox, MediaFire, MEGA, pixeldrain, an author's own
  site), the Stella uploader's list (stellabms.xyz), BMS SEARCH's public API (api.bmssearch.net), and event venues
  (manbow, venue.bmssearch.net). For a manbow event's work, the BMS event mirror
  ([vibeBMS/BMS](https://github.com/vibeBMS/BMS), kept by BMSeeker's authors) is asked first; the setting
  `"mirror_catalog": ""` turns it off.
- **Third parties' copies**, off unless `--third-party`, the setting `"third_party": true` or the window's
  "Third-party sources" is on: other people's mirrors of dead sites and uploaders (darksabun.club,
  bms.hexlataia.xyz), packages found by a chart's MD5 on Ginger (gingerrush.com) and Konmai (bms.alvorna.com), and the
  community archive ("BMS Library" on Google Drive). These are not the authors' links; BMSeeker does not check where
  their files came from, only that what they bring holds the chart wanted.

How BMSeeker behaves towards these sites:

- It identifies itself: natively every request carries its own `User-Agent`
  (`ureq/<version> BMSeeker/<version> (+https://github.com/vibeBMS/BMSeeker)`: the HTTP library's name, then
  BMSeeker's), never a browser's. In a web page the browser's
  `User-Agent` is sent (to hosts reached through the CORS proxy, the proxy's).
- It honours refusals and rate limits: a service that answers 429, or 401/403/451 where its own page, API or listing
  is asked, or with a browser check (a Cloudflare challenge, Turnstile, Google's CAPTCHA), is not asked again for the
  rest of the run, not through a redirect either (natively), and the refusal is reported (see
  [DESIGN.md, Refusals](DESIGN.md#refusals)). A 401/403/451 for a file a link leads to is that file's failure; a host
  that answers so for three different files one after another, with no answer of its own that came between them, is
  not asked again either (but a file host, where each is one file's quota or sharing). It does not retry with other
  queries, other addresses or other headers.
- It asks only the web: natively a table, a link or a redirect cannot make it ask this machine or its own network
  (`localhost`, private and link-local addresses; the setting `"local_hosts": true` allows them, for a mirror on a
  LAN), and a redirect from https to http is not followed.
- It does not bypass access controls: no solving or evading of challenges, CAPTCHAs, logins or download limits.
  A link that needs a browser is shown to the user in one (the window's web view presses a OneDrive share page's
  Download button once, as a person would; the web page opens it in a tab).
  Google Drive's "can't scan this file for viruses" notice is passed (`confirm=t`), as a browser's "Download anyway"
  does: it is a notice, not an access control, and what is downloaded is treated as untrusted either way.

BMSeeker hosts no songs or charts, and is not affiliated with, endorsed by or operated by any of the services above.
Users are responsible for complying with each site's terms of use and with the rights of the copyright holders of what
they download.

A site operator who wants BMSeeker not to use their site, or to use it differently, can say so in the
[issue tracker](https://github.com/vibeBMS/BMSeeker/issues).

## Window

A native window made with [MyGo](https://github.com/egoist/mygo) (Go). `bmseeker-gui` carries the CLI inside:

```text
bmseeker-gui                 the window
bmseeker-gui <BMS folder>    the window, with that library
bmseeker-gui <CLI command>   the CLI itself (link, song, table, library, event, --help ...)
```

- **Tables**: the charts the library lacks in a difficulty table, by level; download one or all those shown.
- **Link**: one link into the library.
- **Waiting**: separate charts that came before their songs, and the likeliest folder.
- **Browser**: links that only give a page (OneDrive ...), opened in a web view (a OneDrive share's Download button
  is pressed for you).
- **Settings**: the BMS folder, third-party sources (asked once before they are on), the difficulty tables.

The window starts from the CLI's settings (`config show`: its `tables`, its downloads folder, `third_party`).

## Web page

The same window runs in a browser at <https://vibebms.github.io/BMSeeker/>. Settings › Where songs go:

- **vibeBMS web library (this browser)**: the library of [vibeBMS](https://vibebms.github.io/vibe/)'s web build.
- **A folder on this computer** (Chrome, Edge): LR2's or beatoraja's songs.
- **Download as zip**: what comes in, as one zip.

Downloads go through vibeBMS's CORS proxy. A link that gives a page opens in a tab: download its file there, then
Browser › Import a downloaded file. A download in the page has 30 minutes in all: a large pack from a slow host
is downloaded the same way.

## Building

`cargo build --features cli` builds the CLI; `gui/build.sh` the window (Go 1.27+); `web/build.sh` the web page.
Details: [DESIGN.md](DESIGN.md).

## Credits

The rules for guessing a chart's folder, smart overwriting, filling links and exporting follow the behaviour of
[BeMusicSeeker](https://github.com/Neeted/bemusicseeker-unofficial-fork) (by rib_2_bit; Neeted's unofficial fork; MIT),
written again in Rust, with some of its regular expressions as they are; its notices are in
[THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md).

## License

`MIT OR Apache-2.0`. RAR files are unpacked with unrar-rs, under the UnRAR licence restriction; its notice is in
full in [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md).
