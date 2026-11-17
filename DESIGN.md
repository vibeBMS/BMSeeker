# bmseeker: design

A library with no hard-coded places, a CLI whose commands write JSON Lines,
and a GUI (Go, MyGo) on top of the CLI, in this one repository:

```
Cargo.toml, src/        the library and the CLI (src/bin/bmseeker, feature `cli`)
gui/                    the GUI: a Go module that embeds the CLI binary
.github/workflows/      CI; releases build the CLI, then the GUI around it
```

## Layers

```
net            Request, Answer, NetError, Task / Next (requests as data), run() with an Http
install::job   Job / Effect / Fed / Driver (downloads as data), run_job; Acquire, Files, Install
hosts          Host strategies (a link -> a file, other links, or a page), Resolver, Native driver
sources        Source strategies (a chart's MD5, a song's title and artist -> links)
unpack         archives (zip, 7z, rar, lzh; nested) into an fs::Store, capped
package        a staged download as packages, each classified against the library
install        Done / Failure; plan (nothing written), apply, take; entry steps and batches
library        the library's song folders, their files, the charts' hashes, titles and artists
placement      which song folder a chart of its own belongs in; the smart overwrite (clash)
table, links, bmssearch, manbow, export, probe, naming, titles, chart, text, config, progress
bin/bmseeker   the CLI (feature `cli`: clap): noun-verb commands, --json
```

### Tasks and jobs

Everything that talks to the network is a `net::Task`: `start()` and
`feed(answer)` return `Next::Ask(Request)` or `Next::Done(output)`. A task
never does I/O, so the same code runs natively (`net::run` with an `Http`
such as `net::Client`), in a web page (vibeBMS's fetch through its CORS
proxy) and in tests (`net::Canned`).

A download is an `install::job::Job`: it asks for `Effect`s (a request, a
file downloaded into a slot, a wait) and is fed what came of them (`Fed`).
`run_job` runs one with a `Driver`: natively `hosts::Native` (a file
streamed to disk, ended by cancel or a stall); in a web page the page's
driver. `Acquire` takes one link to its file, `Files` several, `Install` a
song (or its separate chart) from its links and then the sources' into the
library.

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
MediaFire, MEGA file and folder, browser-only hosts, direct). `Follow`
goes on with the next link (at most a few hops). A host is one struct;
adding one is adding it to the registry.

### Sources (strategy)

```rust
trait Source: Send + Sync {
    fn name(&self) -> &'static str;
    fn find(&self, q: &Query) -> BoxTask<Vec<String>>;
    fn checked(&self) -> bool; // found by name: what it brings must hold the chart
}
struct Query { md5: String, want: Want, diff: bool, org_md5: Vec<String> }
```

Difficulty tables, the Stella uploader's list, TSV lists, BMS SEARCH and
(third parties, off by default) chart mirrors, the community archive and
dead uploaders' copies. Which and in what order: `Config::sources`.

### Into the library

A download is unpacked into a folder of its own (`unpack`), sorted into
packages (`package::split`, `classify`), planned against the `Library`
without touching a file (`install::plan`), then carried out
(`install::apply`); `install::take` does all of it, on any `fs::Store` (a
disk, a web page's storage). `import::import` is the same for a file
brought in by hand, `install::run` for a song's links natively.

### Config

All endpoints have defaults in `Config::default()` and can be set by a
JSON file (`--config`, `BMSEEKER_CONFIG`, the config folder) or flags; the
settings are listed in the README.

## CLI

`bmseeker [--json] [--config FILE] <noun> <verb> ...`. With `--json` every
command writes JSON Lines (one record per line).

```
link resolve <LINK>...                     what each link leads to
link get <LINK>... [-o DIR] [--title T --artist A --md5 M] [--search] [--any]
song find [--md5 M] [--title T] [--artist A]
table show <URL>
table missing <URL> -L DIR [--level L] [--get]
table export lr2|bmt <URL> <OUT>
library import <FILE> -L DIR
library place <CHART> -L DIR [--to DIR] [--min PERCENT]
library pending -L DIR
library install <LINK>... -L DIR [--diff] [--title T --artist A --md5 M]
event works <VENUE> [--overrides FILE]     works of a manbow event
event check [FILE|-] [--known FILE]        each work's links checked
```

### Records

```
work   {"num","title","artist","team","kind","size_kb","page",
        "links":[{"label","url"}],"main":[url],"extra":[[url]],"review"?}
check  {"url","state":"archive|chart|page|gone|error","kind"?,"format"?,
        "size","charts","title"?,"error"?,"at"}
link   {"link","state":"file|page|gone|error","host","url"?,"error"?}
got    {"file","name","format","link","errors":[...]}
```

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
