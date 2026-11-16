//! Unpacking an archive held in memory (.zip, .7z, .rar), for a library
//! that is not on a disk (the vibeBMS web build's): each member is handed over as a
//! relative path (made safe) and its bytes.

use std::io::{Cursor, Read};
use std::path::PathBuf;

/// What an archive may unpack to (it can claim, or expand to, anything).
const BUDGET: u64 = 2 << 30;

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

/// Unpack `bytes` (its format from its first bytes), giving `put` each file.
/// Returns the number of files.
pub fn unpack_bytes(bytes: Vec<u8>, put: impl FnMut(PathBuf, Vec<u8>)) -> Result<usize, String> {
    unpack_within(bytes, BUDGET, put)
}

/// [`unpack_bytes`] with at most `limit` bytes unpacked in all.
fn unpack_within(
    bytes: Vec<u8>,
    limit: u64,
    mut put: impl FnMut(PathBuf, Vec<u8>),
) -> Result<usize, String> {
    let mut budget = limit;
    let mut take = |rel: PathBuf, r: &mut dyn Read, budget: &mut u64| -> Result<(), String> {
        let mut buf = Vec::new();
        r.take(*budget + 1)
            .read_to_end(&mut buf)
            .map_err(|e| e.to_string())?;
        if buf.len() as u64 > *budget {
            return Err(too_large(limit));
        }
        *budget -= buf.len() as u64;
        put(rel, buf);
        Ok(())
    };
    let mut n = 0;
    match crate::hosts::sniff_bytes(&bytes) {
        Some("zip") => {
            let mut z = zip::ZipArchive::new(Cursor::new(bytes)).map_err(|e| e.to_string())?;
            for i in 0..z.len() {
                let mut e = z.by_index(i).map_err(|e| e.to_string())?;
                if e.is_dir() {
                    continue;
                }
                // names in Shift_JIS / EUC-KR as BMS packs often have
                let name = crate::decode_name(e.name_raw());
                let Some(rel) = safe_rel(&name) else { continue };
                take(rel, &mut e, &mut budget)?;
                n += 1;
            }
        }
        Some("7z") => {
            let mut a = sevenz_rust2::ArchiveReader::new(Cursor::new(bytes), "".into())
                .map_err(|e| e.to_string())?;
            let mut err = None;
            a.for_each_entries(|entry, r| {
                if entry.is_directory() {
                    return Ok(true);
                }
                let Some(rel) = safe_rel(entry.name()) else {
                    return Ok(true);
                };
                match take(rel, r, &mut budget) {
                    Ok(()) => {
                        n += 1;
                        Ok(true)
                    }
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
        Some("rar") => {
            let mut a =
                unrar_rs::RarArchive::open(Cursor::new(bytes)).map_err(|e| e.to_string())?;
            for i in 0..a.len() {
                let Some(info) = a.member_info(i) else {
                    continue;
                };
                if info.is_directory {
                    continue;
                }
                let name = info.raw_name.clone();
                let Some(rel) = safe_rel(&name) else { continue };
                // capped as it comes (a small archive can expand to anything)
                let mut w = Capped {
                    w: Vec::new(),
                    left: budget,
                    budget: limit,
                };
                a.by_index(i)
                    .and_then(|e| e.copy_to(&mut w))
                    .map_err(|e| format!("{name}: {e}"))?;
                take(rel, &mut w.w.as_slice(), &mut budget)?;
                n += 1;
            }
        }
        _ => return Err("not an archive".into()),
    }
    Ok(n)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `VIBEBMS_UNPACK=FILE cargo test -p vibebms unpack_file -- --ignored`
    #[test]
    #[ignore = "needs an archive"]
    fn unpack_file() {
        let f = std::env::var("VIBEBMS_UNPACK").expect("VIBEBMS_UNPACK");
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
