//! Archives (.zip, .7z, .rar, .lzh, .tar, .tar.gz) unpacked: into a
//! [`Store`] (a disk, a player's files: [`unpack`]) or handed over member
//! by member ([`unpack_bytes`]). Members by their path (made safe:
//! `safe_rel`), within a budget (an archive can claim, or expand to,
//! anything), with the archive's file times.

use crate::fs::{move_file, ReadSeek, Store};
use std::cell::Cell;
use std::io::{self, Cursor, Write};
use std::path::{Path, PathBuf};

/// What an archive may unpack to in memory: 2 GiB natively; in a web page,
/// where it sits beside the archive (and its own growth) in a heap of 4 GiB
/// at most that never shrinks, 1 GiB.
const MEMORY: u64 = if cfg!(target_arch = "wasm32") {
    1 << 30
} else {
    2 << 30
};

/// What an archive may unpack to: 4 GiB natively, [`MEMORY`] in a web page.
pub const BUDGET: u64 = if cfg!(target_arch = "wasm32") {
    MEMORY
} else {
    4 << 30
};

/// What unpacking fails with.
#[derive(Debug, PartialEq)]
pub(crate) enum Error {
    /// More than the budget (bytes) unpacked.
    TooLarge(u64),
    Other(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::TooLarge(budget) => write!(f, "larger than {} MB unpacked", budget >> 20),
            Error::Other(e) => f.write_str(e),
        }
    }
}

impl From<Error> for String {
    fn from(e: Error) -> String {
        e.to_string()
    }
}

/// An archive's format, known by its first bytes ([`sniff`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    Zip,
    SevenZ,
    Rar,
    Lzh,
    Tar,
    /// A tar compressed with gzip (`.tar.gz`, `.tgz`).
    TarGz,
}

impl Format {
    /// Its file name extension.
    pub fn ext(self) -> &'static str {
        match self {
            Format::Zip => "zip",
            Format::SevenZ => "7z",
            Format::Rar => "rar",
            Format::Lzh => "lzh",
            Format::Tar => "tar",
            Format::TarGz => "tar.gz",
        }
    }
}

/// How many first bytes of a file [`sniff`] is given (a tar's first
/// header is in them; a gzip's too, compressed).
pub const HEAD: usize = 4096;

/// A tar's first header (512 bytes) holds `ustar` at 257 (`ustar\0` in a
/// POSIX one, `ustar ` in a GNU one).
fn is_tar(h: &[u8]) -> bool {
    h.get(257..262) == Some(b"ustar")
}

/// The archive format of a file's first bytes (a tar's: its first 512; a
/// gzip's is a tar's if what they decompress to is one).
pub fn sniff(h: &[u8]) -> Option<Format> {
    if h.starts_with(b"PK\x03\x04") || h.starts_with(b"PK\x05\x06") {
        Some(Format::Zip)
    } else if h.starts_with(b"Rar!\x1a\x07") {
        Some(Format::Rar)
    } else if h.starts_with(b"7z\xbc\xaf\x27\x1c") {
        Some(Format::SevenZ)
    } else if h.len() >= 7 && h[2] == b'-' && h[6] == b'-' && matches!(&h[3..5], b"lh" | b"lz") {
        // an LHA header: `-lh5-` (its method) after its size and checksum
        Some(Format::Lzh)
    } else if is_tar(h) {
        Some(Format::Tar)
    } else if h.starts_with(&[0x1f, 0x8b]) {
        // (what decompresses before the bytes end, or are not gzip)
        use std::io::Read;
        let mut tar = Vec::with_capacity(512);
        let _ = flate2::read::GzDecoder::new(h)
            .take(512)
            .read_to_end(&mut tar);
        is_tar(&tar).then_some(Format::TarGz)
    } else {
        None
    }
}

/// The first [`HEAD`] bytes `r` reads (fewer if it ends).
fn head(r: &mut dyn std::io::Read) -> std::io::Result<Vec<u8>> {
    use std::io::Read;
    let mut head = Vec::with_capacity(HEAD);
    r.take(HEAD as u64).read_to_end(&mut head)?;
    Ok(head)
}

/// The archive format of the file `p` on a disk, from its first bytes.
pub fn sniff_file(p: &Path) -> Option<Format> {
    sniff(&head(&mut std::fs::File::open(p).ok()?).ok()?)
}

/// Archive file name extensions (`tar.gz`: a name's last two).
pub const EXTS: &[&str] = &["zip", "7z", "rar", "lzh", "lha", "tar", "tgz", "tar.gz"];

/// The file name `name` without its archive extension, if it has one, in
/// any case (`Song.TAR.GZ`: `Song`).
pub fn archive_stem(name: &str) -> Option<&str> {
    EXTS.iter().find_map(|e| {
        let at = name.len().checked_sub(e.len() + 1)?;
        let (stem, ext) = (name.get(..at)?, name.get(at..)?);
        (ext.starts_with('.') && ext[1..].eq_ignore_ascii_case(e) && !stem.is_empty())
            .then_some(stem)
    })
}

/// Whether a file is an archive by its name.
pub(crate) fn is_archive(p: &Path) -> bool {
    p.file_name()
        .is_some_and(|n| archive_stem(&n.to_string_lossy()).is_some())
}

/// An archive: a file in the store, or bytes.
#[derive(Debug)]
pub enum Blob {
    File(PathBuf),
    Bytes(Vec<u8>),
}

impl Blob {
    /// The blob as the file `to` of `store` (a file moved there).
    pub(crate) fn put(self, store: &dyn Store, to: &Path) -> io::Result<()> {
        match self {
            Blob::File(p) => move_file(store, &p, to),
            Blob::Bytes(b) => {
                let mut w = store.create(to, None)?;
                w.write_all(&b)?;
                w.flush()
            }
        }
    }
}

/// A file unpacked: its path below the folder, size, and time in the
/// archive (seconds since 1970; None: not given).
#[derive(Clone, Debug, PartialEq)]
pub struct Member {
    pub rel: PathBuf,
    pub size: u64,
    pub mtime: Option<i64>,
}

/// `name` from an archive as a relative path that cannot leave its folder.
pub(crate) fn safe_rel(name: &str) -> Option<PathBuf> {
    let rel: PathBuf = name
        .split(['/', '\\'])
        .filter(|c| !c.is_empty() && *c != "." && *c != "..")
        .filter(|c| !c.contains(':'))
        .collect();
    (!rel.as_os_str().is_empty()).then_some(rel)
}

/// A writer that fails past `left` bytes (an archive member larger than it
/// claims), telling `over` (the archive's reader may wrap the error).
struct Capped<'a, W> {
    w: W,
    left: u64,
    over: &'a Cell<bool>,
}

impl<W: Write> Write for Capped<'_, W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if buf.len() as u64 > self.left {
            self.over.set(true);
            return Err(io::ErrorKind::FileTooLarge.into());
        }
        let n = self.w.write(buf)?;
        self.left -= n as u64;
        Ok(n)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.w.flush()
    }
}

