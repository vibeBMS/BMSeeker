# bmseeker: design

A library with no hard-coded places, a CLI whose commands write JSON Lines,
and a GUI (Go, MyGo) on top of the CLI, in this one repository. This file
is the reference: the layers and modules, the refusal, host and source
rules, how a download goes into the library, the settings, the CLI and its
records, the window, the web page and the releases.

```
Cargo.toml, src/        the library and the CLI (src/cli, src/bin/bmseeker, feature `cli`)
gui/                    the GUI: a Go module that embeds the CLI binary (or, for js/wasm, calls web/)
web/                    the web page: bmseeker-web (the CLI's commands in a worker), its JS, build and test
.github/workflows/      CI; releases build the CLI, then the GUI around it
```

Features: `cli` (the command-line tool: clap, and what only the tool does:
an event's works, probing a link, exporting a table), `testkit` (archives,
folders and a local server for tests, a player's too).

## Layers

```
net            Request, Answer, NetError (code() for programs), Task / Next: requests as data
net::job       Effect / Fed / Job / Driver: downloads as data (a Target, a file Served); run_job, TaskJob
refusal        Refusals: the services that refused in a run (Context::refused); Guarded (the guard); challenge pages
install::job   the jobs of a download: Acquire, Files, Install
native         Native: the transport natively (an HTTP agent for requests, one for downloads on the calling thread); fetch, fetch_files
web            what a web page's drivers share: the CORS proxy's URLs, a served download, an answer's error
hosts          Host strategies (a link -> a file, other links, or a page), Resolver, Context
sources        Source strategies (a chart's MD5, a song's title and artist -> links)
unpack         archives (zip, 7z, rar, lzh, tar, tar.gz; nested) into an fs::Store, capped; read: chosen members into a Sink; the archive name rules
package        a staged download as packages, each classified against the library
install        Done (state()) / Failure (code()); plan (nothing written), apply, take; Entry, EntryJob, Batch
library        the library's song folders, their files, the charts' hashes, titles and artists
placement      which song folder a chart of its own belongs in; the smart overwrite (clash)
titles         titles and artists as the BeMusicSeeker fork compares them; same_song
cli            the CLI's commands (noun-verb, --json) on a Run: a Store, a Driver, where lines go
bin/bmseeker   the CLI natively: the disk, Native, stdout / stderr
```

The layers, lowest first: base, parsing and data, the network model,
hosts and sources, archives, the library, install, drivers, front ends. A
module uses nothing of a layer above its own; which module is in which
layer, and the few uses upward that remain (each with why), are in
`tools/check_layers.py`, which CI runs.

The modules in more detail:

| Module | What it does |
| --- | --- |
| `table` | bmstable difficulty tables (the HTML's `<meta name="bmstable">` → header → data) |
| `hosts` | link → archive: direct links, Google Drive files and folders (their `resourcekey` kept), Dropbox, MediaFire, MEGA files and folders (in a pack, the song found by title and artist), event work pages on manbow and BMS SEARCH (venue.bmssearch.net; a chart's page bmssearch.net/bmses/ID: its work's links; a manbow work in the [event mirror](https://github.com/vibeBMS/BMS) comes from the mirror), dead uploaders' mirrors. OneDrive in a browser, with `PRESS_DOWNLOAD_JS` |
| `unpack` / `import` | unpacking .zip / .7z / .rar / .lzh / .tar / .tar.gz (.tgz) into a disk or any `fs::Store`, or reading the members a caller wants without writing them anywhere (`unpack::read`: each member offered in order with its safe path, size and time, and written where the caller says, passed by, or the end; within the caller's bytes and members, and the same other limits; a member passed by is not decoded in a zip, an LZH, a tar or a RAR that is not solid, and in a zip not read at all: its local header is compared with the directory when it is written, where `unpack` compares all of them before it writes any; the size a member is told written with is the bytes it was, counted as they were written, whatever the archive says) (Shift_JIS and EUC-KR names; no escaping the folder; each name one that Windows holds too, wherever it is unpacked: no device's name, no dot or space at its end; at most 4 GiB unpacked natively, 1 GiB in a web page, what an archive decodes for members passed by counted too; at most 250,000 files, in paths of 32 folders (a zip is read from its first byte: one with bytes before it, as a zip put after another, is not read; one whose directory says it lists more than twice the files (500,000 entries), its folders counted, is refused before that directory is read); a 7z's decoder within 1 GiB of memory, 256 MiB in a web page and on phones, its header within 17 MiB, 9 there (it lists the files, some 150 bytes each: a 7z of more than about 100,000 files, 50,000 there, is refused by it before the 250,000); a tar's long names within 1 MiB; the archive's file times kept; archives inside one holding no chart unpacked too; 7z on one thread in a web page and on phones; given up when its download is cancelled). `import` takes a file brought in by hand, and places the separate charts that waited for what came in. A chart is read up to 32 MiB (a larger one is a chart that cannot be read). What a download passes through (`.job-…` and `.take-…` in the downloads folder; `.in-…` there, or in the song's folder whose files it replaces) is made anew under a name that cannot be guessed; what a run that died left of them in the downloads folder goes after a day. A `.in-…` in a song's folder is not removed: it holds the song's own files a run that died while replacing them had set aside (`N.old`), to be put back by hand |
| `placement` | which song folder a separate chart belongs in: a song's folder is one holding charts, its files matched by their path in it (`sound/kick.wav` is `sound/kick.ogg` there, not a `kick.ogg` beside the charts) and kind (sounds .wav .ogg .mp3 .flac, images .png .bmp .jpg .jpeg, movies); 99% or more of the sounds the chart uses, counting the files that came with it, plus the same song (title and artist), else up to three candidates holding 80% or more (ranked by sounds, then BGA images, movies, stage file / banner / back image); files of the same name overwritten smartly (`clash`: a chart never replaces another; the same size within 2 seconds is the same file; a newer one replaces, an older one does not, nor one of a time more than a day to come (a zip's times are local, read as UTC); where file times mean nothing, as in a web page's storage, the one there stays) |
| `library` | the library as the install tools see it: its songs' folders (those holding charts) with their files, the charts' titles and artists (by artist, as compared), and which folders hold a chart of an MD5 or SHA-256; read from the files once (`scan_cached`), folder by folder when first needed (`new`), or given by a player's own records (`from_catalog`); what comes in is added to it (`add`, `add_folder`) |
| `package` | what a download holds: its packages (`split`: songs' folders, charts of their own; a file in a folder above them all, as a cover beside a song's folder, in none; the whole download when not sure) and what each is to the library (`classify`: owned, a new song, charts for a song's folder there; the link it came from counting most) |
| `install` | one song of a table: its song and separate chart links downloaded into the library (what was found by name only if it holds the chart); what arrives goes through `take` (on any `fs::Store`): unpacked into a folder of its own, planned without touching a file (`plan`), carried out (`apply`) |
| `sources` | where else a song's links are looked for: tables, Stella's list, TSV lists, BMS SEARCH, chart mirrors, Ginger's and Konmai's packages by MD5, the community archive |
| `links` | other links by a chart's MD5 (the tables given, the Stella uploader's list): filling empty or dead links, trying them in turn when one does not download |
| `manbow` | manbow events: works and their links (with labels) from the URL list and work list, and which link is the main archive (HQ > ogg > file, the newest package), an add-on (extra charts, BGA alone) or a mirror (the BMS event mirror's `works.json`; the links to download from a work page); the mirror's list of events and their indexes of works (`manbow::mirror`, which vibe reads too) |
| `bmssearch` | the BMS SEARCH API: a chart's work and its download links |
| `probe` | a link downloaded, unpacked and sorted: downloaded / page / gone (404, 410) / error; main (90% or more of its charts' sounds inside), add-on (separate charts), media (no charts) |
| `export` | a difficulty table as an LR2 custom folder (`.lr2folder`) or a beatoraja cache (`.bmt`). An LR2 export notes what it wrote (`bmseeker-export.json` in the folder): the next removes the levels the table no longer has by that note, no other file, and does not write into a folder noted as another table's |
| `naming` | a song's folder named after its charts (Folder naming, below) |
| `chart` | what bmseeker needs of a chart: its title, artist and the files it uses |
| `text` | text from BMS files and pages: Shift_JIS, EUC-KR, UTF-8 |
| `config` | every place read from, and the user agent (Config, below) |
| `progress` | how far a song's download has got, shared with whoever shows it |
| `fs` | file access (`Disk`, or wherever a player keeps its library): `Fs` reads and moves, a `Store` also writes |
| `native` | the network natively: `Native` drives the jobs (one HTTP agent for every request, so a host's connection is kept; a download streamed to disk on the calling thread, ended by a cancel, a minute without data or coming too slowly; a MEGA file decrypted as it comes and checked when whole; redirects followed one at a time, each kept to the rules below); `fetch`, `fetch_files` |

### Tasks, jobs and drivers

Everything that talks to the network is a `net::Task`: `start()` and
`feed(answer)` return `Next::Ask(Request)` or `Next::Done(output)`. A task
never does I/O, so the same code runs natively, in a web page (vibeBMS's
fetch through its CORS proxy) and in tests (canned answers).

A download is a `net::job::Job`: it asks for `Effect`s (a request, a
file downloaded into a slot, a wait) and is fed what came of them (`Fed`).
`run_job` runs one with a `Driver`; `TaskJob` runs a task as a job. A
request or a download that meets a server's passing failure (502, 503,
504) is asked again by the job after a wait (3 s, then 10 s), so every
driver retries alike and none needs to. A download announced larger than
the unpack budget, or growing past it, fails as an archive that large
would. The jobs of a download, in `install::job`:

- `Acquire`: one link to its file (resolved through the hosts; a page
  reached through redirects opened from where it ended; a page's other
  links tried; Google Drive's quota told apart from a page). A link that
  is not an http(s) one is a failure, never a page for a browser. The
  file's name, whoever gave it (the host, the server), is one file name.
- `Files`: several links' files in order (a skin, an event's work).
- `Install`: a song (or its separate chart) from its links, then each
  source's, into the library, until one brings it in.
- `EntryJob`: a table's entry (`Entry`) as its steps (`entry_steps`: the
  separate chart first when the library has its song, else the song then
  the chart), then add-ons, each an `Install`. The sources are asked only
  with the chart's MD5, and only for the song step or, with the song in the
  library, the chart step; never for an add-on. What of the entry's
  links (and add-ons) is no web link is dropped before the steps are
  made. `Entry::from_table_in` takes a table's entry's links from several
  `Links` lists in turn (`Links::song_links_in`, `chart_links_in`: as the
  lists made one, without making it).

A run's sources share a memo of their GETs (`net::Memo`: successes and
404s, 4 MiB at most), so a request several sources make is asked once.

`native::Native` is the transport natively. Requests go through one
process-wide `ureq` agent, so a host's connection is kept for the next
request. Its downloads have an agent of their own (a host's connection
kept for the next file) whose connections watch its progress while they
wait for data: a cancel or a minute of silence ends one on the calling
thread (no helper thread, no queue), and so does a minute that brought
under 240 KiB (4 KiB a second: a server that sends a byte now and then
holds a run no longer than the file's size at that speed); a MEGA file is
decrypted as it comes, and the file's head kept as it is written. Waits are
slept, ended by a cancel. A server has 15 seconds to take the connection
and 20 (a page, an API) or 30 (a download) to begin its answer. A web
page's driver cannot watch a download come (a worker's request has one
time limit): an answer has a minute in all there, a download 30 minutes
(`web::DOWNLOAD_MS`; a pack of 300 MB from a host that sends 100 KB a
second does not come in time: download it in a tab and import it).

What another party's answer may cost is bounded where it is read:

- A request says what it fetches (`net::Kind`), and its answer is read no
  further than that may be long, after decoding (a gzip answer of a few
  kilobytes can be gigabytes): a table's header 1 MiB, an API's answer or
  an event's index 2 MiB, a page 8 MiB, a folder's listing or a list of
  links 16 MiB, a table's data 32 MiB (`Kind::limit`: some ten times the
  largest of each measured). A web page's driver, which gets a body whole,
  drops a larger one before it is copied in (`Request::limit`).
- JSON is read into the fields looked at, the rest passed by as it is read
  (`json`, `table::parse_data`), never into a tree of every value: a
  table's data keeps the entries with a hash (200 000 at most), a MEGA
  folder its nodes with a handle (100 000 at most).
- The patterns a venue's labels and a folder's names are read with
  (`text::pattern`) take a number of steps in proportion to the text;
  `text::unescape` looks no further than a reference can be long; a
  work's download box is read to its first 64 KiB, 256 links and each
  label's first 256 characters, a chart's links to 64.

Where a request goes is the link's to say only so far:

- A link (and a file's address a page or an API gives) is taken only as
  every URL parser reads it (`hosts::Link::parse`, the one place a host's
  name is taken from): http(s), no backslash, no control character, no
  user name before the host. `refusal::Refusals::before` asks nothing
  else, whoever asks.
- Natively no host of this machine or its networks is asked (`localhost`,
  loopback, private, link-local, shared and unique-local addresses, by
  name, by the address as written, or by what a name resolves to: the
  agent's resolver), unless `"local_hosts": true` (a mirror on a LAN). A
  proxy set for the requests is the user's own; it resolves the hosts
  asked through it.
- Redirects are followed by `native::http::ask`, ten at most, each
  address kept to the same rules: no local host, none from https to http,
  none to a service that refused in this run. In a web page the proxy (or
  the browser) follows them; where they ended is known only afterwards
  (`X-Final-Url`).
- A MEGA file's bytes are asked over https only, and the file is the one
  its link's key was made for (`hosts::MegaCipher`: the chunks' CBC-MACs
  folded as the key's last 8 bytes say) or no file: natively checked as
  the last bytes come and the file removed, in a page before it is handed
  on (`web::served`).
- A download's file is made anew for it (`create_new`), in a folder of
  the run's own (0700) that keeps it from others while it comes, and with
  the mode any file made there has, as an unpacked one (`link get -o
  file` writes it where it is told): what is at its place is removed,
  never written through; what is neither a file nor a link (a device, a
  pipe) is not removed, and the download fails.

### Refusals

A service that answers 429, or with a browser check (`cf-mitigated:
challenge`; in an answer that failed, a page with "Just a moment...",
`_cf_chl_opt`, Google's "unusual traffic" CAPTCHA, `/sorry/`, or a
Turnstile widget, `challenges.cloudflare.com`), or whose API says it limits
us (MEGA's -4), has refused BMSeeker. So has one that answers 401, 403 or
451 where BMSeeker asks a service's own endpoint: an API, a page, a listing
(BMS SEARCH, Stella, manbow and the venues, Ginger, Konmai, a table's
page).

What a link or a table can make a server say refuses no service
(`refusal::refuses`, `Refusals::after`):

- A 401, 403 or 451 for a file a link leads to, or for a table's header or
  data (they are where the table says), is that file's failure, as its 404
  is: any server has an address that answers so. It is one strike for the
  host that answered. Three strikes in a run (`refusal::STRIKES`), for
  three different addresses of the host, with no answer of that host's
  that came between them, are the host's refusal of its files
  (`Why::Files`, the code `files`, with the last status): it is not asked
  for the rest of a table's files one by one. The same address asked
  again is no new strike, and an answer of the host's that came (a file,
  a page) clears its strikes. So one link, or two, to an address that
  answers 403 refuses nothing.
- A file host (Google Drive, MEGA, Dropbox, MediaFire, pixeldrain, GitHub's
  raw files) gets no strikes and is never refused by 401, 403 or 451,
  whatever was asked and however many files answer so: Drive answers 403
  for each file's quota or sharing.
- An answer that came (a success) is a browser check only by the title a
  challenge page begins with, and only where a page was asked for, or
  where a download was sent as no file (no name) by a host that is no file
  host. What a page shows of others' words, and a file a host stores, make
  no check.
- The service charged is the one that answered: where a download's
  redirects ended (`Unserved::at`, natively too), and for a request a 429
  of another service than the one asked (`refusal::answer_error`, natively
  and through `web::unserved`; Drive's quota said as such).
- A work's page on manbow is asked only at its `event.cgi`.

`refusal::Refusals` keeps it for the run, by its host
(`refusal::service`: `www.` and the port left out; the hosts of Google
Drive, MEGA, Dropbox, MediaFire, pixeldrain and BMS SEARCH as one each).
It lives in the run's `Context` (`refused`, shared by its clones), so
every source, host and job sees it. `refusal::Guarded` keeps a driver's
effects to it (`Refusals::before` and `after`, which a driver that answers
asynchronously calls itself): an effect for a refused service is not done
but fed back `NetError::Refused` at once (no other URL, query or header
tried, no wait; a `Retry-After` is kept to by not coming back in this
run), and an answer that refuses is kept and fed back so. It is no passing
failure, so nothing retries it, and no page (not even with
`failures_are_pages`). A source fed it finds nothing and the next is
asked; a host's link fails and its page's other links are tried.

A transport keeps to no refusals itself for the address asked
(`native::Native`, the web page's): `Guarded` is the one guard, and hands
each new refusal to a callback. Where a redirect leads is asked by the
native transport itself, so there it looks at the run's refusals. The CLI's `Cli::run` wraps its driver so, writing a `refused`
record (also for one a task kept from the run's last answer, MEGA's -4),
and the guard tells the progress (`Progress::refusals`). A download that
its host refused fails with the code `refused`. A run is one CLI process;
`--skip-host HOST` (more than once for more) skips a host that refused in
an earlier run, and the window keeps refusals for its session and hands
them to the next runs so. The `event` commands leave `refused` records
out of the records they read.

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
folder, Dropbox, manbow work (the event mirror first), BMS SEARCH venue
and chart page, MediaFire, MEGA file and folder, browser-only hosts, direct; dead
uploaders first: their mirrors with `third_party`, else a failure, nothing
asked). `Follow` goes on with the
next link (at most a few hops). Adding a host is adding it to the registry.

- A short link (`db.tt`, `bit.ly`) whose redirects end at a host bmseeker
  knows (Dropbox, Google Drive, ...) is opened from there.
- A Google Drive file is asked with `confirm=t`, which passes Drive's
  "can't scan this file for viruses" notice as a browser's "Download
  anyway" does. It is a notice, not an access control (a file's sharing
  and quota still answer 403), and what is downloaded is treated as
  untrusted either way.
- Google Drive's "Quota exceeded" (a file downloaded too often lately) is
  reported as such: it passes within a day.
- **Dead uploaders.** A dead uploader's link (a table's too, tried after
  its other links) is downloaded, with `third_party`, from copies by
  number (`uploaders`); without it it fails at once, nothing asked. By
  default: gnqg (`gnqg.rosx.net/upload/upload.cgi?get=03691`) →
  `gnqg-upload/03691.zip` on bms.hexlataia.xyz, then on darksabun.club,
  then the community archive's gnqg folder; absolute
  (`absolute.pv.land.to/uploader/src/up0650.zip`, its other boards too) →
  darksabun.club's `mirror/absolute/uploader/up0650.zip`. In a copy's
  address `{n}` is the number, `{h}` its hundreds (`36`), `{f}` the link's
  file name and `{d}` its first folder (the board); a Google Drive folder
  link goes down `#sub/folder/file`.

### Sources (strategy)

```rust
trait Source: Send + Sync {
    fn name(&self) -> &'static str;
    fn find(&self, q: &Query) -> BoxTask<Vec<String>>;
    fn checked(&self) -> bool; // what it brings must hold the chart's MD5
}
struct Query { md5: String, want: Want, diff: bool, org_md5: Vec<String> }
```

The sources are asked in the order of `Config::sources`:

| name | source |
| --- | --- |
| `table` | difficulty tables (more with `--also`) |
| `stella` | the Stella uploader's list |
| `tsv` | TSV lists (`--list`; columns `md5`, `url_diff`, `url`) |
| `bmssearch` | [BMS SEARCH](https://bmssearch.net): chart → work → the download links its author gave; a separate chart not tied to a work searched by title and artist |
| `mirrors` | third party: mirrors of dead sites' separate charts, by the chart's file name BMS SEARCH knows, in the `chart_mirrors` folders (by default darksabun's mirror of absolute) |
| `ginger`, `konmai` | third party: the package holding a chart by its MD5, as the BeMusicSeeker fork finds one (gingerrush.com's `downloadURL`, then bms.alvorna.com's `song_url`) |
| `bmslibrary` | third party: the community archive's "BMS Library" (bms.kyouko.moe's `Title (by Artist).rar` by title and artist; asked for separate charts too, as a song's archive often holds them) |
| `uploaders` | third party: dead uploaders' files by number (Hosts, above) |

The third-party sources are asked only with `third_party` (off by
default). What they bring goes into the library only if it holds the chart
(its MD5); a song's archive also when its charts are the song's (title and
artist), the chart wanted being a separate one. The sources are asked
only for a chart whose MD5 is known, for its song or (the song in the
library, as the song's download may have brought it) for the chart
itself. `--own-links`: a song's own links only.

### Into the library

A download is unpacked into a folder of its own (`unpack`), sorted into
packages (`package::split`, `classify`), planned against the `Library`
without touching a file (`install::plan`), then carried out
(`install::apply`: a move that fails leaves the library as it was);
`install::take` does all of it, on any `fs::Store` (a disk, a web page's
storage). `import::import` is the same for a file brought in by hand, and
then places the separate charts that waited for what came in.

- **Packages.** An archive may hold several songs' folders and charts of
  their own; it is split by the BeMusicSeeker fork's rules (a folder whose
  charts' files are there is one, with the folders below; a folder of one
  chart, or of charts of one title and artist as compared below, is one;
  other charts are each one). Where those rules are not sure, the whole
  archive is one.
- **What each package is.** A song new to the library (a folder of its own,
  named after the song's title or the archive, `name (2)` beside another
  song of that name); the library's already (state `owned`: nothing is
  downloaded into a second folder; only the files its folder lacks come
  in, or with `--reinstall` / `"reinstall": true` newer ones over those
  there too); or charts for a song's folder there (state `placed`), which
  go into that folder as they are laid out (its `sound/` folder there
  too). What a song's link brings is a folder of its own unless where it
  goes is sure.
- **What may be replaced.** A download is the song of a library's folder
  only when it holds a chart whose MD5 or SHA-256 the library has in that
  folder (`Library::holding`), and the folder's charts' titles and artists
  are not unlike that chart's in both (`titles::Relation::Unrelated`: an
  MD5 can be made to match; such a chart is not the library's, and its
  package is kept apart). Only then do its newer files replace those of
  their paths there (a chart never), in that folder and below it: a
  package laid out above it (a pack of songs, its folder the pack's)
  replaces nothing of the songs beside it, even brought in again, and a
  copy of a chart the library holds elsewhere (at the pack's or the
  library's top) opens no folder unless it is where the package goes;
  nor does a song's folder open the songs' folders below it (a chart
  kept right in a pack's folder: the pack's songs keep their files). A
  folder on the way to a file that holds a chart is a song's of its own,
  whether the library lists it or not (a name starting with `.`, deeper
  than the library's walks, charts not read yet): only the folders on
  that path are looked at, not the whole library. On
  Windows a path there spelled in another case, on macOS in another case
  or Unicode form, is that folder's (on other systems it is another
  folder), unless the library lists a song's folder of that spelling
  apart (a volume set to tell cases apart): then a file of that
  spelling, or below a folder the library lists so spelled, is that
  other song's. Its charts go only into the folders of its song (those, a
  table's `org_md5`'s, or those its sounds were found in): one it lays
  out anywhere else (beside them, in the pack's folder) waits as a
  separate chart whose song is not known, but for one in a folder the
  library does not have yet and in no other song's folder (a pack's
  update that adds a song): that is a new song's folder, and the chart
  goes there with its files; its other files in another
  song's folder stay out (one that song lacks may be one its charts
  would play). A folder found by its charts' titles and artists, its
  name, its sounds or a table's `org_md5` takes only the files it lacks
  (a chart of a name there beside it, `name_.bms`), unless brought in
  again (`--reinstall`: newer files replace those there; a song's folder
  of its name has them all replaced). Wherever a file is not to replace
  one, a file of its kind (a sound, an image, a movie) whose name is the
  same but for the extension counts as there too: a player that does not
  find `01.wav` plays `01.ogg`, so a `01.wav` brought beside it would
  change what the song plays. In the folders of the song a download is
  (by its charts' hashes) its new files come in as before, `bg.png`
  beside an older `bg.bmp` too. A file's
  time more than a day to come (an archive gives its files any; a day,
  as a zip's local times are read as UTC) is newer than none.
- **Which song's folder**, in this order:
  1. the package's own charts the library has (each votes for its folder;
     two folders with as many votes, as when the library holds a chart
     twice, are never picked between: the chart waits);
  2. the song's own charts a table names (`org_md5`);
  3. the files the charts use and their title and artist (`titles::same_song`,
     below); folders whose title or artist is alike (Dice 0.5) are offered
     first.
- **Waiting.** A separate chart whose song is not in the library yet waits
  in the downloads folder (of a pack of many, only the chart wanted; with
  `table missing --get`, the run's missing charts) and goes into the
  song's folder when the song comes.
- **`table missing --get`** takes each missing chart in turn: one the run
  has brought in already is skipped; when the library has its song (one of
  the song's charts the table names, or a chart of its title and artist),
  its separate chart is downloaded first and the song only if the chart is
  left waiting (the song is never downloaded into a second folder); else
  the song, then its separate chart unless the song brought it.
- **Each file once a run**, whatever link leads to it (Google Drive's
  `gdrive:ID`, MediaFire's, MEGA's, Dropbox's, else the URL without its
  scheme and `#` part; a folder's link with the song looked for in it). A
  file whose download failed is not tried again that run, whatever link
  leads to it.

Whether two charts are of one song is `titles::same_song` (the fork's:
the artists the same and the titles' Dice coefficient at least 0.72, both
normalized: NFKC, the chart's name at the end of the title cut
(`[ANOTHER]`, `-7EX-`, kept up to a `MIX`), the artist's credits for
anything but the music removed (`/ obj:`, `feat.`, `BGA:`, `差分`; not
`remix`, `作曲`)). A chart needs it to go into a folder without asking
(two such folders: it waits). It is the one test of "the same song"
everywhere: reusing a song's folder, whether the library has a table
entry's song, and what a download found by name must hold; a player
asks it as `placement::SongKey` (`SongKey::of(title, artist)`, made once
a chart, then `same_song`). A folder's
charts are one package when their normalized titles and artists are
equal, as the fork splits packages.

The `Library` keeps each song folder's file keys once (for `placement`'s
coverage), the charts by hash, and the charts by their normalized artist
(so a song is looked up among one artist's charts). It is read from the
files (`scan_cached`, in parallel natively, a chart the last scan read
taken from its cache while its time and size are the same: the CLI keeps
it in `<downloads>/scan.json`), folder by folder when first needed
(`new`), or given by a player's records (`from_catalog`: a web page cannot
read its library's charts when it likes).

#### Folder naming

A song new to the library gets a folder named after the song's title (a
table's), or the archive. With `--name-folders` (setting
`"folder_naming": true`; off by default) it is named after its charts as
BeMusicSeeker names one:

- `folder_name_format` (default `[%ARTIST%] %TITLE%`) with what the charts'
  artists and titles share: their chart names such as `[ANOTHER]`,
  prefixes such as `BMS:` and credits such as `/ obj:` trimmed; the best by
  its length^0.65 × how many charts give it.
- Characters a file name cannot have are made full-width; at most 128
  bytes (250 for the whole path) as Shift_JIS counts them; `name (2)`
  beside another song of that name; `新しいフォルダー` when nothing is
  left. `"sjis_only": true` keeps only characters Shift_JIS has (LR2 on
  Windows).
- A folder the archive wraps everything in is left out
  (`[alpha] Song/song.bms`, not `[alpha] Song/Song_v1/song.bms`), and a
  song the library has under another folder name (a chart of the same
  title and artist there) goes into that folder, not a second one.

### Config

All endpoints have defaults in `Config::default()` and can be set by a
JSON file over the defaults (`--config`, `$BMSEEKER_CONFIG`,
`<config folder>/bmseeker/config.json`) or flags:

| key | what |
| --- | --- |
| `user_agent` | BMSeeker's product in the `User-Agent` (below) |
| `mirror_catalog` | the BMS event mirror's list; empty: not used |
| `manbow`, `bmssearch_api`, `stella_list`, `mega_api` | the services' addresses |
| `sources` | which sources, in order (Sources, above) |
| `third_party` | the third-party sources on (`--third-party`); off by default |
| `ginger`, `konmai`, `uploaders`, `chart_mirrors`, `bms_library` | the third-party sources' addresses |
| `tables` | difficulty tables to choose from (the window's list; by default `table::DEFAULT_TABLES`) |
| `downloads` | the folder in a library where downloads wait |
| `min_coverage` | the share of a chart's sounds a song's folder must hold |
| `reinstall` | newer files over those there too (`--reinstall`); a song's folder found by name alone has its files replaced |
| `folder_naming`, `folder_name_format`, `sjis_only` | Folder naming, above (`--name-folders`) |
| `local_hosts` | hosts of this machine and its own networks are asked natively (a mirror on a LAN); off by default |

The `User-Agent` natively is the HTTP library's own name as it sends it
(`config::HTTP_CLIENT`, kept first whatever the setting says), then
BMSeeker's (`config::USER_AGENT`, the `user_agent` setting's default). An
app that embeds it adds its own product after BMSeeker's
(`config::user_agent_with("vibeBMS/0.1.0 (+https://github.com/vibeBMS/vibe)")`:
`ureq/3.5.2 BMSeeker/0.1.0 (+https://github.com/vibeBMS/BMSeeker) vibeBMS/0.1.0 (+https://github.com/vibeBMS/vibe)`
on the wire). The README's "Third-party sources and disclaimer" states
what it is and that it is never a browser's.

## CLI

```text
bmseeker [--json] [--config FILE] [--third-party] [--name-folders] [--skip-host HOST]... <noun> <verb> ...

link resolve <link>... [-t title -a artist]          what a link leads to (a file's URL and name / a page that
                                                     needs a browser / an error), without downloading
link page-script                                     the script a window showing a link's page runs on it (it presses
                                                     a OneDrive share's Download button, on OneDrive's hosts only)
link get <link>... [-o folder|file] [-t -a --md5] [--search] [--any]
                                                     download from the links in turn (into a folder: under the
                                                     file's own name; --search: when all fail, the sources' links
                                                     too (BMS SEARCH ...); --any: a single file that is not a page
                                                     (a chart, a video) too)
song find [-t -a --md5] [--diff] [--table URL]... [--list FILE|URL]...
                                                     the links the sources know for the song (--diff: its separate
                                                     chart)
table show <URL>                                     a difficulty table and its charts per level
table missing <URL> -L library [-l level] [--md5 MD5]... [--sha256 SHA256]... [--chosen FILE|-] [--get [--reinstall]] [--also URL]... [--list ...] [--own-links]
                                                     the charts the library lacks (-l: of that level only; --md5,
                                                     --sha256: of those charts only, named by either hash;
                                                     --chosen: the same, their hashes one a line in a file or
                                                     on stdin, 32 hex digits an MD5 and 64 a SHA-256, anything
                                                     else an error by its line; --get: download them, each song,
                                                     then its separate chart, the chart first when the library
                                                     has its song; each file once)
table export lr2|bmt <URL> <folder>                  an LR2 custom folder / a beatoraja table cache
library import <file> -L library [--name NAME] [--reinstall]
                                                     a downloaded file into the library, and the waiting
                                                     charts it lets be placed (--name: the name it was
                                                     published under, when the file's own is not it)
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
config show                                          the settings in effect: the settings file's (or the defaults)
                                                     with what the command line turns on
```

With `--json`, every command writes JSON Lines (one record a line, as it
goes). The `event` commands always do, and read records from a file or
`-` (stdin): JSON Lines, a JSON array, or an object by key. Notes for
people go to stderr only.

`--overrides` takes work records (JSON or JSONL) whose fields replace the
guess for that `num`.

The `event` commands are the tooling of the event mirror: for example
`bmseeker event works 152 --overrides overrides.json > works.jsonl`,
`bmseeker event check works.jsonl --known links.jsonl --minutes 290 > links.new.jsonl`.
`event check --search`: for a work none of whose links gives an archive,
the links the sources know for it by title and artist (BMS SEARCH; with
`--third-party`, the community archive too) are checked as well and
written with the work's `"num"` and their `"source"`. With
`--third-party`, a failed link that a dead uploader's mirrors cover is
checked again through them (`"mirrored"`).

### Records

```
work   {"num","title","artist","team","kind","size_kb","page",
        "links":[{"label","url"}],"main":[url],"extra":[[url]],"review"?, ...}
check  {"url","state":"archive|chart|page|gone|error","kind"?,"format"?,
        "size","charts","title"?,"code"?,"error"?,"at","num"?,"source"?,"mirrored"?}
link   {"link","state":"file|page|gone|error","host","url"?,"code"?,"error"?}
got    {"file","name","format","link","errors":[...]}
missing {"level","label","title","artist","md5","sha256","url","url_diff"}
done   {"title"?,"md5"?,"sha256"?,"state":"added|owned|placed|waiting|page|failed","path"?,"files"?,"link"?,"code"?,"error"?,"have"?}
refused {"refused": host,"code":"status|files|challenge|rate_limited|earlier","status"?,"error"}
notice {"notice":"third_party","text"}
config {"config":{<every setting, as in the settings file (Config, above)>}}
```

A `done` record's `state` is `install::Done::state` (the first four are
what `install::got` counts as brought in or there already); its `md5` is
the chart's it was for (`table missing --get`; `library install --md5`),
else its `sha256` (`table missing --get` of a table that gives no MD5).
`table missing --get` ends each chart's records with one that has
`"have"`: whether the library has that chart now (by its hash), whatever
came of the downloads on the way (a song's archive `owned` may not hold
the separate chart; a pack an earlier chart brought holds this one). A
chart that needed nothing gets one of its own: `owned` when the library
has it (before the run, or brought by an earlier chart), else `failed`
(`no_link`: nothing to try). With charts named (`--md5`, `--sha256`,
`--chosen`), each named one the library had before the run gets an
`owned` one too, and a hash no entry of the table has a `failed` one
(`not_listed`, its `md5` or `sha256`, no `title`).
A `missing` record's hash it has not is `""` (a table's entry has one at
least: one with neither is no chart, and is not read).
`notice`: what a person is told once a run (with `--json` a record in
place of the note; `third_party`: third parties' sources are on). A
program reading records passes by those it does not know (`event check`
does by `refused` and `notice`).

When a record cannot be written (what reads them is gone: the window
closed), the run is cancelled: no download starts after it.

`testdata/records.jsonl` holds the window's runs as the commands write
them (the command line, what it read on stdin, the lines in order, the
exit code): a test of
`cli` checks them (`BMSEEKER_BLESS=1 cargo test --features cli records`
writes it anew), and `gui/gui_test.go` replays them as the CLI, so a
record the window reads cannot change with both sides' tests green.

`"error"` is for a person; `"code"` for a program: `status`, `stalled`,
`cancelled`, `gone`, `refused`, `other` (the download), `not_the_chart`,
`no_chart_link`, `no_link`, `downloaded` (the install), `unpack`,
`not_listed` (`table missing`: a chart named that the table lists not). Each
refusal is written once, when it comes (in text: `refused: ...`).

## GUI

Go with MyGo, native UI. It runs the CLI (`bmseeker --json <command>`) as
a child process and reads its records. Released as two binaries:
`bmseeker` (CLI) and the GUI, which embeds the CLI: started with a CLI
command it runs as the CLI (same arguments, stdin/stdout, exit code); with
no arguments or a path it opens the window. A link that needs a browser
opens in a MyGo web view, which runs the CLI's script (`link
page-script`: a OneDrive share's Download button pressed, no other
host's) and hands the page's first download back to the CLI (`library
import --name`): kept until then under a name of the window's making, the
page's name for it only said to the CLI.

The CLI it carries is unpacked into the user's cache folder on first use,
for its user alone, and run from there only as the bytes it was built
with (its hash, each start); without a cache folder (no home) it is not
unpacked at all (`$BMSEEKER_CLI` if set; a development build without it
uses `bmseeker` on the PATH). What it starts from is the CLI's settings
(`config show`: the tables to choose from, the downloads folder, whether
third-party sources are on in the settings file), read once the window
is there, and again when Settings is opened or a job starts while they
could not be read; a `downloads` setting that is absolute is that folder,
as the CLI takes it. The charts chosen in a table are downloaded by one
run of `table missing --get --chosen -`, their hashes on its stdin (each
named by its MD5, else its SHA-256; the library's order and batch; a
command line could not hold thousands): the log says each chart's last
record with its place in the run, and a chart leaves the list when that
record's `have` says the library has it. A CLI that ends otherwise than
by its own codes 1 and 2 (a signal, a crash) is told as that, with what
it said last; a line longer than the reader takes (16 MiB) ends the run.
The window ends the CLI runs going on when it quits. A host that refused BMSeeker (a `refused` record) is noted
in the log and skipped by every later run while the window is open
(`--skip-host`). Turning third-party sources on asks first, once (the
CLI's `third_party` notice is then not repeated in the log; when the
settings file turned them on, it is said once a session); the OK is
remembered in `<config folder>/bmseeker/third-party-acknowledged` (in a
web page, the browser's `localStorage`).

Build: `gui/build.sh` (builds the CLI with `cargo build --release --features
cli` into `gui/bin/`, then `go build`; Go 1.27+).

## Web page (`web/`)

The GUI compiled to `js/wasm` (MyGo draws it on a canvas) and the CLI's
commands (`cli`) compiled to wasm in `bmseeker-web`, which runs in a
worker. The GUI's `cli_js.go` speaks the CLI's protocol to it: a command
line and what it reads for `-` (the page's run hands it over) in, JSON
Lines records and notes out as they come, an exit code. A command before
any target is open (a folder waiting for the user's permission) runs on
a session of no files: the settings (`config show`) need none.

The commands are synchronous; a browser's files and network are not,
except in a worker: `FileReaderSync` reads a `File`, and synchronous
`XMLHttpRequest` makes a request (`net::Xhr`, the job's `Driver`, through
vibeBMS's CORS proxy; `?proxy=<url>` uses another, only on a page served
from localhost, as the tests' is). Listing and writing
stay asynchronous, so the worker lists the target into a `Mirror` (each
file's path, size and time; its bytes read when a command reads them),
runs the command on it, then writes what changed (`Mirror::changes`:
removed, moved, written) and settles the mirror. What a command writes is
handed to the worker as each file is done, so a run of many songs does
not hold them all in the page's memory.

Targets: the vibeBMS library in OPFS (`bms/`, written with sync access
handles as vibeBMS writes it; `bmseeker/` for downloads and the scan;
nothing else writable), a folder from `showDirectoryPicker` (its handle
kept in IndexedDB, allowed again with one click on the next visit), or
nothing (a zip of what came in). For vibeBMS: the writes hold the Web Lock
`vibebms-library` alone (vibeBMS lists and reads its library holding it
shared), a file vibeBMS has open is asked again a few times, and the song
folders changed are posted on the `BroadcastChannel` `vibebms-library`,
which open vibeBMS pages list and mount again.

The library builds for wasm32 (the network is `net::Task` and
`net::job::Job`, so the browser makes the requests; `import`,
`install::take` and `unpack` work on whatever `fs::Store` the page keeps
its files in); `unrar-rs` and `delharc` build for it.

Build: `web/build.sh` (into `web/dist`; needs the `wasm32-unknown-unknown`
target and `wasm-bindgen-cli` of the version in `Cargo.lock`). Test:
`node web/test/e2e.mjs web/dist [<vibeBMS web folder>]` (Playwright,
Chromium).

## Releases

Each push to `main` whose checks pass publishes the
[nightly](https://github.com/vibeBMS/BMSeeker/releases/tag/nightly)
prerelease (`.github/workflows/ci.yml`): `bmseeker-<system>` (the
CLI) and `bmseeker-gui-<system>` for `linux-x64`, `linux-arm64`,
`windows-x64`, `windows-arm64` and `macos-universal`, `bmseeker-web.zip`,
and, as vibeBMS's nightly, `latest.json` (each file's version, SHA-256 and
size, per channel) and `builds.json` (every build's). Its version is
`<version>-nightly.<run>+<commit>` (`tools/ci_version.sh`), which
`bmseeker --version`, the window's Settings and the web page's foot show;
a tag `v<version>` releases `<version>` (`.github/workflows/release.yml`).
Local builds are `<version>-dev` (`Cargo.toml`).
