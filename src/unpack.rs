//! Archives (.zip, .7z, .rar, .lzh, .tar, .tar.gz) unpacked: into a
//! [`Store`] (a disk, a player's files: [`unpack`]) or handed over member
//! by member ([`unpack_bytes`]). Members by their path (made safe:
//! `safe_rel`), within a budget (an archive can claim, or expand to,
//! anything), with the archive's file times.

use crate::fs::{move_file, safe_part, ReadSeek, Store};
use crate::progress::{Progress, CANCELLED};
use std::cell::Cell;
use std::collections::HashMap;
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

/// Files an archive may hold. A pack of songs within the budget has a
/// few tens of thousands; files beyond counting cost a disk its room for
/// files, whatever their size.
const MEMBERS: usize = 250_000;

/// Folders a member's path may have (a song's files are a few deep).
const DEPTH: usize = 32;

/// Whether memory is short (a web page, a phone).
const SMALL: bool = cfg!(any(
    target_arch = "wasm32",
    target_os = "android",
    target_os = "ios"
));

/// What a 7z's decoder may ask for, in KB: its dictionary or its PPMd
/// model (7-Zip's own settings go to 1 GiB only when asked; its strongest
/// preset takes 64 MiB), and a little beside it.
const SEVENZ_MEMORY_KB: usize = (if SMALL { 256 << 10 } else { 1 << 20 }) + (16 << 10);

/// What reading a 7z's header may take, in KB: its decoder's dictionary
/// (7-Zip's is 1 MiB, other writers' 8 or 16) and the header itself. It
/// lists the files (some hundred bytes each), and the reader keeps a
/// record for as many files as it has bytes before it reads the first: a
/// hundred times its size.
const SEVENZ_HEADER_KB: usize = if SMALL { 9 << 10 } else { 17 << 10 };

/// Bytes a tar's long name or PAX records may have (a path is a few
/// hundred; they are kept in memory).
const TAR_NAME: u64 = 1 << 20;

/// What unpacking fails with.
#[derive(Debug, PartialEq)]
pub(crate) enum Error {
    /// More than the budget (bytes) unpacked.
    TooLarge(u64),
    /// More files than this.
    TooMany(usize),
    /// Stopped by whoever waits for it.
    Cancelled,
    Other(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::TooLarge(budget) => write!(f, "larger than {} MB unpacked", budget >> 20),
            Error::TooMany(n) => write!(f, "more than {n} files"),
            Error::Cancelled => f.write_str(CANCELLED),
            Error::Other(e) => f.write_str(e),
        }
    }
}

impl From<Error> for String {
    fn from(e: Error) -> String {
        e.to_string()
    }
}

/// What reading a 7z failed with, for a person.
fn sevenz_error(e: sevenz_rust2::Error) -> String {
    match e {
        sevenz_rust2::Error::MaxMemLimited { max_kb, .. } => {
            format!("needs more than {} MB of memory to unpack", max_kb >> 10)
        }
        e => e.to_string(),
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

/// `name` from an archive as a relative path that cannot leave its folder,
/// each part a name every file system holds ([`safe_part`]). None: nothing
/// of it is left, or it is more than [`DEPTH`] folders deep.
pub(crate) fn safe_rel(name: &str) -> Option<PathBuf> {
    let parts: Vec<String> = name
        .split(['/', '\\'])
        .filter(|c| !c.is_empty() && *c != "." && *c != "..")
        .filter(|c| !c.contains(':'))
        .filter_map(safe_part)
        .take(DEPTH + 2)
        .collect();
    (1..=DEPTH + 1)
        .contains(&parts.len())
        .then(|| parts.iter().collect())
}

/// Why an unpacking was stopped (told apart from the error it ends with,
/// which the archive's reader may wrap).
#[derive(Clone, Copy)]
enum Stopped {
    TooLarge,
    TooMany,
    Cancelled,
}

/// What an unpacking may still take, and who may stop it.
struct Left<'a> {
    bytes: u64,
    members: usize,
    stop: Option<&'a Progress>,
}

/// A writer that fails past `left` bytes (an archive member larger than it
/// claims) or once the unpacking is cancelled, telling `why`.
struct Capped<'a, W> {
    w: W,
    left: u64,
    stop: Option<&'a Progress>,
    why: &'a Cell<Option<Stopped>>,
}