/// Where members go.
trait Sink {
    /// Somewhere to write the member `rel` to.
    fn open(&mut self, rel: &Path, mtime: Option<i64>) -> Result<Box<dyn Write + '_>, String>;
    /// The member written in full.
    fn close(&mut self, m: Member);
    /// The member could not be written: what came of it goes.
    fn drop_partial(&mut self, rel: &Path);
}

/// Seconds since 1970 of a date and time (no time zone).
fn unix_time(y: i64, mo: i64, d: i64, h: i64, mi: i64, s: i64) -> i64 {
    // days from 1970-01-01 (Howard Hinnant's days_from_civil)
    let y = if mo <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (mo + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    days * 86_400 + h * 3_600 + mi * 60 + s
}

/// A Windows file time (100 ns since 1601) in seconds since 1970.
fn nt_time(t: u64) -> Option<i64> {
    (t > 0).then(|| (t / 10_000_000) as i64 - 11_644_473_600)
}

/// A zip member's time: its exact one (extended timestamp, NTFS) if it has
/// one, else its MS-DOS time (no time zone: taken as UTC).
fn zip_time<R: std::io::Read>(e: &zip::read::ZipFile<'_, R>) -> Option<i64> {
    use zip::ExtraField;
    for f in e.extra_data_fields() {
        match f {
            ExtraField::ExtendedTimestamp(t) => {
                if let Some(m) = t.mod_time() {
                    return Some(m as i64);
                }
            }
            ExtraField::Ntfs(n) => {
                if let Some(m) = nt_time(n.mtime()) {
                    return Some(m);
                }
            }
            _ => {}
        }
    }
    let d = e.last_modified().filter(|d| d.is_valid())?;
    Some(unix_time(
        d.year() as i64,
        d.month() as i64,
        d.day() as i64,
        d.hour() as i64,
        d.minute() as i64,
        d.second() as i64,
    ))
}

/// The zip `r`, refused when a member's local header names another file,
/// or another method or encryption, than the central directory does: a
/// tool that reads the local headers would unpack other files than these.
/// (zip's own check refuses a Shift_JIS name with a `\` byte, as in `ソ`,
/// beside its UTF-8 name.)
fn zip_checked(
    r: Box<dyn ReadSeek + Send>,
) -> Result<zip::ZipArchive<Box<dyn ReadSeek + Send>>, String> {
    let z = zip::ZipArchive::new(r).map_err(|e| e.to_string())?;
    let at = (0..z.len())
        .map(|i| {
            let e = z.by_index_data(i)?;
            Ok((e.central_header_start(), e.header_start()))
        })
        .collect::<zip::result::ZipResult<Vec<_>>>()
        .map_err(|e| e.to_string())?;
    let mut r = z.into_inner();
    for (central, local) in at {
        let c = zip_header(&mut r, central, true).map_err(|e| e.to_string())?;
        let l = zip_header(&mut r, local, false).map_err(|e| e.to_string())?;
        if c != l {
            let name = crate::text::decode_text(&c.name);
            return Err(format!(
                "{name}: its local header differs from the central directory"
            ));
        }
    }
    zip::ZipArchive::new(r).map_err(|e| e.to_string())
}

/// What a zip member's local and central headers must agree on.
#[derive(PartialEq)]
struct ZipHeader {
    /// Encrypted, sizes after the data, UTF-8 name.
    flags: u16,
    method: u16,
    name: Vec<u8>,
    /// The UTF-8 name of the extra field 0x7075.
    utf8_name: Option<Vec<u8>>,
}

/// The header at `at`: a central directory entry, or a local header.
fn zip_header(r: &mut dyn ReadSeek, at: u64, central: bool) -> std::io::Result<ZipHeader> {
    use std::io::SeekFrom;
    let u16_at = |b: &[u8], i: usize| u16::from_le_bytes([b[i], b[i + 1]]);
    // where the flags and the name's length are (the method follows the flags)
    let (len, flags, lens) = if central { (46, 8, 28) } else { (30, 6, 26) };
    let mut fixed = vec![0; len];
    r.seek(SeekFrom::Start(at))?;
    r.read_exact(&mut fixed)?;
    let mut name = vec![0; u16_at(&fixed, lens) as usize];
    let mut extra = vec![0; u16_at(&fixed, lens + 2) as usize];
    r.read_exact(&mut name)?;
    r.read_exact(&mut extra)?;
    let mut utf8_name = None;
    let mut x = &extra[..];
    while x.len() >= 4 {
        let (id, n) = (u16_at(x, 0), u16_at(x, 2) as usize);
        let body = &x[4..(4 + n).min(x.len())];
        if id == 0x7075 {
            utf8_name = Some(body.to_vec());
        }
        x = &x[4 + body.len()..];
    }
    Ok(ZipHeader {
        flags: u16_at(&fixed, flags) & 0x0809,
        method: u16_at(&fixed, flags + 2),
        name,
        utf8_name,
    })
}

/// Unpack the archive `r` of `format` into `sink`, with at most `budget`
/// bytes in all (`limit`: the whole's).
fn members(
    r: Box<dyn ReadSeek + Send>,
    format: Format,
    limit: u64,
    budget: &mut u64,
    sink: &mut dyn Sink,
) -> Result<(), Error> {
    let over = Cell::new(false);
    each_member(r, format, limit, budget, sink, &over).map_err(|e| match over.get() {
        true => Error::TooLarge(limit),
        false => Error::Other(e),
    })
}

/// [`members`], telling `over` when the budget is what failed.
fn each_member(
    r: Box<dyn ReadSeek + Send>,
    format: Format,
    limit: u64,
    budget: &mut u64,
    sink: &mut dyn Sink,
    over: &Cell<bool>,
) -> Result<(), String> {
    // one member: written through the cap, kept or dropped
    let mut put = |sink: &mut dyn Sink,
                   rel: PathBuf,
                   mtime: Option<i64>,
                   copy: &mut dyn FnMut(&mut dyn Write) -> Result<u64, String>|
     -> Result<(), String> {
        let r = {
            let mut w = sink.open(&rel, mtime)?;
            let mut capped = Capped {
                w: &mut w,
                left: *budget,
                over,
            };
            copy(&mut capped)
                .and_then(|n| capped.flush().map(|_| n).map_err(|e| e.to_string()))
                .map(|n| (n, capped.left))
        };
        match r {
            Ok((size, left)) => {
                *budget = left;
                sink.close(Member { rel, size, mtime });
                Ok(())
            }
            Err(e) => {
                sink.drop_partial(&rel);
                Err(e)
            }
        }
    };
    match format {
        Format::Zip => {
            let mut z = zip_checked(r)?;
            for i in 0..z.len() {
                let mut e = z.by_index(i).map_err(|e| e.to_string())?;
                // a link is not unpacked (as a file, its target would be its data)
                if e.is_dir() || e.is_symlink() {
                    continue;
                }
                // names in Shift_JIS / EUC-KR as BMS packs often have
                let Some(rel) = safe_rel(&crate::text::decode_text(e.name_raw())) else {
                    continue;
                };
                let mtime = zip_time(&e);
                put(sink, rel, mtime, &mut |w| {
                    std::io::copy(&mut e, w).map_err(|e| e.to_string())
                })?;
            }
        }
        Format::SevenZ => {
            let mut a =
                sevenz_rust2::ArchiveReader::new(r, "".into()).map_err(|e| e.to_string())?;
            // LZMA2 on several threads buffers whole runs of input and
            // output (a 64 MB 7z of 120 MB: 218 MB at its peak on 4 threads,
            // 18 MB on one, 1.7 times as long): one where memory is short
            if cfg!(any(
                target_arch = "wasm32",
                target_os = "android",
                target_os = "ios"
            )) {
                a.set_thread_count(1);
            }
            let mut err = None;
            a.for_each_entries(|entry, r| {
                if entry.is_directory() {
                    return Ok(true);
                }
                let Some(rel) = safe_rel(entry.name()) else {
                    return Ok(true);
                };
                let mtime = entry
                    .has_last_modified_date
                    .then(|| nt_time(entry.last_modified_date().into()))
                    .flatten();
                match put(sink, rel, mtime, &mut |w| {
                    std::io::copy(r, w).map_err(|e| e.to_string())
                }) {
                    Ok(()) => Ok(true),
                    Err(e) => {
                        err = Some(e);
                        Ok(false)
                    }
                }
            })
            .map_err(|e| e.to_string())?;
            if let Some(e) = err {
                return Err(e);
            }
        }
        Format::Rar => {
            let mut a = unrar_rs::RarArchive::open(r).map_err(|e| e.to_string())?;
            for i in 0..a.len() {
                let Some(info) = a.member_info(i) else {
                    continue;
                };
                if info.is_directory {
                    continue;
                }
                // the name as stored (made safe here, as for the other formats)
                let name = info.raw_name.clone();
                let Some(rel) = safe_rel(&name) else { continue };
                let mtime = info
                    .mtime
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|d| d.as_secs() as i64);
                // capped as it comes (a small archive can expand to anything)
                put(sink, rel, mtime, &mut |w| {
                    a.by_index(i)
                        .and_then(|e| e.copy_to(w))
                        .map_err(|e| format!("{name}: {e}"))
                })?;
            }
        }
        Format::Lzh => {
            let mut a = delharc::LhaDecodeReader::new(r).map_err(|e| e.to_string())?;
            loop {
                let h = a.header();
                if !h.is_directory() {
                    let name = lzh_name(h);
                    let mtime = h.parse_last_modified().to_utc().map(|t| t.timestamp());
                    if let Some(rel) = safe_rel(&name) {
                        if !a.is_decoder_supported() {
                            return Err(format!("{name}: a compression not supported"));
                        }
                        put(sink, rel, mtime, &mut |w| {
                            let n = std::io::copy(&mut a, w).map_err(|e| e.to_string())?;
                            a.crc_check().map_err(|e| format!("{name}: {e}"))?;
                            Ok(n)
                        })?;
                    }
                }
                if !a.next_file().map_err(|e| e.to_string())? {
                    break;
                }
            }
        }
        Format::Tar | Format::TarGz => {
            let r: Box<dyn std::io::Read> = if format == Format::TarGz {
                // a few bytes can decompress to anything: what is passed by
                // (links, folders) counts too, up to twice the budget
                Box::new(Within {
                    // (gzip members one after another are one stream)
                    r: flate2::read::MultiGzDecoder::new(r),
                    left: limit.saturating_mul(2),
                    over,
                })
            } else {
                r
            };
            let mut a = tar::Archive::new(r);
            for e in a.entries().map_err(|e| e.to_string())? {
                let mut e = e.map_err(|e| e.to_string())?;
                use tar::EntryType as T;
                // files only: a link (symbolic or hard) is not unpacked, as
                // its target would be its data, nor a folder or a device
                let kind = e.header().entry_type();
                if !matches!(kind, T::Regular | T::Continuous | T::GNUSparse) {
                    continue;
                }
                let mtime = e.header().mtime().ok().map(|t| t as i64);
                // names are bytes: UTF-8 (a PAX name is), else Shift_JIS or
                // EUC-KR; an old tar's folder ends in `/`
                let raw = e.path_bytes();
                if raw.ends_with(b"/") {
                    continue;
                }
                let Some(rel) = safe_rel(&crate::text::decode_text(&raw)) else {
                    continue;
                };
                put(sink, rel, mtime, &mut |w| {
                    std::io::copy(&mut e, w).map_err(|e| e.to_string())
                })?;
            }
        }
    }
    Ok(())
}

