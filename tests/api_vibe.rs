//! Every BMSeeker item vibeBMS (the player built on BMSeeker) names, named
//! once: a rename, a move without a `pub use` at the old path, or a
//! narrower visibility fails here, in BMSeeker's own tests. A change to
//! this file means vibe must change too. Made by
//! `python3 tools/api_vibe.py <vibeBMS checkout> > tests/api_vibe.rs`
//! (it names; it does not check signatures).
#![allow(unused_imports)]

// (only their types: with type parameters of their own, they cannot be
// named without a call) bmseeker::library::Library::from_catalog

use bmseeker::config::native_user_agent as _;
use bmseeker::config::user_agent_with as _;
use bmseeker::config::Config as _;
use bmseeker::config::HTTP_CLIENT as _;
use bmseeker::config::USER_AGENT as _;
use bmseeker::fs::file_name as _;
use bmseeker::fs::Disk as _;
use bmseeker::fs::Entry as _;
use bmseeker::fs::Fs as _;
use bmseeker::fs::ReadSeek as _;
use bmseeker::fs::Store as _;
use bmseeker::hosts::Context as _;
use bmseeker::hosts::Fetch as _;
use bmseeker::hosts::Target as _;
use bmseeker::hosts::Want as _;
use bmseeker::hosts::PRESS_DOWNLOAD_JS as _;
use bmseeker::hosts::STALL_MS as _;
use bmseeker::import::import as _;
use bmseeker::import::unpack_into as _;
use bmseeker::import::Imported as _;
use bmseeker::install::got as _;
use bmseeker::install::job::run_job as _;
use bmseeker::install::job::Driver as _;
use bmseeker::install::job::Effect as _;
use bmseeker::install::job::Fed as _;
use bmseeker::install::job::FileLink as _;
use bmseeker::install::job::Files as _;
use bmseeker::install::job::Installed as _;
use bmseeker::install::job::Job as _;
use bmseeker::install::job::Missed as _;
use bmseeker::install::job::Step as _;
use bmseeker::install::job::TaskJob as _;
use bmseeker::install::job::Unserved as _;
use bmseeker::install::md5_hex as _;
use bmseeker::install::request as _;
use bmseeker::install::safe_name as _;
use bmseeker::install::take as _;
use bmseeker::install::Done as _;
use bmseeker::install::Entry as _;
use bmseeker::install::Failure as _;
use bmseeker::library::Library as _;
use bmseeker::library::Owned as _;
use bmseeker::links::Links as _;
#[cfg(not(target_arch = "wasm32"))]
use bmseeker::native::fetch_files as _;
#[cfg(not(target_arch = "wasm32"))]
use bmseeker::native::Native as _;
use bmseeker::net::too_large as _;
use bmseeker::net::Answer as _;
use bmseeker::net::Kind as _;
use bmseeker::net::NetError as _;
use bmseeker::net::NetError::Cancelled as _;
use bmseeker::net::Request as _;
use bmseeker::placement::bring as _;
use bmseeker::placement::chart_info as _;
use bmseeker::placement::is_chart as _;
use bmseeker::placement::locate as _;
use bmseeker::placement::same_artist as _;
use bmseeker::placement::same_name as _;
use bmseeker::placement::same_title as _;
use bmseeker::placement::ChartInfo as _;
use bmseeker::placement::Located as _;
use bmseeker::placement::Rules as _;
use bmseeker::progress::Progress as _;
use bmseeker::progress::Stage as _;
use bmseeker::progress::Stage::Unpacking as _;
use bmseeker::progress::CANCELLED as _;
use bmseeker::refusal::Refusals as _;
use bmseeker::sources::Query as _;
use bmseeker::table::Course as _;
use bmseeker::table::CourseTrophy as _;
use bmseeker::table::DifficultyTable as _;
use bmseeker::table::TableEntry as _;
use bmseeker::table::TableHeader as _;
use bmseeker::table::DEFAULT_TABLES as _;
use bmseeker::table::INSANE as _;
use bmseeker::table::MOVED as _;
#[cfg(feature = "testkit")]
use bmseeker::testkit::tar as _;
#[cfg(feature = "testkit")]
use bmseeker::testkit::tar_gz as _;
#[cfg(feature = "testkit")]
use bmseeker::testkit::zip as _;
#[cfg(feature = "testkit")]
use bmseeker::testkit::SONG_RAR as _;
use bmseeker::unpack::sniff as _;
use bmseeker::unpack::unpack as _;
use bmseeker::unpack::unpack_bytes as _;
use bmseeker::unpack::Blob as _;
use bmseeker::unpack::Blob::Bytes as _;
use bmseeker::unpack::Blob::File as _;
use bmseeker::unpack::Format::Zip as _;
use bmseeker::unpack::BUDGET as _;
use bmseeker::web::proxied as _;
use bmseeker::web::sends_cors as _;
use bmseeker::web::served as _;
use bmseeker::web::unserved as _;
use bmseeker::web::DOWNLOAD_MAX as _;
use bmseeker::web::PROXY as _;

fn _associated() {
    let _ = bmseeker::config::Config::default;
    let _ = bmseeker::install::EntryJob::new;
    let _ = bmseeker::install::Request::default;
    #[cfg(not(target_arch = "wasm32"))]
    let _ = bmseeker::native::Native::new;
    let _ = bmseeker::refusal::Guarded::new;
    let _ = bmseeker::sources::Sources::from_config;
    let _ = bmseeker::table::TableLoader::new;
}

fn _trait_0<T: bmseeker::fs::Fs + ?Sized>() {
    let _ = T::rename;
}

fn _trait_1<T: bmseeker::fs::Store + ?Sized>() {
    let _ = T::copy;
    let _ = T::create;
    let _ = T::create_new_dir;
    let _ = T::open;
    let _ = T::remove_dir;
}