impl<W: Write> Write for Capped<'_, W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if self.stop.is_some_and(|p| p.cancelled()) {
            self.why.set(Some(Stopped::Cancelled));
            return Err(io::Error::other(CANCELLED));
        }
        if buf.len() as u64 > self.left {
            self.why.set(Some(Stopped::TooLarge));
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

/// Unpack the archive `r` of `format` into `sink`, with at most what is
/// `left` in all (`limit`: the whole's bytes).
fn members(
    r: Box<dyn ReadSeek + Send>,
    format: Format,
    limit: u64,
    left: &mut Left,
    sink: &mut dyn Sink,
) -> Result<(), Error> {
    let why = Cell::new(None);
    each_member(r, format, limit, left, sink, &why).map_err(|e| match why.get() {
        Some(Stopped::TooLarge) => Error::TooLarge(limit),
        Some(Stopped::TooMany) => Error::TooMany(MEMBERS),
        Some(Stopped::Cancelled) => Error::Cancelled,
        None => Error::Other(e),
    })
}

/// One member through the cap: written to `sink` as `to` (its path and
/// time), kept or dropped; with no `to`, passed by (what an archive must
/// decode to reach the next member counts as what it writes does).
fn put(
    left: &mut Left,
    why: &Cell<Option<Stopped>>,
    sink: &mut dyn Sink,
    to: Option<(PathBuf, Option<i64>)>,
    copy: &mut dyn FnMut(&mut dyn Write) -> Result<u64, String>,
) -> Result<(), String> {
    if left.stop.is_some_and(|p| p.cancelled()) {
        why.set(Some(Stopped::Cancelled));
        return Err(CANCELLED.into());
    }
    let Some((rel, mtime)) = to else {
        let mut capped = Capped {
            w: io::sink(),
            left: left.bytes,
            stop: left.stop,
            why,
        };
        copy(&mut capped)?;
        left.bytes = capped.left;
        return Ok(());
    };
    if left.members == 0 {
        why.set(Some(Stopped::TooMany));
        return Err(Error::TooMany(MEMBERS).to_string());
    }
    left.members -= 1;
    let r = {
        let mut w = sink.open(&rel, mtime)?;
        let mut capped = Capped {
            w: &mut w,
            left: left.bytes,
            stop: left.stop,
            why,
        };
        copy(&mut capped)
            .and_then(|n| capped.flush().map(|_| n).map_err(|e| e.to_string()))
            .map(|n| (n, capped.left))
    };
    match r {
        Ok((size, bytes)) => {
            left.bytes = bytes;
            sink.close(Member { rel, size, mtime });
            Ok(())
        }
        Err(e) => {
            sink.drop_partial(&rel);
            Err(e)
        }
    }
}

/// [`members`], telling `why` when it was stopped.
fn each_member(
    r: Box<dyn ReadSeek + Send>,
    format: Format,
    limit: u64,
    left: &mut Left,
    sink: &mut dyn Sink,
    why: &Cell<Option<Stopped>>,
) -> Result<(), String> {
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
                put(left, why, sink, Some((rel, mtime)), &mut |w| {
                    std::io::copy(&mut e, w).map_err(|e| e.to_string())
                })?;
            }
        }
        Format::SevenZ => {
            let mut r = r;
            let none = sevenz_rust2::Password::empty();
            // its header within its own bound first: read with the
            // decoders' (far larger), it could list files without end
            sevenz_rust2::Archive::read_with_memory_limit(&mut r, &none, SEVENZ_HEADER_KB)
                .map_err(sevenz_error)?;
            let mut a =
                sevenz_rust2::ArchiveReader::new_with_memory_limit(r, none, SEVENZ_MEMORY_KB)
                    .map_err(sevenz_error)?;
            // LZMA2 on several threads buffers whole runs of input and
            // output (a 64 MB 7z of 120 MB: 218 MB at its peak on 4 threads,
            // 18 MB on one, 1.7 times as long): one where memory is short
            if SMALL {
                a.set_thread_count(1);
            }
            let mut err = None;
            a.for_each_entries(|entry, r| {
                // (stopped: the blocks after this one are not begun)
                if err.is_some() {
                    return Ok(false);
                }
                if entry.is_directory() {
                    return Ok(true);
                }
                let mtime = entry
                    .has_last_modified_date
                    .then(|| nt_time(entry.last_modified_date().into()))
                    .flatten();
                // (one whose name is not kept is decoded all the same)
                let to = safe_rel(entry.name()).map(|rel| (rel, mtime));
                match put(left, why, sink, to, &mut |w| {
                    std::io::copy(r, w).map_err(|e| e.to_string())
                }) {
                    Ok(()) => Ok(true),
                    Err(e) => {
                        err = Some(e);
                        Ok(false)
                    }
                }
            })
            .map_err(sevenz_error)?;
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
                let mtime = info
                    .mtime
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|d| d.as_secs() as i64);
                // one whose name is not kept: in a solid archive it is
                // decoded for those after it
                let to = safe_rel(&name).map(|rel| (rel, mtime));
                if to.is_none() && !a.is_solid() {
                    continue;
                }
                // capped as it comes (a small archive can expand to anything)
                put(left, why, sink, to, &mut |w| {
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
                        put(left, why, sink, Some((rel, mtime)), &mut |w| {
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
            use std::io::Read;
            let r: Box<dyn Read> = if format == Format::TarGz {
                // a few bytes can decompress to anything: what is passed by
                // (links, folders) counts too, up to twice the budget
                Box::new(Within {
                    // (gzip members one after another are one stream)
                    r: flate2::read::MultiGzDecoder::new(r),
                    left: limit.saturating_mul(2),
                    why,
                })
            } else {
                r
            };
            let mut a = tar::Archive::new(r);
            // The entries as they are: the tar crate would read the entry
            // that gives the next one's long name (GNU) or records (PAX)
            // whole, whatever size it claims. Read here, within a bound.
            let (mut long, mut pax) = (None, None);
            for e in a.entries().map_err(|e| e.to_string())?.raw(true) {
                let mut e = e.map_err(|e| e.to_string())?;
                use tar::EntryType as T;
                let kind = e.header().entry_type();
                if matches!(kind, T::GNULongName | T::XHeader) {
                    let mut b = Vec::new();
                    let read = (&mut e).take(TAR_NAME + 1).read_to_end(&mut b);
                    read.map_err(|e| e.to_string())?;
                    if b.len() as u64 > TAR_NAME {
                        return Err("a name longer than any file's".into());
                    }
                    match kind {
                        T::GNULongName => long = Some(b),
                        _ => pax = Some(b),
                    }
                    continue;
                }
                // (a link's long name goes with its link)
                if kind == T::GNULongLink {
                    continue;
                }
                let (long, pax) = (long.take(), pax.take());
                // a file whose holes are left out of the archive is not
                // read as one here (no chart, sound or image is stored so)
                if kind == T::GNUSparse {
                    return Err("a sparse file in a tar is not supported".into());
                }
                // files only: a link (symbolic or hard) is not unpacked, as
                // its target would be its data, nor a folder or a device
                if !matches!(kind, T::Regular | T::Continuous) {
                    continue;
                }
                let records = || tar::PaxExtensions::new(pax.as_deref().unwrap_or_default());
                let record = |key: &str| {
                    let found = records()
                        .flatten()
                        .find(|x| x.key_bytes() == key.as_bytes());
                    found.map(|x| x.value_bytes())
                };
                // (the size the entry is read by is its header's)
                let size = e.header().size().map_err(|e| e.to_string())?;
                if record("size").is_some_and(|s| s != size.to_string().as_bytes()) {
                    return Err("a file in a tar with two sizes".into());
                }
                let mtime = e.header().mtime().ok().map(|t| t as i64);
                // names are bytes: UTF-8 (a PAX name is), else Shift_JIS or
                // EUC-KR; an old tar's folder ends in `/`
                let header = e.path_bytes();
                let raw: &[u8] = match &long {
                    Some(l) => l.strip_suffix(b"\0").unwrap_or(l),
                    None => record("path").unwrap_or(&header),
                };
                if raw.ends_with(b"/") {
                    continue;
                }
                let Some(rel) = safe_rel(&crate::text::decode_text(raw)) else {
                    continue;
                };
                put(left, why, sink, Some((rel, mtime)), &mut |w| {
                    std::io::copy(&mut e, w).map_err(|e| e.to_string())
                })?;
            }
        }
    }
    Ok(())
}

/// A reader that fails past `left` bytes, telling `why`.
struct Within<'a, R> {
    r: R,
    left: u64,
    why: &'a Cell<Option<Stopped>>,
}

