//! What tests build their cases from: archives, and folders of their own
//! that go when done. A player's tests have it with the `testkit` feature.

use std::io::Write;
use std::path::{Path, PathBuf};

/// A zip of `files` (name, contents).
pub fn zip<B: AsRef<[u8]>>(files: &[(&str, B)]) -> Vec<u8> {
    zipped(files, zip::write::SimpleFileOptions::default())
}

/// A zip of `files` (name, contents), each of the time `at`.
pub fn zip_at<B: AsRef<[u8]>>(files: &[(&str, B)], at: zip::DateTime) -> Vec<u8> {
    zipped(
        files,
        zip::write::SimpleFileOptions::default().last_modified_time(at),
    )
}

fn zipped<B: AsRef<[u8]>>(files: &[(&str, B)], o: zip::write::SimpleFileOptions) -> Vec<u8> {
    let mut out = std::io::Cursor::new(Vec::new());
    let mut z = zip::ZipWriter::new(&mut out);
    for (name, body) in files {
        z.start_file(*name, o).unwrap();
        z.write_all(body.as_ref()).unwrap();
    }
    z.finish().unwrap();
    out.into_inner()
}

/// A RAR5 (stored) of `song/a.bms` ("#TITLE A") and `song/01.ogg`
/// ("ogg"), both of 2020-01-02 03:04:05 UTC.
pub const SONG_RAR: [u8; 117] = [
    82, 97, 114, 33, 26, 7, 1, 0, 51, 146, 181, 229, 10, 1, 5, 6, 0, 5, 1, 1, 128, 128, 0, 66, 98,
    211, 59, 32, 2, 2, 136, 0, 6, 136, 0, 164, 131, 2, 165, 93, 13, 94, 221, 184, 73, 19, 128, 0,
    1, 10, 115, 111, 110, 103, 47, 97, 46, 98, 109, 115, 35, 84, 73, 84, 76, 69, 32, 65, 227, 107,
    162, 118, 33, 2, 2, 131, 0, 6, 131, 0, 164, 131, 2, 165, 93, 13, 94, 148, 92, 160, 69, 128, 0,
    1, 11, 115, 111, 110, 103, 47, 48, 49, 46, 111, 103, 103, 111, 103, 103, 29, 119, 86, 81, 3, 5,
    4, 0,
];

/// A folder of a test's own on the disk, empty to begin with, removed
/// (with what is in it) when dropped.
pub struct Tmp(PathBuf);

/// A new [`Tmp`] named after `name` (and this process).
pub fn tmp(name: &str) -> Tmp {
    let d = std::env::temp_dir().join(format!("bmseeker-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    Tmp(d)
}

impl std::ops::Deref for Tmp {
    type Target = Path;
    fn deref(&self) -> &Path {
        &self.0
    }
}

impl AsRef<Path> for Tmp {
    fn as_ref(&self) -> &Path {
        &self.0
    }
}

impl Drop for Tmp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