/// A reader that fails past `left` bytes, telling `over`.
struct Within<'a, R> {
    r: R,
    left: u64,
    over: &'a Cell<bool>,
}

impl<R: io::Read> io::Read for Within<'_, R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.left == 0 && !buf.is_empty() {
            // (the end may be here)
            let mut one = [0u8];
            if self.r.read(&mut one)? == 0 {
                return Ok(0);
            }
            self.over.set(true);
            return Err(io::ErrorKind::FileTooLarge.into());
        }
        let max = buf.len().min(self.left.min(usize::MAX as u64) as usize);
        let n = self.r.read(&mut buf[..max])?;
        self.left -= n as u64;
        Ok(n)
    }
}

/// An LHA member's path (names in Shift_JIS as LHA archives from Japan
/// have; its folders' parts kept apart by 0xFF in the header, never a
/// Shift_JIS byte).
fn lzh_name(h: &delharc::LhaHeader) -> String {
    let (mut dir, mut file) = (Vec::new(), None);
    for x in h.iter_extra() {
        match x {
            [1, name @ ..] => file = Some(name.to_vec()),
            [2, path @ ..] => dir = path.to_vec(),
            _ => {}
        }
    }
    let mut raw = dir;
    if !raw.is_empty() {
        raw.push(0xFF);
    }
    raw.extend(file.unwrap_or_else(|| h.filename.to_vec()));
    let raw: Vec<u8> = raw
        .into_iter()
        .take_while(|b| *b != 0)
        .map(|b| if b == 0xFF { b'/' } else { b })
        .collect();
    crate::text::decode_text(&raw)
}

/// A blob opened: an archive (its format, by its first bytes, and
/// something to read it with), or none (the blob as it was).
pub(crate) enum Opened {
    Archive(Format, Box<dyn ReadSeek + Send>),
    Not(Blob),
}