impl<R: io::Read> io::Read for Within<'_, R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.left == 0 && !buf.is_empty() {
            // (the end may be here)
            let mut one = [0u8];
            if self.r.read(&mut one)? == 0 {
                return Ok(0);
            }
            self.why.set(Some(Stopped::TooLarge));
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
    /// Those written (None: one a later member replaced).
    out: Vec<Option<Member>>,
    /// Where in `out` the member of a path is, and of a path but for its
    /// case.
    at: HashMap<PathBuf, usize>,
    folded: HashMap<String, usize>,
}

/// A path as a disk that does not tell case apart compares it.
fn folded(rel: &Path) -> String {
    rel.to_string_lossy().to_lowercase()
}

impl<'a> Into<'a> {
    fn new(store: &'a dyn Store, dest: &'a Path) -> Into<'a> {
        Into {
            store,
            dest,
            out: Vec::new(),
            at: HashMap::new(),
            folded: HashMap::new(),
        }
    }

    /// The members written, in order.
    fn members(self) -> impl Iterator<Item = Member> {
        self.out.into_iter().flatten()
    }
}

impl Sink for Into<'_> {
    fn open(&mut self, rel: &Path, mtime: Option<i64>) -> Result<Box<dyn Write + '_>, String> {
        let p = self.dest.join(rel);
        // a path met before (a tar's file appended again, names alike once
        // made safe, or but for their case on a disk that does not tell
        // them apart): the later member replaces the earlier
        if self.store.stamp(&p).is_some() {
            let at = (self.at.get(rel)).or_else(|| self.folded.get(&folded(rel)));
            if let Some(&i) = at {
                self.out[i] = None;
            }
        }
        self.store
            .create(&p, mtime)
            .map_err(|e| format!("{}: {e}", p.display()))
    }
    fn close(&mut self, m: Member) {
        self.at.insert(m.rel.clone(), self.out.len());
        self.folded.insert(folded(&m.rel), self.out.len());
        self.out.push(Some(m));
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
    Ok(unpack_watched(blob, store, dest, budget, None)?)
}

