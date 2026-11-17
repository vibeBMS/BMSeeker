//! Archives (.zip, .7z, .rar, .lzh) unpacked: into a [`Store`] (a disk, a
//! player's files: [`unpack`]) or handed over member by member
//! ([`unpack_bytes`]). Members by their path (made safe: [`safe_rel`]),
//! within a budget (an archive can claim, or expand to, anything), with
//! the archive's file times.

use crate::fs::{ReadSeek, Store};
use std::io::{Cursor, Write};
use std::path::{Path, PathBuf};

/// What an archive may unpack to in memory.
const MEMORY: u64 = 2 << 30;

/// What an archive may unpack to: 4 GiB natively, 2 GiB in a web page.
pub const BUDGET: u64 = if cfg!(target_arch = "wasm32") {
    MEMORY
} else {
    4 << 30
};

/// An archive: a file in the store, or bytes.
#[derive(Debug)]
pub enum Blob {
    File(PathBuf),
    Bytes(Vec<u8>),
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
pub fn safe_rel(name: &str) -> Option<PathBuf> {
    let rel: PathBuf = name
        .split(['/', '\\'])
        .filter(|c| !c.is_empty() && *c != "." && *c != "..")
        .filter(|c| !c.contains(':'))
        .collect();
    (!rel.as_os_str().is_empty()).then_some(rel)
}

/// What an archive larger than `budget` unpacked fails with.
pub(crate) fn too_large(budget: u64) -> String {
    format!("larger than {} MB unpacked", budget >> 20)
}

/// A writer that fails past `left` bytes (an archive member larger than it
/// claims), `budget` being the whole's limit (for the message).
pub(crate) struct Capped<W> {
    pub w: W,
    pub left: u64,
    pub budget: u64,
}

impl<W: std::io::Write> std::io::Write for Capped<W> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        if buf.len() as u64 > self.left {
            return Err(std::io::Error::other(too_large(self.budget)));
        }
        let n = self.w.write(buf)?;
        self.left -= n as u64;
        Ok(n)
    }
    fn flush(&mut self) -> std::io::Result<()> {
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

/// Unpack the archive `r` (`format`: "zip", "7z", "rar", "lzh") into `sink`, with
/// at most `budget` bytes in all (`limit`: the whole's, for the message).
fn members(
    r: Box<dyn ReadSeek + Send>,
    format: &str,
    limit: u64,
    budget: &mut u64,
    sink: &mut dyn Sink,
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
                budget: limit,
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
        "zip" => {
            let mut z = zip::ZipArchive::new(r).map_err(|e| e.to_string())?;
            for i in 0..z.len() {
                let mut e = z.by_index(i).map_err(|e| e.to_string())?;
                if e.is_dir() {
                    continue;
                }
                // names in Shift_JIS / EUC-KR as BMS packs often have
                let Some(rel) = safe_rel(&crate::decode_name(e.name_raw())) else {
                    continue;
                };
                let mtime = zip_time(&e);
                put(sink, rel, mtime, &mut |w| {
                    std::io::copy(&mut e, w).map_err(|e| e.to_string())
                })?;
            }
        }
        "7z" => {
            let mut a =
                sevenz_rust2::ArchiveReader::new(r, "".into()).map_err(|e| e.to_string())?;
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
        "rar" => {
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
        "lzh" => {
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
        _ => return Err("not an archive".into()),
    }
    Ok(())
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
    crate::decode_name(&raw)
}

/// The format of the archive `blob` and something to read it with.
fn reader(
    blob: Blob,
    store: &dyn Store,
) -> Result<(&'static str, Box<dyn ReadSeek + Send>), String> {
    use std::io::{Read, Seek, SeekFrom};
    let mut r: Box<dyn ReadSeek + Send> = match blob {
        Blob::Bytes(b) => Box::new(Cursor::new(b)),
        Blob::File(p) => store
            .open(&p)
            .map_err(|e| format!("{}: {e}", p.display()))?,
    };
    let mut head = [0u8; 8];
    let n = r.read(&mut head).map_err(|e| e.to_string())?;
    r.seek(SeekFrom::Start(0)).map_err(|e| e.to_string())?;
    let format = crate::hosts::sniff_bytes(&head[..n]).ok_or("not an archive")?;
    Ok((format, r))
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
        self.store
            .create(&p, mtime)
            .map_err(|e| format!("{}: {e}", p.display()))
    }
    fn close(&mut self, m: Member) {
        self.out.push(m);
    }
    fn drop_partial(&mut self, rel: &Path) {
        self.store.remove(&self.dest.join(rel));
    }
}

/// Whether a file is an archive by its name.
pub fn is_archive(p: &Path) -> bool {
    p.extension().and_then(|e| e.to_str()).is_some_and(|e| {
        matches!(
            e.to_ascii_lowercase().as_str(),
            "zip" | "7z" | "rar" | "lzh" | "lha"
        )
    })
}

/// Unpack the archive `blob` (its format from its first bytes) into the
/// folder `dest` of `store`, at most `budget` bytes in all. An archive
/// holding no chart but archives (a shared folder downloaded as a zip of
/// the song's archives): each of those unpacked into a folder of its name
/// beside it, then removed. Returns the files written. What it wrote before
/// it failed stays (the caller removes the folder).
pub fn unpack(
    blob: Blob,
    store: &dyn Store,
    dest: &Path,
    budget: u64,
) -> Result<Vec<Member>, String> {
    let (format, r) = reader(blob, store)?;
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
        let m = out.remove(i);
        let at = dest.join(&m.rel);
        let into = m.rel.with_extension("");
        let (format, r) = reader(Blob::File(at.clone()), store)?;
        let mut sink = Into {
            store,
            dest: &dest.join(&into),
            out: Vec::new(),
        };
        members(r, format, budget, &mut left, &mut sink)?;
        inner.extend(sink.out.into_iter().map(|n| Member {
            rel: into.join(n.rel),
            ..n
        }));
        store.remove(&at);
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
    let format = crate::hosts::sniff_bytes(&bytes).ok_or("not an archive")?;
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
        let mut out = Cursor::new(Vec::new());
        {
            let mut z = zip::ZipWriter::new(&mut out);
            let o = zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Stored);
            z.start_file("song/a.bms", o).unwrap();
            std::io::Write::write_all(&mut z, b"#TITLE A").unwrap();
            z.start_file("../evil.txt", o).unwrap();
            std::io::Write::write_all(&mut z, b"x").unwrap();
            z.finish().unwrap();
        }
        let mut got = Vec::new();
        let n = unpack_bytes(out.into_inner(), |p, b| got.push((p, b))).unwrap();
        assert_eq!(n, 2);
        assert_eq!(got[0].0, PathBuf::from("song/a.bms"));
        assert_eq!(got[0].1, b"#TITLE A");
        // no way out of the folder
        assert_eq!(got[1].0, PathBuf::from("evil.txt"));
        assert!(unpack_bytes(b"<!DOCTYPE html>".to_vec(), |_, _| {}).is_err());
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

    /// The same as a RAR5 (stored).
    const RAR_TIMES: [u8; 117] = [
        82, 97, 114, 33, 26, 7, 1, 0, 51, 146, 181, 229, 10, 1, 5, 6, 0, 5, 1, 1, 128, 128, 0, 66,
        98, 211, 59, 32, 2, 2, 136, 0, 6, 136, 0, 164, 131, 2, 165, 93, 13, 94, 221, 184, 73, 19,
        128, 0, 1, 10, 115, 111, 110, 103, 47, 97, 46, 98, 109, 115, 35, 84, 73, 84, 76, 69, 32,
        65, 227, 107, 162, 118, 33, 2, 2, 131, 0, 6, 131, 0, 164, 131, 2, 165, 93, 13, 94, 148, 92,
        160, 69, 128, 0, 1, 11, 115, 111, 110, 103, 47, 48, 49, 46, 111, 103, 103, 111, 103, 103,
        29, 119, 86, 81, 3, 5, 4, 0,
    ];

    /// 2020-01-02 03:04:05 UTC.
    const T: i64 = 1_577_934_245;

    /// A zip of `entries`, each of 2020-01-02 03:04:04 (MS-DOS times are
    /// to 2 seconds).
    fn zip(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut out = Cursor::new(Vec::new());
        {
            let mut z = zip::ZipWriter::new(&mut out);
            let at = zip::DateTime::from_date_and_time(2020, 1, 2, 3, 4, 4).unwrap();
            let o = zip::write::SimpleFileOptions::default().last_modified_time(at);
            for (name, data) in entries {
                z.start_file(*name, o).unwrap();
                std::io::Write::write_all(&mut z, data).unwrap();
            }
            z.finish().unwrap();
        }
        out.into_inner()
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
        for (bytes, t) in [(z, T - 1), (SEVEN.to_vec(), T), (RAR_TIMES.to_vec(), T)] {
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
        assert_eq!(crate::hosts::sniff_bytes(&LZH), Some("lzh"));
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

    #[test]
    fn times_kept_on_a_disk() {
        use crate::fs::{Disk, Fs};
        let d = std::env::temp_dir().join(format!("bmseeker-times-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        let file = d.join("x.rar");
        std::fs::write(&file, RAR_TIMES).unwrap();
        let got = unpack(Blob::File(file), &Disk, &d.join("out"), BUDGET).unwrap();
        assert_eq!(got.len(), 2);
        assert_eq!(Disk.stamp(&d.join("out/song/01.ogg")), Some((T, 3)));
        let _ = std::fs::remove_dir_all(&d);
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
        use std::io::Write;
        let mut w = Capped {
            w: Vec::new(),
            left: 4,
            budget: 4 << 20,
        };
        assert!(w.write_all(b"abcd").is_ok());
        let e = w.write_all(b"e").unwrap_err();
        assert_eq!(e.to_string(), "larger than 4 MB unpacked");
        assert_eq!(w.w, b"abcd");
    }
}