/// `blob` (in `store`, if a file) opened.
pub(crate) fn open(blob: Blob, store: &dyn Store) -> Result<Opened, String> {
    use std::io::{Seek, SeekFrom};
    match blob {
        Blob::Bytes(b) => Ok(match sniff(&b) {
            Some(format) => Opened::Archive(format, Box::new(Cursor::new(b))),
            None => Opened::Not(Blob::Bytes(b)),
        }),
        Blob::File(p) => {
            let failed = |e: io::Error| format!("{}: {e}", p.display());
            let mut r = store.open(&p).map_err(failed)?;
            let head = head(&mut r).map_err(failed)?;
            r.seek(SeekFrom::Start(0)).map_err(failed)?;
            Ok(match sniff(&head) {
                Some(format) => Opened::Archive(format, r),
                None => Opened::Not(Blob::File(p)),
            })
        }
    }
}

/// The format of the archive `blob` and something to read it with.
fn reader(blob: Blob, store: &dyn Store) -> Result<(Format, Box<dyn ReadSeek + Send>), String> {
    match open(blob, store)? {
        Opened::Archive(format, r) => Ok((format, r)),
        Opened::Not(_) => Err("not an archive".into()),
    }
}

/// Members into a store, below a folder.
struct Into<'a> {
    store: &'a dyn Store,
    dest: &'a Path,
    out: Vec<Member>,
}

impl Sink for Into<'_> {
    fn open(&mut self, rel: &Path, mtime: Option<i64>) -> Result<Box<dyn Write + '_>, String> {
        let p = self.dest.join(rel);
        // a path met before (a tar's file appended again, names alike once
        // made safe, or but for their case on a disk that does not tell
        // them apart): the later member replaces the earlier
        if self.store.stamp(&p).is_some() {
            let folded = |r: &Path| r.to_string_lossy().to_lowercase();
            let at = (self.out.iter().position(|m| m.rel == rel))
                .or_else(|| self.out.iter().position(|m| folded(&m.rel) == folded(rel)));
            if let Some(i) = at {
                self.out.remove(i);
            }
        }
        self.store
            .create(&p, mtime)
            .map_err(|e| format!("{}: {e}", p.display()))
    }
    fn close(&mut self, m: Member) {
        self.out.push(m);
    }
    fn drop_partial(&mut self, rel: &Path) {
        let _ = self.store.remove(&self.dest.join(rel));
    }
}

/// Unpack the archive `blob` (its format from its first bytes) into the
/// folder `dest` of `store`, at most `budget` bytes in all. An archive
/// holding no chart but archives (a shared folder downloaded as a zip of
/// the song's archives): each of those unpacked into a folder of its name
/// beside it, then removed (one that cannot be unpacked, but for being too
/// large, is kept as it is). Returns the files written. What it wrote
/// before it failed stays (the caller removes the folder).
pub fn unpack(
    blob: Blob,
    store: &dyn Store,
    dest: &Path,
    budget: u64,
) -> Result<Vec<Member>, String> {
    let (format, r) = reader(blob, store)?;
    Ok(unpack_from(r, format, store, dest, budget)?)
}

/// [`unpack`] of the archive `r` of `format`.
pub(crate) fn unpack_from(
    r: Box<dyn ReadSeek + Send>,
    format: Format,
    store: &dyn Store,
    dest: &Path,
    budget: u64,
) -> Result<Vec<Member>, Error> {
    let mut left = budget;
    let mut sink = Into {
        store,
        dest,
        out: Vec::new(),
    };
    members(r, format, budget, &mut left, &mut sink)?;
    let mut out = sink.out;
    let charts = out.iter().any(|m| crate::placement::is_chart(&m.rel));
    let nested: Vec<usize> = (0..out.len())
        .filter(|&i| !charts && is_archive(&out[i].rel))
        .collect();
    let mut inner = Vec::new();
    for &i in nested.iter().rev() {
        let at = dest.join(&out[i].rel);
        let rel = &out[i].rel;
        let stem = (rel.file_name().map(|n| n.to_string_lossy()))
            .and_then(|n| archive_stem(&n).map(str::to_owned))
            .unwrap_or_else(|| "archive".into());
        let into = rel.with_file_name(stem);
        let sub = dest.join(&into);
        // (a folder of its own, unless the archive has one of that name)
        let fresh = store.read_dir(&sub).is_err() && store.stamp(&sub).is_none();
        let mut sink = Into {
            store,
            dest: &sub,
            out: Vec::new(),
        };
        let was = left;
        let read = reader(Blob::File(at.clone()), store)
            .map_err(Error::Other)
            .and_then(|(format, r)| members(r, format, budget, &mut left, &mut sink));
        match read {
            Ok(()) => {
                // (it goes: what it took is free again)
                left += out.remove(i).size;
                let _ = store.remove(&at);
                inner.extend(sink.out.into_iter().map(|n| Member {
                    rel: into.join(n.rel),
                    ..n
                }));
            }
            Err(e @ Error::TooLarge(_)) => return Err(e),
            Err(Error::Other(_)) => {
                for n in &sink.out {
                    let _ = store.remove(&sub.join(&n.rel));
                }
                if fresh {
                    store.remove_dir(&sub);
                }
                left = was;
            }
        }
    }
    out.extend(inner);
    Ok(out)
}

/// Members handed over with their bytes.
struct Handed<F> {
    buf: Vec<u8>,
    put: F,
    n: usize,
}

impl<F: FnMut(PathBuf, Vec<u8>)> Sink for Handed<F> {
    fn open(&mut self, _: &Path, _: Option<i64>) -> Result<Box<dyn Write + '_>, String> {
        self.buf.clear();
        Ok(Box::new(&mut self.buf))
    }
    fn close(&mut self, m: Member) {
        (self.put)(m.rel, std::mem::take(&mut self.buf));
        self.n += 1;
    }
    fn drop_partial(&mut self, _: &Path) {
        self.buf = Vec::new();
    }
}

/// Unpack `bytes` (its format from its first bytes), giving `put` each file.
/// Returns the number of files.
pub fn unpack_bytes(bytes: Vec<u8>, put: impl FnMut(PathBuf, Vec<u8>)) -> Result<usize, String> {
    unpack_within(bytes, MEMORY, put)
}