/// [`unpack`], given up once `stop` is cancelled.
pub(crate) fn unpack_watched(
    blob: Blob,
    store: &dyn Store,
    dest: &Path,
    budget: u64,
    stop: Option<&Progress>,
) -> Result<Vec<Member>, Error> {
    let (format, r) = reader(blob, store).map_err(Error::Other)?;
    unpack_from(r, format, store, dest, budget, stop)
}

/// [`unpack_watched`] of the archive `r` of `format`.
pub(crate) fn unpack_from(
    r: Box<dyn ReadSeek + Send>,
    format: Format,
    store: &dyn Store,
    dest: &Path,
    budget: u64,
    stop: Option<&Progress>,
) -> Result<Vec<Member>, Error> {
    let mut left = Left {
        bytes: budget,
        members: MEMBERS,
        stop,
    };
    let mut sink = Into::new(store, dest);
    members(r, format, budget, &mut left, &mut sink)?;
    let mut out: Vec<Member> = sink.members().collect();
    let charts = out.iter().any(|m| crate::placement::is_chart(&m.rel));
    let nested: Vec<usize> = (0..out.len())
        .filter(|&i| !charts && is_archive(&out[i].rel))
        .collect();
    let mut inner = Vec::new();
    // (those unpacked: taken out once all are, whatever their number)
    let mut gone = vec![false; out.len()];
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
        let mut sink = Into::new(store, &sub);
        let was = (left.bytes, left.members);
        let read = reader(Blob::File(at.clone()), store)
            .map_err(Error::Other)
            .and_then(|(format, r)| members(r, format, budget, &mut left, &mut sink));
        match read {
            Ok(()) => {
                // (it goes: what it took is free again)
                left.bytes += out[i].size;
                left.members += 1;
                gone[i] = true;
                let _ = store.remove(&at);
                inner.extend(sink.members().map(|n| Member {
                    rel: into.join(n.rel),
                    ..n
                }));
            }
            Err(Error::Other(_)) => {
                for n in sink.members() {
                    let _ = store.remove(&sub.join(&n.rel));
                }
                if fresh {
                    store.remove_dir(&sub);
                }
                (left.bytes, left.members) = was;
            }
            Err(e) => return Err(e),
        }
    }
    let mut gone = gone.into_iter();
    out.retain(|_| !gone.next().unwrap_or(false));
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
    let mut left = Left {
        bytes: limit,
        members: MEMBERS,
        stop: None,
    };
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

    /// A solid 7z (LZMA) of `a.bms` ("#TITLE A\n") and `x:y` (a name no
    /// folder can hold: 64 KiB of zeros), in that order.
    const PASSED_7Z: [u8; 251] = [
        55, 122, 188, 175, 39, 28, 0, 4, 23, 84, 47, 179, 187, 0, 0, 0, 0, 0, 0, 0, 32, 0, 0, 0, 0,
        0, 0, 0, 128, 219, 196, 249, 0, 17, 149, 5, 37, 103, 49, 254, 148, 176, 156, 80, 37, 154,
        95, 184, 148, 153, 81, 104, 86, 99, 106, 172, 102, 51, 222, 103, 67, 17, 189, 116, 37, 11,
        30, 227, 70, 63, 230, 29, 10, 70, 6, 220, 42, 210, 213, 33, 180, 2, 157, 60, 35, 6, 33, 62,
        144, 248, 242, 249, 183, 142, 12, 221, 46, 221, 44, 117, 177, 102, 11, 69, 69, 112, 221,
        181, 243, 125, 72, 52, 91, 159, 12, 23, 123, 247, 0, 0, 0, 129, 51, 7, 174, 15, 211, 27,
        194, 189, 64, 192, 144, 211, 67, 196, 225, 249, 232, 176, 11, 188, 222, 126, 34, 253, 226,
        100, 101, 216, 162, 99, 12, 120, 204, 32, 30, 138, 54, 181, 123, 115, 207, 47, 101, 214, 0,
        66, 93, 72, 45, 36, 79, 59, 0, 157, 254, 164, 118, 38, 96, 187, 153, 106, 23, 33, 180, 28,
        152, 215, 174, 4, 15, 119, 220, 234, 109, 214, 92, 225, 96, 190, 254, 80, 114, 118, 114,
        236, 219, 213, 166, 56, 109, 49, 227, 192, 0, 0, 0, 23, 6, 87, 1, 9, 100, 0, 7, 11, 1, 0,
        1, 35, 3, 1, 1, 5, 93, 0, 16, 0, 0, 12, 118, 10, 1, 159, 80, 128, 248, 0, 0,
    ];

    /// What an archive decodes for a member whose name is not kept counts
    /// against the budget as what it writes does: a 7z's (its reader
    /// decodes what is not read), a solid RAR's (each member is decoded
    /// with those before it; `x:y` is 300 bytes, the charts 27).
    #[test]
    fn what_is_passed_by_counts() {
        let within = |z: &[u8], limit| unpack_within(z.to_vec(), limit, |_, _| {});
        assert_eq!(within(&PASSED_7Z, 128 << 10), Ok(1));
        let e = within(&PASSED_7Z, 1 << 10).unwrap_err();
        assert_eq!(e, Error::TooLarge(1 << 10).to_string());
        assert_eq!(within(&SOLID, 400), Ok(2));
        assert_eq!(within(&SOLID, 100), Err(Error::TooLarge(100).to_string()));
    }

    /// `z` (a 7z) with the bytes `from` of its header as `to`, its
    /// checksums made right again.
    fn sevenz_header(z: &[u8], from: &[u8], to: &[u8]) -> Vec<u8> {
        let mut z = z.to_vec();
        let at = |z: &[u8], i: usize| u64::from_le_bytes(z[i..i + 8].try_into().unwrap()) as usize;
        let (start, len) = (32 + at(&z, 12), at(&z, 20));
        let header = &z[start..start + len];
        let i = start + header.windows(from.len()).position(|w| w == from).unwrap();
        z[i..i + to.len()].copy_from_slice(to);
        let crc = |b: &[u8]| {
            let mut c = flate2::Crc::new();
            c.update(b);
            c.sum().to_le_bytes()
        };
        let header = crc(&z[start..start + len]);
        z[28..32].copy_from_slice(&header);
        let first = crc(&z[12..32]);
        z[8..12].copy_from_slice(&first);
        z
    }

    /// A 7z whose decoder asks for a dictionary of 1 GiB for a header of
    /// a few bytes (its LZMA properties: `93`, then the size): refused
    /// before anything is set aside for it.
    #[test]
    fn sevenz_within_its_memory() {
        let same = sevenz_header(&SEVEN, &[93, 0, 16, 0, 0], &[93, 0, 16, 0, 0]);
        assert_eq!(names(same).unwrap().len(), 2);
        let large = sevenz_header(&SEVEN, &[93, 0, 16, 0, 0], &[93, 0, 0, 0, 64]);
        let e = names(large).unwrap_err();
        assert!(e.contains("MB of memory"), "{e}");
    }

    /// Names every file system holds, Windows' too, wherever the archive
    /// is unpacked; a path of more folders than any song's is not one.
    #[test]
    fn names_windows_holds_too() {
        for (name, safe) in [
            ("song/a.bms", Some("song/a.bms")),
            ("song\\a.bms.", Some("song/a.bms")),
            ("song ./a.bms . ", Some("song/a.bms")),
            ("song/CON", Some("song/_CON")),
            ("NUL/com1.wav", Some("_NUL/_com1.wav")),
            ("a/x?y*.wav", Some("a/x_y_.wav")),
            ("a/x:y/b.bms", Some("a/b.bms")),
            ("../../a/./b", Some("a/b")),
            ("C:\\x\\y.bms", Some("x/y.bms")),
            ("skin/Theme/parts/a b.png", Some("skin/Theme/parts/a b.png")),
            ("... /. ", None),
            ("x:y", None),
        ] {
            assert_eq!(safe_rel(name), safe.map(PathBuf::from), "{name:?}");
        }
        let deep = |folders: usize| "d/".repeat(folders) + "a.bms";
        assert!(safe_rel(&deep(DEPTH)).is_some());
        assert!(safe_rel(&deep(DEPTH + 1)).is_none());
        assert!(safe_rel(&deep(100_000)).is_none());
        // a chart is a chart, whatever its name ends with in the archive
        let z = crate::testkit::zip(&[("Song/a.bms.", "#TITLE A"), ("Song/aux.ogg ", "x")]);
        let mem = crate::fs::Mem::default();
        let got = unpack(Blob::Bytes(z), &mem, Path::new("d"), BUDGET).unwrap();
        let rels: Vec<PathBuf> = sorted(got).into_iter().map(|m| m.rel).collect();
        assert_eq!(rels, [PathBuf::from("Song/_aux.ogg"), "Song/a.bms".into()]);
        assert!(crate::placement::is_chart(&rels[1]));
    }

    /// More files than any pack has: none past them is written, whatever
    /// their size; a file come twice counts each time it comes, and is one.
    #[test]
    fn within_the_number_of_files() {
        let files: Vec<(String, &str)> = (0..5).map(|i| (format!("{i}.txt"), "")).collect();
        let files: Vec<(&str, &str)> = files.iter().map(|(n, b)| (n.as_str(), *b)).collect();
        let unpacked = |files_left: usize| {
            let mut left = Left {
                bytes: BUDGET,
                members: files_left,
                stop: None,
            };
            let mem = crate::fs::Mem::default();
            let mut sink = Into::new(&mem, Path::new("d"));
            let z = Box::new(Cursor::new(crate::testkit::zip(&files)));
            let r = members(z, Format::Zip, BUDGET, &mut left, &mut sink);
            (r, sink.members().count())
        };
        assert_eq!(unpacked(5), (Ok(()), 5));
        assert_eq!(unpacked(4), (Err(Error::TooMany(MEMBERS)), 4));
        assert_eq!(
            Error::TooMany(MEMBERS).to_string(),
            "more than 250000 files"
        );
        // 20,000 files, each twice (a tar's file appended again)
        let names: Vec<String> = (0..20_000).map(|i| format!("Song/{i}.wav")).collect();
        let twice: Vec<(&str, &str)> = (names.iter().map(|n| (n.as_str(), "old")))
            .chain(names.iter().map(|n| (n.as_str(), "new")))
            .collect();
        let mem = crate::fs::Mem::default();
        let t = Blob::Bytes(crate::testkit::tar(&twice));
        let got = unpack(t, &mem, Path::new("d"), BUDGET).unwrap();
        assert_eq!(got.len(), 20_000);
        assert_eq!(got[0].rel, PathBuf::from("Song/0.wav"));
        assert_eq!(mem.get(Path::new("d/Song/7.wav")).unwrap().0, b"new");
    }

    /// Cancelled: nothing more is unpacked.
    #[test]
    fn cancelled_while_unpacking() {
        let stop = Progress::new();
        let z = || Blob::Bytes(zip(&[("a.bms", b"#TITLE A"), ("b.bms", b"#TITLE B")]));
        let mem = crate::fs::Mem::default();
        let d = Path::new("d");
        assert_eq!(
            unpack_watched(z(), &mem, d, BUDGET, Some(&stop))
                .unwrap()
                .len(),
            2
        );
        stop.cancel();
        let mem = crate::fs::Mem::default();
        let e = unpack_watched(z(), &mem, d, BUDGET, Some(&stop)).unwrap_err();
        assert_eq!(e, Error::Cancelled);
        assert_eq!(e.to_string(), CANCELLED);
        assert!(mem.0.lock().unwrap().is_empty());
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

    /// A tar's long name (GNU) or records (PAX) are a path's size: one
    /// that claims megabytes is not read into memory. A file whose records
    /// give it another size than its header, or stored with its holes
    /// left out, is not one this reads.
    #[test]
    fn tar_names_within_a_bound() {
        use tar::{EntryType, Header};
        let named = |kind: EntryType, name: &[u8]| {
            let mut t = tar::Builder::new(Vec::new());
            let mut h = Header::new_gnu();
            h.set_entry_type(kind);
            h.set_size(name.len() as u64);
            h.set_path("././@LongLink").unwrap();
            h.set_cksum();
            t.append(&h, name).unwrap();
            let mut h = Header::new_gnu();
            h.set_size(3);
            h.set_path("short.wav").unwrap();
            h.set_cksum();
            t.append(&h, &b"wav"[..]).unwrap();
            t.into_inner().unwrap()
        };
        let long = format!("song/{}.wav\0", "x".repeat(300));
        let t = named(EntryType::GNULongName, long.as_bytes());
        assert_eq!(
            names(t).unwrap(),
            [PathBuf::from(long.trim_end_matches('\0'))]
        );
        let huge = vec![b'x'; TAR_NAME as usize + 1];
        for kind in [EntryType::GNULongName, EntryType::XHeader] {
            let e = names(named(kind, &huge)).unwrap_err();
            assert!(e.contains("a name longer"), "{e}");
        }
        // (a link's long name is passed by, whatever its size)
        assert_eq!(
            names(named(EntryType::GNULongLink, &huge)).unwrap().len(),
            1
        );
        let t = named(EntryType::XHeader, b"19 path=song/a.wav\n9 size=3\n");
        assert_eq!(names(t).unwrap(), [PathBuf::from("song/a.wav")]);
        let t = named(EntryType::XHeader, b"19 path=song/a.wav\n9 size=4\n");
        assert!(names(t).unwrap_err().contains("two sizes"));
        let mut t = tar::Builder::new(Vec::new());
        let mut h = Header::new_gnu();
        h.set_entry_type(EntryType::GNUSparse);
        h.set_size(0);
        h.set_path("holes.bin").unwrap();
        h.set_cksum();
        t.append(&h, &b""[..]).unwrap();
        assert!(names(t.into_inner().unwrap())
            .unwrap_err()
            .contains("sparse"));
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
        let why = Cell::new(None);
        let stop = Progress::new();
        let mut w = Capped {
            w: Vec::new(),
            left: 4,
            stop: Some(&stop),
            why: &why,
        };
        assert!(w.write_all(b"abcd").is_ok() && why.get().is_none());
        assert!(w.write_all(b"e").is_err());
        assert!(matches!(why.get(), Some(Stopped::TooLarge)));
        assert_eq!(w.w, b"abcd");
        // cancelled: nothing more is written, whatever is left
        w.left = 4;
        stop.cancel();
        assert!(w.write_all(b"e").is_err());
        assert!(matches!(why.get(), Some(Stopped::Cancelled)));
        assert_eq!(w.w, b"abcd");
        assert_eq!(
            Error::TooLarge(4 << 20).to_string(),
            "larger than 4 MB unpacked"
        );
    }
}
