//! BMS library tools, as a BMS player needs them and a command-line tool
//! (`bmseeker`) offers them:
//!
//! - [`table`]: difficulty tables (bmstable: 発狂BMS, Satellite, Stella, ...);
//! - [`hosts`]: from the link an author or a table gives to the archive
//!   (Google Drive, Dropbox, MediaFire, MEGA, event venues, ...);
//! - [`unpack`] / [`import`]: archives (.zip, .7z, .rar, .lzh) into the library
//!   (a [`fs::Store`]), with their files' times;
//! - [`placement`]: which song's folder a chart of its own (差分) belongs in
//!   (its title and artist compared, NFKC and alike in part);
//! - [`library`]: the library's songs' folders, their files and the charts'
//!   hashes, read once or given by a player's own records;
//! - what a download holds (`package`): songs' folders and charts of their
//!   own, each new to the library, the library's already, or for a song's
//!   folder there;
//! - [`install`]: a table's song from its links into the library (a
//!   download staged, planned, then applied: [`install::take`]);
//! - [`links`]: links for a table's songs whose own are missing or dead;
//! - [`sources`]: where else a song's links are looked for, in order (BMS
//!   SEARCH among them);
//! - with the feature `tools` (the command-line tool's): `export` (a table
//!   as LR2 custom folders or a beatoraja table cache), `probe` and an
//!   event's works (`manbow`).
//!
//! The parts that touch files take an [`fs::Fs`], so a player can keep its
//! library somewhere other than a disk (the vibeBMS web build keeps it in
//! memory). Whatever reads the network is a [`net::Task`] (it asks for
//! requests and is fed the answers): [`hosts::Resolver`] (a link through
//! its hosts' strategies), [`table::TableLoader`], BMS SEARCH's lookup. A
//! download is an [`install::job::Job`] (requests, files downloaded, waits
//! as effects a platform's driver does): [`install::job::Acquire`] takes a
//! link to its file. Natively `native::Native` drives both. Every place
//! read from is in [`config::Config`].

#![warn(unreachable_pub)]

mod bmssearch;
mod chart;
// the command-line tool's commands, on a platform's files and network
#[cfg(feature = "cli")]
pub mod cli;
pub mod config;
#[cfg(feature = "tools")]
pub mod export;
pub mod fs;
pub mod hosts;
pub mod import;
pub mod install;
pub mod library;
pub mod links;
pub mod manbow;
mod naming;
#[cfg(not(target_arch = "wasm32"))]
pub mod native;
pub mod net;
mod package;
pub mod placement;
// downloads and unpacks to a disk
#[cfg(all(feature = "tools", not(target_arch = "wasm32")))]
pub mod probe;
pub mod progress;
pub mod sources;
pub mod table;
#[cfg(any(test, feature = "testkit"))]
pub mod testkit;
mod text;
mod titles;
pub mod unpack;