/// [`unpack_bytes`] with at most `limit` bytes unpacked in all.
fn unpack_within(
    bytes: Vec<u8>,
    limit: u64,
    put: impl FnMut(PathBuf, Vec<u8>),
) -> Result<usize, String> {
    let format = sniff(&bytes).ok_or("not an archive")?;
    let mut sink = Handed {
        buf: Vec::new(),
        put,
        n: 0,
    };
    let mut left = limit;
    members(
        Box::new(Cursor::new(bytes)),
        format,
        limit,
        &mut left,
        &mut sink,
    )?;
    Ok(sink.n)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `BMSEEKER_UNPACK=FILE cargo test -p bmseeker unpack_file -- --ignored`
    #[test]
    #[ignore = "needs an archive"]
    fn unpack_file() {
        let f = std::env::var("BMSEEKER_UNPACK").expect("BMSEEKER_UNPACK");
        let mut size = 0;
        let n = unpack_bytes(std::fs::read(f).unwrap(), |_, b| size += b.len()).unwrap();
        println!("UNPACKED {n} files, {size} bytes");
    }

    #[test]
    fn zip_in_memory() {
        let z = crate::testkit::zip(&[("song/a.bms", "#TITLE A"), ("../evil.txt", "x")]);
        let mut got = Vec::new();
        let n = unpack_bytes(z, |p, b| got.push((p, b))).unwrap();
        assert_eq!(n, 2);
        assert_eq!(got[0].0, PathBuf::from("song/a.bms"));
        assert_eq!(got[0].1, b"#TITLE A");
        // no way out of the folder
        assert_eq!(got[1].0, PathBuf::from("evil.txt"));
        assert!(unpack_bytes(b"<!DOCTYPE html>".to_vec(), |_, _| {}).is_err());
    }

    /// The bytes `from` as `to` in `z`: in all its headers, or only in the
    /// first (a member's local header).
    fn renamed(z: &[u8], from: &[u8], to: &[u8], first: bool) -> Vec<u8> {
        let mut z = z.to_vec();
        let mut at = 0;
        while let Some(i) = z[at..].windows(from.len()).position(|w| w == from) {
            z[at + i..at + i + from.len()].copy_from_slice(to);
            at += i + from.len();
            if first {
                break;
            }
        }
        z
    }

    fn names(z: Vec<u8>) -> Result<Vec<PathBuf>, String> {
        let mut got = Vec::new();
        unpack_bytes(z, |p, _| got.push(p))?;
        Ok(got)
    }

    /// A zip (stored) of `song/ソ.bms` ("#TITLE A"), its name in Shift_JIS
    /// (`ソ` ending in `\`) and in UTF-8 in its extra field 0x7075.
    const SJIS_UTF8_ZIP: [u8; 170] = [
        80, 75, 3, 4, 10, 0, 0, 0, 0, 0, 34, 24, 34, 80, 221, 184, 73, 19, 8, 0, 0, 0, 8, 0, 0, 0,
        11, 0, 21, 0, 115, 111, 110, 103, 47, 131, 92, 46, 98, 109, 115, 117, 112, 17, 0, 1, 145,
        214, 247, 44, 115, 111, 110, 103, 47, 227, 130, 189, 46, 98, 109, 115, 35, 84, 73, 84, 76,
        69, 32, 65, 80, 75, 1, 2, 30, 3, 10, 0, 0, 0, 0, 0, 34, 24, 34, 80, 221, 184, 73, 19, 8, 0,
        0, 0, 8, 0, 0, 0, 11, 0, 21, 0, 0, 0, 0, 0, 0, 0, 0, 0, 164, 129, 0, 0, 0, 0, 115, 111,
        110, 103, 47, 131, 92, 46, 98, 109, 115, 117, 112, 17, 0, 1, 145, 214, 247, 44, 115, 111,
        110, 103, 47, 227, 130, 189, 46, 98, 109, 115, 80, 75, 5, 6, 0, 0, 0, 0, 1, 0, 1, 0, 78, 0,
        0, 0, 70, 0, 0, 0, 0, 0,
    ];

    /// Names in Shift_JIS (`ソ` ending in `\`, not a folder's end) and
    /// EUC-KR, and in UTF-8 beside them.
    #[test]
    fn zip_names_in_their_encodings() {
        let z = crate::testkit::zip(&[("song/@@.bms", "#TITLE A"), ("song/##.ogg", "ogg")]);
        let z = renamed(
            &renamed(&z, b"@@", &[0x83, 0x5C], false),
            b"##",
            &[0xB0, 0xEE],
            false,
        );
        assert_eq!(
            names(z).unwrap(),
            [PathBuf::from("song/ソ.bms"), PathBuf::from("song/곡.ogg")]
        );
        assert_eq!(
            names(SJIS_UTF8_ZIP.to_vec()).unwrap(),
            [PathBuf::from("song/ソ.bms")]
        );
    }

    /// A member whose local header has another name, or another UTF-8
    /// name, than the central directory says: the zip is refused.
    #[test]
    fn zip_whose_local_header_differs() {
        let z = crate::testkit::zip(&[("song/a.bms", "#TITLE A"), ("song/b.bms", "#TITLE B")]);
        assert_eq!(names(z.clone()).unwrap().len(), 2);
        let e = names(renamed(&z, b"song/b.bms", b"song/c.bms", true)).unwrap_err();
        assert!(e.contains("local header"), "{e}");
        let utf8 = "song/ソ.bms".as_bytes();
        let e = names(renamed(
            &SJIS_UTF8_ZIP,
            utf8,
            "song/ン.bms".as_bytes(),
            true,
        ))
        .unwrap_err();
        assert!(e.contains("local header"), "{e}");
    }

    /// A symbolic link is not unpacked.
    #[test]
    fn zip_links_are_not_unpacked() {
        let mut z = zip::ZipWriter::new(Cursor::new(Vec::new()));
        let o = zip::write::SimpleFileOptions::default();
        z.add_symlink("song/a.bms", "/etc/passwd", o).unwrap();
        z.start_file("song/b.bms", o).unwrap();
        z.write_all(b"#TITLE B").unwrap();
        let z = z.finish().unwrap().into_inner();
        assert_eq!(names(z).unwrap(), [PathBuf::from("song/b.bms")]);
    }

    /// A RAR5 of `a.bms` (8 bytes) and `big.bin` (64 KiB of zeros).
    const RAR: [u8; 149] = [
        82, 97, 114, 33, 26, 7, 1, 0, 202, 11, 12, 141, 12, 1, 5, 8, 0, 7, 1, 1, 128, 128, 128,
        128, 0, 250, 245, 13, 243, 35, 2, 3, 11, 136, 0, 4, 136, 0, 164, 131, 2, 221, 184, 73, 19,
        128, 0, 1, 5, 97, 46, 98, 109, 115, 10, 3, 19, 227, 110, 251, 106, 8, 218, 25, 31, 35, 84,
        73, 84, 76, 69, 32, 65, 172, 21, 94, 38, 39, 2, 3, 11, 152, 128, 0, 4, 128, 128, 4, 164,
        131, 2, 235, 142, 151, 215, 128, 5, 1, 7, 98, 105, 103, 46, 98, 105, 110, 10, 3, 19, 227,
        110, 251, 106, 8, 218, 25, 31, 193, 142, 21, 2, 51, 253, 30, 254, 215, 114, 61, 72, 112,
        253, 175, 240, 255, 237, 254, 11, 252, 23, 248, 0, 29, 119, 86, 81, 3, 5, 4, 0,
    ];

    #[test]
    fn rar_members_within_the_budget() {
        let mut got = Vec::new();
        let n = unpack_bytes(RAR.to_vec(), |p, b| got.push((p, b.len()))).unwrap();
        assert_eq!(n, 2);
        assert_eq!(got[1], (PathBuf::from("big.bin"), 65536));
        // a member past the budget fails as it comes, nothing of it handed over
        let mut got = Vec::new();
        let e = unpack_within(RAR.to_vec(), 1 << 10, |p, _| got.push(p)).unwrap_err();
        assert!(e.contains("larger than"), "{e}");
        assert_eq!(got, [PathBuf::from("a.bms")]);
    }

    /// A solid RAR5 of `x:y` (a name no folder can hold), `a.bms` and
    /// `b.bms`, in that order.
    const SOLID: [u8; 165] = [
        82, 97, 114, 33, 26, 7, 1, 0, 32, 182, 250, 17, 10, 1, 5, 6, 4, 5, 1, 1, 128, 128, 0, 101,
        150, 61, 137, 21, 2, 2, 166, 0, 4, 172, 2, 164, 131, 2, 208, 78, 226, 211, 128, 29, 1, 3,
        120, 58, 121, 195, 186, 35, 35, 0, 35, 250, 51, 221, 226, 168, 94, 74, 208, 112, 65, 209,
        225, 92, 17, 16, 65, 255, 241, 117, 120, 183, 136, 132, 239, 166, 115, 153, 167, 105, 133,
        217, 16, 90, 239, 150, 192, 23, 2, 2, 136, 0, 4, 137, 0, 164, 131, 2, 66, 225, 166, 202,
        192, 29, 1, 5, 97, 46, 98, 109, 115, 70, 25, 5, 234, 18, 24, 243, 192, 176, 75, 146, 187,
        23, 2, 2, 134, 0, 4, 146, 0, 164, 131, 2, 120, 1, 127, 83, 192, 29, 1, 5, 98, 46, 98, 109,
        115, 68, 29, 3, 201, 240, 184, 29, 119, 86, 81, 3, 5, 4, 0,
    ];

    /// A member passed by in a solid RAR (each is unpacked with those
    /// before it): those after it still come.
    #[test]
    fn solid_rar_with_a_member_passed_by() {
        let mut got = Vec::new();
        let n = unpack_bytes(SOLID.to_vec(), |p, b| got.push((p, b))).unwrap();
        assert_eq!(n, 2);
        assert_eq!(got[0], (PathBuf::from("a.bms"), b"#TITLE A\n".to_vec()));
        assert_eq!(got[1].0, PathBuf::from("b.bms"));
    }

    /// A 7z (stored) of `song/a.bms` ("#TITLE A") and `song/01.ogg`
    /// ("ogg"), both of 2020-01-02 03:04:05 UTC, and its folder.
    const SEVEN: [u8; 192] = [
        55, 122, 188, 175, 39, 28, 0, 4, 139, 131, 199, 239, 127, 0, 0, 0, 0, 0, 0, 0, 33, 0, 0, 0,
        0, 0, 0, 0, 120, 120, 69, 236, 111, 103, 103, 35, 84, 73, 84, 76, 69, 32, 65, 0, 0, 129,
        51, 7, 174, 49, 152, 64, 197, 194, 158, 64, 28, 172, 142, 157, 127, 66, 152, 14, 59, 188,
        105, 214, 217, 75, 70, 12, 1, 133, 183, 237, 133, 28, 221, 21, 235, 28, 66, 70, 101, 153,
        182, 211, 199, 123, 121, 147, 236, 183, 8, 174, 154, 170, 241, 98, 115, 51, 117, 249, 230,
        59, 133, 25, 134, 89, 152, 160, 198, 214, 171, 74, 96, 102, 32, 97, 200, 206, 183, 73, 236,
        201, 51, 152, 233, 93, 26, 115, 57, 85, 239, 195, 186, 92, 54, 63, 87, 107, 57, 179, 189,
        240, 131, 35, 221, 101, 125, 66, 96, 65, 43, 63, 224, 0, 0, 23, 6, 11, 1, 9, 116, 0, 7, 11,
        1, 0, 1, 35, 3, 1, 1, 5, 93, 0, 16, 0, 0, 12, 128, 154, 10, 1, 124, 253, 132, 0, 0, 0,
    ];

    /// 2020-01-02 03:04:05 UTC.
    const T: i64 = 1_577_934_245;

    /// A zip of `entries`, each of 2020-01-02 03:04:04 (MS-DOS times are
    /// to 2 seconds).
    fn zip(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let at = zip::DateTime::from_date_and_time(2020, 1, 2, 3, 4, 4).unwrap();
        crate::testkit::zip_at(entries, at)
    }

    fn sorted(mut m: Vec<Member>) -> Vec<Member> {
        m.sort_by(|a, b| a.rel.cmp(&b.rel));
        m
    }

    #[test]
    fn formats_into_a_store_with_their_times() {
        use crate::fs::Mem;
        let song = |t: i64| {
            vec![
                Member {
                    rel: "song/01.ogg".into(),
                    size: 3,
                    mtime: Some(t),
                },
                Member {
                    rel: "song/a.bms".into(),
                    size: 8,
                    mtime: Some(t),
                },
            ]
        };
        let z = zip(&[("song/a.bms", b"#TITLE A"), ("song/01.ogg", b"ogg")]);
        let files = [("song/a.bms", "#TITLE A"), ("song/01.ogg", "ogg")];
        for (bytes, t) in [
            (z, T - 1),
            (SEVEN.to_vec(), T),
            (crate::testkit::SONG_RAR.to_vec(), T),
            (crate::testkit::tar(&files), T),
            (crate::testkit::tar_gz(&files), T),
        ] {
            let mem = Mem::default();
            let got = unpack(Blob::Bytes(bytes), &mem, Path::new("d"), BUDGET).unwrap();
            assert_eq!(sorted(got), song(t));
            assert_eq!(
                mem.get(Path::new("d/song/a.bms")),
                Some((b"#TITLE A".to_vec(), t))
            );
        }
        // the same from a file in the store
        let mem = Mem::default();
        mem.put(Path::new("dl/x.7z"), &SEVEN, 0);
        let got = unpack(Blob::File("dl/x.7z".into()), &mem, Path::new("d"), BUDGET).unwrap();
        assert_eq!(got.len(), 2);
        assert!(unpack(
            Blob::Bytes(b"<html>".to_vec()),
            &mem,
            Path::new("d"),
            BUDGET
        )
        .is_err());
    }

    /// An LHA (level 0, stored) of `song\a.bms` ("#TITLE A") and
    /// `song\ソ.ogg` ("ogg", its name in Shift_JIS, `ソ` ending in `\`),
    /// both of 2020-01-02 03:04:04.
    const LZH: [u8; 81] = [
        32, 38, 45, 108, 104, 48, 45, 8, 0, 0, 0, 8, 0, 0, 0, 130, 24, 34, 80, 32, 0, 10, 115, 111,
        110, 103, 92, 97, 46, 98, 109, 115, 182, 232, 35, 84, 73, 84, 76, 69, 32, 65, 33, 26, 45,
        108, 104, 48, 45, 3, 0, 0, 0, 3, 0, 0, 0, 130, 24, 34, 80, 32, 0, 11, 115, 111, 110, 103,
        92, 131, 92, 46, 111, 103, 103, 91, 199, 111, 103, 103, 0,
    ];

    #[test]
    fn lzh() {
        use crate::fs::Mem;
        assert_eq!(sniff(&LZH), Some(Format::Lzh));
        assert!(is_archive(Path::new("A.LHA")) && !is_archive(Path::new("a.txt")));
        let mem = Mem::default();
        let got = unpack(Blob::Bytes(LZH.to_vec()), &mem, Path::new("d"), BUDGET).unwrap();
        assert_eq!(
            got,
            [
                Member {
                    rel: "song/a.bms".into(),
                    size: 8,
                    mtime: Some(T - 1)
                },
                Member {
                    rel: "song/ソ.ogg".into(),
                    size: 3,
                    mtime: Some(T - 1)
                }
            ]
        );
        assert_eq!(mem.get(Path::new("d/song/ソ.ogg")).unwrap().0, b"ogg");
        // a member that is not what its checksum says
        let mut bad = LZH.to_vec();
        bad[40] = b'X';
        assert!(unpack(Blob::Bytes(bad), &mem, Path::new("e"), BUDGET).is_err());
        let mut n = 0;
        assert_eq!(unpack_bytes(LZH.to_vec(), |_, _| n += 1), Ok(2));
    }

    /// An archive in an archive that fails once some of it is written
    /// (its second member is not what its checksum says): nothing of it is
    /// left, not even its folder.
    #[test]
    fn a_nested_archive_that_fails_midway() {
        use crate::fs::Disk;
        let mut bad = LZH.to_vec();
        bad[77] = b'X';
        let outer = zip(&[("Song.lzh", &bad[..]), ("readme.txt", b"hi")]);
        let d = crate::testkit::tmp("nested-bad");
        let got = unpack(Blob::Bytes(outer), &Disk, &d, BUDGET).unwrap();
        let rels: Vec<PathBuf> = sorted(got).into_iter().map(|m| m.rel).collect();
        assert_eq!(rels, [PathBuf::from("Song.lzh"), "readme.txt".into()]);
        assert!(!d.join("Song").exists());
    }

    #[test]
    fn tar_by_its_bytes_and_its_name() {
        use crate::testkit::{gzip, tar, tar_gz};
        let t = tar(&[("a.bms", "#TITLE A")]);
        assert_eq!(sniff(&t), Some(Format::Tar));
        assert_eq!(
            sniff(&tar_gz(&[("a.bms", "#TITLE A")])),
            Some(Format::TarGz)
        );
        // a gzip of something else (a beatoraja table cache), or cut short
        assert_eq!(sniff(&gzip(b"{\"name\": \"table\"}")), None);
        assert_eq!(sniff(&gzip(&t)[..12]), None);
        assert_eq!(sniff(&t[..200]), None);
        for (name, stem) in [
            ("Song.tar.gz", Some("Song")),
            ("Song.TGZ", Some("Song")),
            ("a.b.tar", Some("a.b")),
            ("x.zip", Some("x")),
            ("x.gz", None),
            (".tar.gz", None),
            ("tar", None),
        ] {
            assert_eq!(archive_stem(name), stem, "{name}");
            assert_eq!(is_archive(Path::new(name)), stem.is_some(), "{name}");
        }
        assert_eq!(Format::TarGz.ext(), "tar.gz");
    }

    /// A tar's names kept in its folder; links, folders and devices not
    /// unpacked; names in Shift_JIS, long ones (GNU, PAX).
    #[test]
    fn tar_members() {
        use tar::{EntryType, Header};
        let mut t = tar::Builder::new(Vec::new());
        let put = |t: &mut tar::Builder<Vec<u8>>,
                   kind: EntryType,
                   name: &[u8],
                   link: &[u8],
                   body: &[u8]| {
            let mut h = Header::new_gnu();
            h.set_entry_type(kind);
            h.set_size(body.len() as u64);
            h.as_old_mut().name[..name.len()].copy_from_slice(name);
            h.as_old_mut().linkname[..link.len()].copy_from_slice(link);
            h.set_cksum();
            t.append(&h, body).unwrap();
        };
        put(&mut t, EntryType::Directory, b"song/", b"", b"");
        put(&mut t, EntryType::Regular, b"song/a.bms", b"", b"#TITLE A");
        put(&mut t, EntryType::Regular, b"../../evil.txt", b"", b"x");
        put(&mut t, EntryType::Regular, b"/etc/abs.txt", b"", b"y");
        put(
            &mut t,
            EntryType::Symlink,
            b"song/b.bms",
            b"/etc/passwd",
            b"",
        );
        put(&mut t, EntryType::Link, b"song/c.bms", b"song/a.bms", b"");
        // (data a link should not have: passed by)
        put(&mut t, EntryType::Symlink, b"song/d.bms", b"x", b"#TITLE D");
        put(&mut t, EntryType::Char, b"song/tty", b"", b"");
        put(&mut t, EntryType::Fifo, b"song/fifo", b"", b"");
        put(&mut t, EntryType::Regular, b"old/", b"", b"");
        put(
            &mut t,
            EntryType::Regular,
            &[b's', b'/', 0x83, 0x5C, b'.', b'o', b'g', b'g'],
            b"",
            b"ogg",
        );
        let long = format!("song/{}.wav", "x".repeat(150));
        let mut h = Header::new_gnu();
        h.set_size(3);
        t.append_data(&mut h, &long, &b"wav"[..]).unwrap();
        let pax = format!("song/{}/ソ.bmp", "y".repeat(120));
        t.append_pax_extensions([("path", pax.as_bytes())]).unwrap();
        put(&mut t, EntryType::Regular, b"short.bmp", b"", b"bmp");
        let t = t.into_inner().unwrap();
        let expect = [
            PathBuf::from("song/a.bms"),
            "evil.txt".into(),
            "etc/abs.txt".into(),
            "s/ソ.ogg".into(),
            long.into(),
            pax.into(),
        ];
        assert_eq!(names(t.clone()).unwrap(), expect);
        assert_eq!(names(crate::testkit::gzip(&t)).unwrap(), expect);
        let mut got = Vec::new();
        unpack_bytes(t, |p, b| got.push((p, b))).unwrap();
        assert_eq!(got[0].1, b"#TITLE A");
    }

    #[test]
    fn tar_within_the_budget() {
        use crate::testkit::{gzip, tar, tar_gz};
        let big = vec![0u8; 64 << 10];
        let files = [("a.bms", &b"#TITLE A"[..]), ("big.bin", &big)];
        for t in [tar(&files), tar_gz(&files)] {
            let mut got = Vec::new();
            let e = unpack_within(t, 1 << 10, |p, _| got.push(p)).unwrap_err();
            assert!(e.contains("larger than"), "{e}");
            assert_eq!(got, [PathBuf::from("a.bms")]);
        }
        // a gzip expanding to more than twice the budget in what is not
        // unpacked (a link's data) fails too, as it comes
        let mut t = tar::Builder::new(Vec::new());
        let mut h = tar::Header::new_gnu();
        h.set_entry_type(tar::EntryType::Symlink);
        h.set_size(big.len() as u64);
        h.set_path("x").unwrap();
        h.set_cksum();
        t.append(&h, &big[..]).unwrap();
        let t = t.into_inner().unwrap();
        assert_eq!(unpack_within(t.clone(), 1 << 10, |_, _| {}), Ok(0));
        let e = unpack_within(gzip(&t), 1 << 10, |_, _| {}).unwrap_err();
        assert_eq!(e, Error::TooLarge(1 << 10).to_string());
        assert_eq!(unpack_within(gzip(&t), 64 << 10, |_, _| {}), Ok(0));
    }

    /// A .tar.gz in an archive holding no chart: into a folder of its name
    /// without `.tar.gz`.
    #[test]
    fn tar_gz_in_an_archive() {
        use crate::fs::Mem;
        let inner = crate::testkit::tar_gz(&[("Song/song.bms", "#TITLE A")]);
        let outer = zip(&[("Song_ogg.tar.gz", &inner)]);
        let mem = Mem::default();
        let got = unpack(Blob::Bytes(outer), &mem, Path::new("d"), BUDGET).unwrap();
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].rel, PathBuf::from("Song_ogg/Song/song.bms"));
        assert_eq!(got[0].mtime, Some(T));
        assert!(mem.get(Path::new("d/Song_ogg.tar.gz")).is_none());
    }

    #[test]
    fn times_kept_on_a_disk() {
        use crate::fs::{Disk, Fs};
        let d = crate::testkit::tmp("times");
        let file = d.join("x.rar");
        std::fs::write(&file, crate::testkit::SONG_RAR).unwrap();
        let got = unpack(Blob::File(file), &Disk, &d.join("out"), BUDGET).unwrap();
        assert_eq!(got.len(), 2);
        assert_eq!(Disk.stamp(&d.join("out/song/01.ogg")), Some((T, 3)));
    }

    #[test]
    fn archives_in_an_archive_without_charts() {
        use crate::fs::Mem;
        let inner = zip(&[("Song/song.bms", b"#TITLE A"), ("Song/01.ogg", b"ogg")]);
        let outer = zip(&[("Song_ogg.zip", &inner), ("readme.txt", b"hi")]);
        let mem = Mem::default();
        let got = unpack(Blob::Bytes(outer), &mem, Path::new("d"), BUDGET).unwrap();
        let rels: Vec<PathBuf> = sorted(got).into_iter().map(|m| m.rel).collect();
        assert_eq!(
            rels,
            [
                PathBuf::from("Song_ogg/Song/01.ogg"),
                "Song_ogg/Song/song.bms".into(),
                "readme.txt".into()
            ]
        );
        assert!(mem.get(Path::new("d/Song_ogg.zip")).is_none());
        // with a chart of its own: they are what it holds
        let outer = zip(&[("Song_bga.zip", &inner), ("song.bms", b"#TITLE B")]);
        let mem = Mem::default();
        let got = unpack(Blob::Bytes(outer), &mem, Path::new("d"), BUDGET).unwrap();
        assert_eq!(got.len(), 2);
        assert!(mem.get(Path::new("d/Song_bga.zip")).is_some());
    }

    /// An archive in an archive that is none (or cannot be read) stays as
    /// it is; one unpacked no longer counts against the budget.
    #[test]
    fn archives_in_an_archive_that_are_not() {
        use crate::fs::Mem;
        let inner = zip(&[("Song/song.bms", b"#TITLE A")]);
        let outer = zip(&[("Song.zip", &inner), ("notes.zip", b"not a zip")]);
        let mem = Mem::default();
        let got = unpack(Blob::Bytes(outer), &mem, Path::new("d"), BUDGET).unwrap();
        let rels: Vec<PathBuf> = sorted(got).into_iter().map(|m| m.rel).collect();
        assert_eq!(
            rels,
            [PathBuf::from("Song/Song/song.bms"), "notes.zip".into()]
        );
        assert_eq!(mem.get(Path::new("d/notes.zip")).unwrap().0, b"not a zip");
        // two of 40 KiB (that do not compress) in 130 KiB: the second fits
        // once the first is gone
        let noise = |seed: u32| -> Vec<u8> {
            let next = |x: &mut u32, _| {
                *x = x.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                Some((*x >> 24) as u8)
            };
            (0..40 << 10).scan(seed, next).collect()
        };
        let a = zip(&[("a.bin", &noise(1))]);
        let b = zip(&[("b.bin", &noise(2))]);
        let outer = zip(&[("a.zip", &a), ("b.zip", &b)]);
        let got = unpack(
            Blob::Bytes(outer),
            &Mem::default(),
            Path::new("d"),
            130 << 10,
        );
        assert_eq!(got.unwrap().len(), 2);
    }

    #[test]
    fn within_the_budget() {
        use crate::fs::Mem;
        let big = vec![0u8; 64 << 10];
        let z = zip(&[("a.bms", b"#TITLE A"), ("big.bin", &big)]);
        let mem = Mem::default();
        let e = unpack(Blob::Bytes(z), &mem, Path::new("d"), 1 << 10).unwrap_err();
        assert!(e.contains("larger than"), "{e}");
        // what was written in full stays, the member past it goes
        assert!(mem.get(Path::new("d/a.bms")).is_some());
        assert!(mem.get(Path::new("d/big.bin")).is_none());
        // the budget is the whole's: the nested archives' too
        let inner = zip(&[("big.bin", &big)]);
        let pad = vec![0u8; 10 << 10];
        let outer = zip(&[("in.zip", &inner), ("pad.bin", &pad)]);
        let e = unpack(
            Blob::Bytes(outer),
            &Mem::default(),
            Path::new("d"),
            70 << 10,
        );
        assert!(e.unwrap_err().contains("larger than"));
    }

    #[test]
    fn dos_and_windows_times() {
        assert_eq!(unix_time(1970, 1, 1, 0, 0, 0), 0);
        assert_eq!(unix_time(2020, 1, 2, 3, 4, 5), T);
        assert_eq!(unix_time(2000, 3, 1, 0, 0, 0), 951_868_800);
        assert_eq!(nt_time(116_444_736_000_000_000), Some(0));
        assert_eq!(nt_time(0), None);
    }

    #[test]
    fn capped_writer() {
        let over = Cell::new(false);
        let mut w = Capped {
            w: Vec::new(),
            left: 4,
            over: &over,
        };
        assert!(w.write_all(b"abcd").is_ok() && !over.get());
        assert!(w.write_all(b"e").is_err() && over.get());
        assert_eq!(w.w, b"abcd");
        assert_eq!(
            Error::TooLarge(4 << 20).to_string(),
            "larger than 4 MB unpacked"
        );
    }
}
