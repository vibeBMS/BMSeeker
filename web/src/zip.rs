//! The "download as zip" target: the mirror's files as one zip, with their
//! times.

use crate::mirror::Mirror;
use std::io::{Cursor, Write};
use std::path::Path;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, DateTime, ZipWriter};

/// Files worth compressing (charts and text); sounds, images and movies
/// are compressed already.
fn compresses(p: &Path) -> bool {
    let ext = p
        .extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    matches!(
        ext.as_str(),
        "bms" | "bme" | "bml" | "pms" | "bmson" | "txt" | "wav" | "bmp" | "json"
    )
}

/// A time (seconds since 1970, UTC) as a zip's.
fn date(t: i64) -> DateTime {
    let days = t.div_euclid(86_400);
    let secs = t.rem_euclid(86_400);
    // days to a civil date (Howard Hinnant's algorithm)
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    DateTime::from_date_and_time(
        year.clamp(1980, 2107) as u16,
        month as u8,
        day as u8,
        (secs / 3600) as u8,
        (secs / 60 % 60) as u8,
        (secs % 60) as u8,
    )
    .unwrap_or_default()
}

/// The mirror's files but those below `skip` (BMSeeker's own folder), as a
/// zip; and the files that could not be read.
pub fn zip(m: &Mirror, skip: &Path) -> std::io::Result<(Vec<u8>, Vec<String>)> {
    let mut z = ZipWriter::new(Cursor::new(Vec::new()));
    let mut unread = Vec::new();
    for (p, bytes, mtime) in m.contents(skip) {
        let name = p.to_string_lossy().trim_start_matches('/').to_string();
        let Ok(bytes) = bytes else {
            unread.push(name);
            continue;
        };
        let method = if compresses(&p) {
            CompressionMethod::Deflated
        } else {
            CompressionMethod::Stored
        };
        let opts = SimpleFileOptions::default()
            .compression_method(method)
            .last_modified_time(date(mtime))
            .large_file(bytes.len() as u64 >= u32::MAX as u64);
        z.start_file(name, opts)?;
        z.write_all(&bytes)?;
    }
    Ok((z.finish()?.into_inner(), unread))
}

#[cfg(test)]
mod tests {
    use super::*;
    use bmseeker::fs::Store;
    use std::io::Read;

    #[test]
    fn zips_the_files_with_their_times() {
        let source: crate::mirror::Source = Box::new(|_| Err(std::io::ErrorKind::NotFound.into()));
        let m = Mirror::new(source, vec!["/".into()], true, || 0);
        // 2020-01-02 03:04:04 UTC (a zip keeps even seconds)
        for (p, body) in [("/曲/a.bms", "#TITLE a"), ("/.bmseeker/scan.json", "[]")] {
            let mut w = m.create(Path::new(p), Some(1_577_934_244)).unwrap();
            w.write_all(body.as_bytes()).unwrap();
        }
        let (bytes, unread) = zip(&m, Path::new("/.bmseeker")).unwrap();
        assert!(unread.is_empty());
        let mut a = zip::ZipArchive::new(Cursor::new(bytes)).unwrap();
        assert_eq!(a.len(), 1);
        let mut f = a.by_index(0).unwrap();
        assert_eq!(f.name().unwrap(), "曲/a.bms");
        let t = f.last_modified().unwrap();
        assert_eq!(
            (
                t.year(),
                t.month(),
                t.day(),
                t.hour(),
                t.minute(),
                t.second()
            ),
            (2020, 1, 2, 3, 4, 4)
        );
        let mut s = String::new();
        f.read_to_string(&mut s).unwrap();
        assert_eq!(s, "#TITLE a");
    }
}
