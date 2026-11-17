//! BMS library tools, as a BMS player needs them and a command-line tool
//! (`bmseeker`) offers them:
//!
//! - [`table`]: difficulty tables (bmstable: 発狂BMS, Satellite, Stella, ...);
//! - [`hosts`]: from the link an author or a table gives to the archive
//!   (Google Drive, Dropbox, MediaFire, MEGA, event venues, ...);
//! - [`unpack`] / [`import`]: archives (.zip, .7z, .rar) into the library
//!   (a [`fs::Store`]), with their files' times;
//! - [`placement`]: which song's folder a chart of its own (差分) belongs in
//!   (its title and artist compared as [`titles`] does);
//! - [`library`]: the library's songs' folders, their files and the charts'
//!   hashes, read once or given by a player's own records;
//! - [`package`]: what a download holds: songs' folders and charts of their
//!   own, each new to the library, the library's already, or for a song's
//!   folder there;
//! - [`install`]: a table's song from its links into the library (a
//!   download staged, planned, then applied: [`install::plan`]);
//! - [`links`]: links for a table's songs whose own are missing or dead;
//! - [`bmssearch`]: a chart's work and its links from BMS SEARCH;
//! - [`sources`]: where else a song's links are looked for, in order;
//! - [`export`]: a table as LR2 custom folders or a beatoraja table cache.
//!
//! The parts that touch files take an [`fs::Fs`], so a player can keep its
//! library somewhere other than a disk (the vibeBMS web build keeps it in
//! memory). Whatever reads the network is a [`net::Task`] (it asks for
//! requests and is fed the answers): [`hosts::Resolver`] (a link through
//! its [`hosts::Host`] strategies), [`table::TableLoader`],
//! [`bmssearch::Lookup`]; [`net::run`] runs one natively. Every place read
//! from is in [`config::Config`].

pub mod bmssearch;
pub mod chart;
pub mod config;
pub mod export;
pub mod fs;
pub mod hosts;
pub mod import;
pub mod install;
pub mod library;
pub mod links;
pub mod manbow;
pub mod naming;
pub mod net;
pub mod package;
pub mod placement;
// downloads and unpacks to a disk
#[cfg(not(target_arch = "wasm32"))]
pub mod probe;
pub mod progress;
pub mod sources;
pub mod table;
pub mod text;
pub mod titles;
pub mod unpack;

/// A file name from an archive: UTF-8, else Shift_JIS / EUC-KR as BMS packs
/// use.
pub fn decode_name(raw: &[u8]) -> String {
    text::decode_text(raw)
}
