# bmseeker

A BMS library tool: it reads difficulty tables, downloads the songs your library lacks from the links their authors
and the tables give, unpacks them, and puts separate charts (差分) into their songs' folders.
It is the library [vibeBMS](https://github.com/vibeBMS/vibe) uses, and a command-line tool that works without the game.

```sh
cargo install --git https://github.com/vibeBMS/BMSeeker --features cli   # the bmseeker command
```

As a library: `bmseeker = { git = "https://github.com/vibeBMS/BMSeeker" }` (the command-line tool's dependency,
`clap`, comes in only with the `cli` feature).

```text
bmseeker [--json] [--config FILE] [--third-party] [--name-folders] <noun> <verb> ...

link resolve <link>... [-t title -a artist]          what a link leads to (a file's URL and name / a page that
                                                     needs a browser / an error), without downloading
link get <link>... [-o folder|file] [-t -a --md5] [--search] [--any]
                                                     download from the links in turn (into a folder: under the
                                                     file's own name; --search: when all fail, the sources' links
                                                     too (BMS SEARCH ...); --any: a single file that is not a page
                                                     (a chart, a video) too)
song find [-t -a --md5] [--diff] [--table URL]... [--list FILE|URL]...
                                                     the links the sources know for the song (--diff: its separate
                                                     chart)
table show <URL>                                     a difficulty table and its charts per level
table missing <URL> -L library [-l level] [--get [--reinstall]] [--also URL]... [--list ...] [--own-links]
                                                     the charts the library lacks (--get: download them)
table export lr2|bmt <URL> <folder>                  an LR2 custom folder / a beatoraja table cache
library import <file> -L library [--reinstall]       a downloaded file into the library
library place <chart> -L library [--to folder] [--min percent]
                                                     the song folders a separate chart may go into (or move it)
library pending -L library                           separate charts waiting for their songs
library install <link>... -L library [--diff] [-t -a --md5] [--own-links] [--reinstall]
                                                     a song (or its separate chart): its links in turn, then the
                                                     sources' links
event works <manbow event number> [--overrides FILE|-]
                                                     each work's links and which is the main archive, an add-on, a
                                                     mirror (JSON Lines)
event check [works FILE|-] [--known FILE] [--days N] [--minutes M] [--search]
                                                     each work's links downloaded and looked into (JSON Lines)
```

With `--json`, every command writes JSON Lines (one record a line, as it goes). The `event` commands always do, and
read records from a file or `-` (stdin): JSON Lines, a JSON array, or an object by key. Notes for people go to
stderr only.

```text
work   {"num","title","artist","team","kind","size_kb","page","links":[{"label","url"}],"main":[url],"extra":[[url]],"review"?, ...}
check  {"url","state":"archive|chart|page|gone|error","kind"?,"format"?,"size","charts","title"?,"error"?,"at",
        "num"?,"source"?,"mirrored"?}
```

For example `bmseeker event works 152 --overrides overrides.json > works.jsonl`,
`bmseeker event check works.jsonl --known links.jsonl --minutes 290 > links.new.jsonl`.
`event check --search`: for a work none of whose links gives an archive, the links the sources know for it by title
and artist (BMS SEARCH; with `--third-party`, the community archive too) are checked as well and written with the
work's `"num"` and their `"source"`. With `--third-party`, a failed link that a dead uploader's mirrors cover is checked
again through them (`"mirrored"`).

The places other links are looked for (sources) are asked in the order of the `sources` setting: difficulty tables
(`table`; more with `--also`), the Stella uploader's list (`stella`), TSV lists (`tsv`, `--list`; columns `md5`,
`url_diff`, `url`), and [BMS SEARCH](https://bmssearch.net) (`bmssearch`: chart → work → the download links its author
gave; a separate chart not tied to a work is searched by title and artist).
`--third-party` (setting `"third_party": true`; off by default) adds third parties' copies: mirrors of dead sites'
separate charts (`mirrors`: by the chart's file name BMS SEARCH knows, in the `chart_mirrors` folders, by default
darksabun's mirror of absolute), the community archive's "BMS Library" (`bmslibrary`: bms.kyouko.moe's
`Title (by Artist).rar` by title and artist; asked for separate charts too, as a song's archive often holds them), and
dead uploaders' files by number (`uploaders`). What is found by name (`mirrors`, `bmslibrary`) goes into the library
only if it holds the chart (its MD5); a song's archive also when its charts are the song's (title and artist), the
chart wanted being a separate one. `--own-links`: a song's own links only.

What a download holds is sorted before anything goes into the library: an archive may hold several songs' folders
and charts of their own (packages, by the BeMusicSeeker fork's rules: a folder whose charts' files are there is one,
with the folders below; a folder of one chart or one song's charts is one; other charts are each one), and where those
rules are not sure, the whole archive is one, as before. Each package is a song new to the library (a folder of its
own, named after the song's title or the archive, `name (2)` beside another song of that name), the library's
already (state `owned`: nothing is downloaded into a second folder; only the files its folder lacks come in, or with
`--reinstall` / `"reinstall": true` newer ones over those there too), or charts for a song's folder there (state
`placed`). Such a package goes into its song's folder as it is laid out (its `sound/` folder there too); which folder
is decided first by its own charts the library has (each votes for its folder; two folders with as many votes, as
when the library holds a chart twice, are never picked between: the chart waits), then by the song's own charts a
table names (`org_md5`), then by the files the charts use and their title and artist (as the BeMusicSeeker fork
compares them: NFKC, the chart's name at the end of the title cut (`[ANOTHER]`, `-7EX-`, kept up to a `MIX`), the
artist's credits for anything but the music removed (`/ obj:`, `feat.`, `BGA:`, `差分`; not `remix`, `作曲`); the same
song when the artists are the same and the titles' Dice coefficient is at least 0.72, which a chart needs to go into
a folder without asking (two such folders: it waits); folders whose title or artist is alike, Dice 0.5, are offered
first). What a song's link brings is a
folder of its own unless where it goes is sure. A separate chart whose song is not in the library yet waits in the
downloads folder (of a pack of many, only the chart wanted; with `table missing --get`, the run's missing charts) and
goes into the song's folder when the song comes. `table missing --get` takes each missing chart in turn: one the run
has brought in already is skipped; when the library has its song (one of the song's charts the table names, or a
chart of its title and artist), its separate chart is downloaded first and the song only if the chart is left waiting
(the song is never downloaded into a second folder); else the song, then its separate chart unless the song brought
it. A file is downloaded once a run, whatever link leads to it (Google Drive's `gdrive:ID`, MediaFire's, MEGA's,
Dropbox's, else the URL without its `#` part; a folder's link with the song looked for in it). A short link (`db.tt`, `bit.ly`) whose redirects
end at a host bmseeker knows (Dropbox, Google Drive...) is opened from there. A server's passing failure (HTTP 429,
502, 503, 504) is tried again twice. Google Drive's "Quota exceeded" (a file downloaded too often lately) is reported
as such: it passes within a day.

Dead uploaders' links (with `--third-party`) are downloaded from copies by number (`uploaders`). By default: gnqg
(`gnqg.rosx.net/upload/upload.cgi?get=03691`) → `gnqg-upload/03691.zip` on bms.hexlataia.xyz, then on darksabun.club,
then the community archive's gnqg folder; absolute (`absolute.pv.land.to/uploader/src/up0650.zip`, its other boards
too) → darksabun.club's `mirror/absolute/uploader/up0650.zip`. In a copy's address `{n}` is the number, `{h}` its
hundreds (`36`), `{f}` the link's file name and `{d}` its first folder (the board); a Google Drive folder link goes
down `#sub/folder/file`.

Settings (`--config`, `$BMSEEKER_CONFIG`, `<config folder>/bmseeker/config.json`) are JSON over the defaults:
`user_agent`, `mirror_catalog` (the BMS event mirror's list; empty: not used), `manbow`, `bmssearch_api`,
`stella_list`, `mega_api`, `sources`, `third_party`, `uploaders`, `chart_mirrors`, `bms_library`, `tables`,
`downloads` (the folder in a library where downloads wait), `min_coverage`, `reinstall`, `folder_naming`,
`folder_name_format`, `sjis_only`.

A song new to the library gets a folder named after the song's title (a table's), or the archive. With
`--name-folders` (setting `"folder_naming": true`; off by default) it is named after its charts as BeMusicSeeker names
one: `folder_name_format` (default `[%ARTIST%] %TITLE%`) with what the charts' artists and titles share (their chart
names such as `[ANOTHER]`, prefixes such as `BMS:` and credits such as `/ obj:` trimmed; the best by its length^0.65 ×
how many charts give it), characters a file name cannot have made full-width, at most 128 bytes (250 for the whole
path) as Shift_JIS counts them, `name (2)` beside another song of that name, `新しいフォルダー` when nothing is left;
`"sjis_only": true` keeps only characters Shift_JIS has (LR2 on Windows). A folder the archive wraps everything in is
left out (`[alpha] Song/song.bms`, not `[alpha] Song/Song_v1/song.bms`), and a song the library has under another
folder name (a chart of the same title and artist there) goes into that folder, not a second one.

`cargo run --features cli -- table missing https://stellabms.xyz/sl/table.html -L ~/bms -l 0`

## Window (`gui/`)

A native window made with [MyGo](https://github.com/egoist/mygo) (Go). A release has two binaries, `bmseeker` (the
CLI) and `bmseeker-gui`, which carries the CLI inside:

```text
bmseeker-gui                 the window
bmseeker-gui <BMS folder>    the window, with that library
bmseeker-gui <CLI command>   the CLI itself (link, song, table, library, event, --help ...)
```

- **Tables**: choose a difficulty table to see the charts the library lacks (by level), and download one or all those
  shown (the song, then the separate chart).
- **Link**: one link into the library.
- **Waiting**: separate charts that came before their songs, and the likeliest folder.
- **Browser**: links that only give a page (OneDrive ...). Opening one shows it in a web view, which presses its
  Download button; what it downloads goes into the library.
- **Settings**: the BMS folder, third-party mirrors (`--third-party`), the difficulty tables (forgotten when the window
  closes).

The window runs the CLI with `--json` and reads its records. The CLI it carries is unpacked into the user's cache
folder on first use (`$BMSEEKER_CLI` if set; a development build without it uses `bmseeker` on the PATH).

Build: `gui/build.sh` (builds the CLI with `cargo build --release --features cli` into `gui/bin/`, then `go build`;
Go 1.27+).

## Modules

| Module | What it does |
| --- | --- |
| `table` | bmstable difficulty tables (the HTML's `<meta name="bmstable">` → header → data) |
| `hosts` | link → archive: direct links, Google Drive files and folders, Dropbox, MediaFire, MEGA files and folders (in a pack, the song found by title and artist), event work pages on manbow and BMS SEARCH (venue.bmssearch.net; a manbow work in the [event mirror](https://github.com/vibeBMS/BMS) comes from the mirror), dead uploaders' mirrors. OneDrive in a browser, with `PRESS_DOWNLOAD_JS` |
| `unpack` / `import` | unpacking .zip / .7z / .rar / .lzh into a disk or any `fs::Store` (Shift_JIS and EUC-KR names; no escaping the folder, no archive bombs; the archive's file times kept; archives inside one holding no chart unpacked too) |
| `placement` | which song folder a separate chart belongs in: a song's folder is one holding charts, its files matched by their path in it (`sound/kick.wav` is `sound/kick.ogg` there, not a `kick.ogg` beside the charts) and kind (sounds .wav .ogg .mp3 .flac, images .png .bmp .jpg .jpeg, movies); 99% or more of the sounds the chart uses, counting the files that came with it, plus the same song (title and artist), else up to three candidates holding 80% or more (ranked by sounds, then BGA images, movies, stage file / banner / back image); files of the same name overwritten smartly (`clash`: a chart never replaces another; the same size within 2 seconds is the same file; a newer one replaces, an older one does not; where file times mean nothing, as in a web page's storage, the one there stays) |
| `library` | the library as the install tools see it: its songs' folders (those holding charts) with their files, the charts' titles and artists, and which folders hold a chart of an MD5 or SHA-256; read from the files once (`scan`), folder by folder when first needed (`new`), or given by a player's own records of its charts (`from_catalog`: a web page cannot read its library's charts when it likes); what comes in is added to it (`add`, `add_folder`). The `*_in` functions of `install`, `import` and `placement` work in one |
| `package` | what a download holds: its packages (`split`: songs' folders, charts of their own; the whole download when not sure) and what each is to the library (`classify`: owned, a new song, charts for a song's folder there; the link it came from counting most) |
| `install` | one song of a table: its song and separate chart links downloaded into the library (what was found by name only if it holds the chart); what arrives goes through `take` (on any `fs::Store`): unpacked into a folder of its own, planned without touching a file (`plan`), carried out (`apply`) |
| `sources` | where else a song's links are looked for: tables, Stella's list, TSV lists, BMS SEARCH, chart mirrors, the community archive |
| `links` | other links by a chart's MD5 (the tables given, the Stella uploader's list): filling empty or dead links, trying them in turn when one does not download |
| `manbow` | manbow events: works and their links (with labels) from the URL list and work list, and which link is the main archive (HQ > ogg > file, the newest package), an add-on (extra charts, BGA alone) or a mirror (the BMS event mirror's `works.json`; the links to download from a work page) |
| `bmssearch` | the BMS SEARCH API: a chart's work and its download links |
| `probe` | a link downloaded, unpacked and sorted: downloaded / page / gone (404, 410) / error; main (90% or more of its charts' sounds inside), add-on (separate charts), media (no charts) |
| `export` | a difficulty table as an LR2 custom folder (`.lr2folder`) or a beatoraja cache (`.bmt`) |
| `fs` | file access (`Disk`, or wherever a player keeps its library) |

Everything that reads the network is a `net::Task` (an IO-free job that hands out requests and takes their answers):
`hosts::Resolver` (a link through the `Host` strategies), `table::TableLoader`, `bmssearch::Lookup`, the sources.
Natively `net::run` runs one blocking; the browser (vibeBMS's web build) runs the same job asynchronously. Where it
reads from (APIs, mirrors, lists) and the user agent are all in `config::Config`. Where the design is going:
[DESIGN.md](DESIGN.md).

## Web (wasm32)

The library builds for wasm32 too (the network is `net::Task`, so the browser makes the requests; `import`,
`install::take` and `unpack` work on whatever `fs::Store` the page keeps its files in). `unrar-rs`, which unpacks RAR, builds for
wasm32-unknown-unknown from 0.16.4; `delharc` (LHA) builds for it as it is.

## Credits

The rules for guessing a chart's folder, smart overwriting, filling links and exporting follow the behaviour of
[BeMusicSeeker](https://github.com/Neeted/bemusicseeker-unofficial-fork) (by rib_2_bit; Neeted's unofficial fork; MIT),
written again in Rust.

## License

`MIT OR Apache-2.0`. `.rar` files are unpacked with
[unrar-rs](https://github.com/scryer-media/rarpar/tree/main/crates/unrar-rs) (Apache-2.0, with the UnRAR license
restriction).

> UnRAR source code may be used in any software to handle RAR archives without limitations free of charge, but cannot be used to develop RAR (WinRAR) compatible archiver and to re-create RAR compression algorithm, which is proprietary. Distribution of modified UnRAR source code in separate form or as a part of other software is permitted, provided that full text of this paragraph, starting from "UnRAR source code" words, is included in license, or in documentation if license is not available, and in source code comments of resulting package.
