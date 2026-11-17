# bmseeker: design

A library with no hard-coded places, a CLI whose commands write JSON Lines,
and a GUI (Go, MyGo) on top of the CLI, in this one repository:

```
Cargo.toml, src/        the library and the CLI (src/bin/bmseeker, feature `cli`)
gui/                    the GUI: a Go module that embeds the CLI binary
.github/workflows/      CI; releases build the CLI, then the GUI around it
```

Features: `cli` (the command-line tool: clap, and `tools`), `tools` (what
only the tool does: an event's works, probing a link, exporting a table),
`testkit` (archives, folders and a local server for tests, a player's too).

## Layers

```
net            Request, Answer, NetError (code() for programs), Task / Next: requests as data
install::job   Effect / Fed / Job / Driver: downloads as data; run_job, TaskJob; Acquire, Files, Install
native         Native: the driver natively (one HTTP agent, downloads on the calling thread); fetch, fetch_files
hosts          Host strategies (a link -> a file, other links, or a page), Resolver, Context
sources        Source strategies (a chart's MD5, a song's title and artist -> links)
unpack         archives (zip, 7z, rar, lzh; nested) into an fs::Store, capped
package        a staged download as packages, each classified against the library
install        Done / Failure (code()); plan (nothing written), apply, take; Entry, EntryJob, Batch
library        the library's song folders, their files, the charts' hashes, titles and artists
placement      which song folder a chart of its own belongs in; the smart overwrite (clash)
titles         titles and artists as the BeMusicSeeker fork compares them; same_song
table, links, bmssearch, manbow, export, probe, naming, chart, text, config, progress, fs
bin/bmseeker   the CLI: noun-verb commands, --json
```

### Tasks, jobs and drivers

Everything that talks to the network is a `net::Task`: `start()` and
`feed(answer)` return `Next::Ask(Request)` or `Next::Done(output)`. A task
never does I/O, so the same code runs natively, in a web page (vibeBMS's
fetch through its CORS proxy) and in tests (canned answers).

A download is an `install::job::Job`: it asks for `Effect`s (a request, a
file downloaded into a slot, a wait) and is fed what came of them (`Fed`).
`run_job` runs one with a `Driver`; `TaskJob` runs a task as a job. A
request or a download that meets a server's passing failure (429, 502, 503,
504) is asked again by the job after a wait (3 s, then 10 s), so every
driver retries alike and none needs to.

- `Acquire`: one link to its file (resolved through the hosts; a page
  reached through redirects opened from where it ended; a page's other
  links tried; Google Drive's quota told apart from a page).
- `Files`: several links' files in order (a skin, an event's work).
- `Install`: a song (or its separate chart) from its links, then each
  source's, into the library, until one brings it in.
- `EntryJob`: a table's entry (`Entry`) as its steps (`entry_steps`: the
  separate chart first when the library has its song, else the song then
  the chart), then add-ons, each an `Install`. The sources are asked only
  with the chart's MD5, and only for the song step or, with the song in the
  library, the chart step; never for an add-on.

`native::Native` is the driver natively. Requests go through one
process-wide `ureq` agent, so a host's connection is kept for the next
request. A download has an agent of its own whose connections watch its
progress while they wait for data: a cancel or a minute of silence ends it
on the calling thread (no helper thread, no queue); a MEGA file is
decrypted as it comes. Waits are slept, ended by a cancel. `Native::run`
and `Native::get` are blocking conveniences for tasks.

### Hosts (strategy)

```rust
trait Host: Send + Sync {
    fn name(&self) -> &'static str;
    fn accepts(&self, link: &Link) -> bool;
    fn open(&self, link: &Link, want: Option<&Want>, config: &Config) -> BoxTask<Resolved>;
}
enum Resolved { File(Target), Follow(Vec<String>), Page, Failed(NetError) }
struct Target { url: String, mega_key: Option<Vec<u8>>, name: Option<String> }
```

`Resolver` walks a registry (`Hosts::default()`: Google Drive file and
folder, Dropbox, manbow work (the event mirror first), BMS SEARCH venue,
MediaFire, MEGA file and folder, browser-only hosts, direct; dead
uploaders' mirrors first with `third_party`). `Follow` goes on with the
next link (at most a few hops). Adding a host is adding it to the registry.

### Sources (strategy)

```rust
trait Source: Send + Sync {
    fn name(&self) -> &'static str;
    fn find(&self, q: &Query) -> BoxTask<Vec<String>>;
    fn checked(&self) -> bool; // what it brings must hold the chart's MD5
}
struct Query { md5: String, want: Want, diff: bool, org_md5: Vec<String> }
```

Difficulty tables, the Stella uploader's list, TSV lists, BMS SEARCH; with
`third_party` (off by default) chart mirrors, Ginger's and Konmai's
packages by MD5, and the community archive. Which and in what order:
`Config::sources`.

### Into the library

A download is unpacked into a folder of its own (`unpack`), sorted into
packages (`package::split`, `classify`), planned against the `Library`
without touching a file (`install::plan`), then carried out
(`install::apply`: a move that fails leaves the library as it was);
`install::take` does all of it, on any `fs::Store` (a disk, a web page's
storage). `import::import` is the same for a file brought in by hand.

Whether two charts are of one song is `titles::same_song` (the fork's:
the artists the same and the titles' Dice at least 0.72, both normalized:
NFKC, the chart's name cut from the title, credits but the music's cut
from the artist): for reusing a song's folder, whether the library has a
table entry's song, and what a download found by name must hold. A
folder's charts are one package when their normalized titles and artists
are equal, as the fork splits packages.

The `Library` keeps each song folder's file keys once (for `placement`'s
coverage), the charts by hash, and the charts by their normalized artist
(so a song is looked up among one artist's charts). It is read from the
files (`scan_cached`, in parallel natively, with a cache of what the last
scan read: the CLI keeps it in the downloads folder), folder by folder when
first needed (`new`), or given by a player's records (`from_catalog`).

### Config

All endpoints have defaults in `Config::default()` and can be set by a
JSON file (`--config`, `BMSEEKER_CONFIG`, the config folder) or flags; the
settings are listed in the README.

## CLI

`bmseeker [--json] [--config FILE] [--third-party] [--name-folders] <noun> <verb> ...`
(the commands: README). With `--json` every command writes JSON Lines (one
record per line).

### Records

```
work   {"num","title","artist","team","kind","size_kb","page",
        "links":[{"label","url"}],"main":[url],"extra":[[url]],"review"?}
check  {"url","state":"archive|chart|page|gone|error","kind"?,"format"?,
        "size","charts","title"?,"code"?,"error"?,"at"}
link   {"link","state":"file|page|gone|error","host","url"?,"code"?,"error"?}
got    {"file","name","format","link","errors":[...]}
done   {"title","state":"added|owned|placed|waiting|page|failed","path"?,"link"?,"code"?,"error"?}
```

`"error"` is for a person; `"code"` for a program: `status`, `stalled`,
`cancelled`, `gone`, `other` (the download), `not_the_chart`,
`no_chart_link`, `no_link`, `downloaded` (the install), `unpack`.

`--overrides` takes work records (JSON or JSONL) whose fields replace the
guess for that `num`.

## GUI

Go with MyGo, native UI. It runs the CLI (`bmseeker --json <command>`) as
a child process and reads its records. Released as two binaries:
`bmseeker` (CLI) and the GUI, which embeds the CLI: started with a CLI
command it runs as the CLI (same arguments, stdin/stdout, exit code); with
no arguments or a path it opens the window. A link that needs a browser
opens in a MyGo web view, which presses the page's Download button and
hands the download back to the CLI.
