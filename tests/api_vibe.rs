//! Every BMSeeker item vibeBMS (the player built on BMSeeker) names, named
//! once: a rename, a move without a `pub use` at the old path, or a
//! narrower visibility fails here, in BMSeeker's own tests. A change to
//! this file means vibe must change too. Made by
//! `python3 tools/api_vibe.py <vibeBMS checkout> > tests/api_vibe.rs`
//! (it names; it does not check signatures).
#![allow(unused_imports)]
// (a field named in a pattern as `_`, beside `..`, is named all the same)
#![allow(clippy::unneeded_wildcard_pattern)]

// (only their types: with type parameters of their own or of their
// type, they cannot be named without a call) bmseeker::hosts::Target::url, bmseeker::install::job::TaskJob::new, bmseeker::library::Library::from_catalog, bmseeker::links::Links::from_tables, bmseeker::testkit::Reply::ok, bmseeker::testkit::Reply::status

use bmseeker::config::native_user_agent as _;
use bmseeker::config::user_agent_with as _;
use bmseeker::config::Config as _;
use bmseeker::config::HTTP_CLIENT as _;
use bmseeker::config::USER_AGENT as _;
use bmseeker::fs::device_name as _;
use bmseeker::fs::file_name as _;
use bmseeker::fs::Disk as _;
use bmseeker::fs::Entry as _;
use bmseeker::fs::Fs as _;
use bmseeker::fs::ReadSeek as _;
use bmseeker::fs::Store as _;
use bmseeker::hosts::Context as _;
use bmseeker::hosts::Fetch as _;
use bmseeker::hosts::Fetch::Failed as _;
use bmseeker::hosts::Fetch::Page as _;
use bmseeker::hosts::Target as _;
use bmseeker::hosts::Want as _;
use bmseeker::hosts::PRESS_DOWNLOAD_JS as _;
use bmseeker::hosts::STALL_MS as _;
use bmseeker::import::import as _;
use bmseeker::import::unpack_into as _;
use bmseeker::import::Imported as _;
use bmseeker::import::Imported::Ask as _;
use bmseeker::import::Imported::Other as _;
use bmseeker::import::Imported::Owned as _;
use bmseeker::import::Imported::Placed as _;
use bmseeker::import::Imported::Unpacked as _;
use bmseeker::import::Imported::Waiting as _;
use bmseeker::install::got as _;
use bmseeker::install::job::run_job as _;
use bmseeker::install::job::Effect as _;
use bmseeker::install::job::Effect::Download as _;
use bmseeker::install::job::Effect::Http as _;
use bmseeker::install::job::Effect::Sleep as _;
use bmseeker::install::job::Fed as _;
use bmseeker::install::job::Fed::Downloaded as _;
use bmseeker::install::job::Fed::Http as _;
use bmseeker::install::job::Fed::Woke as _;
use bmseeker::install::job::FileLink as _;
use bmseeker::install::job::Files as _;
use bmseeker::install::job::Got as _;
use bmseeker::install::job::Job as _;
use bmseeker::install::job::Missed as _;
use bmseeker::install::job::Step as _;
use bmseeker::install::job::Step::Do as _;
use bmseeker::install::job::Step::Done as _;
use bmseeker::install::job::TaskJob as _;
use bmseeker::install::job::Unserved as _;
use bmseeker::install::md5_hex as _;
use bmseeker::install::request as _;
use bmseeker::install::safe_name as _;
use bmseeker::install::take as _;
use bmseeker::install::Done as _;
use bmseeker::install::Done::Added as _;
use bmseeker::install::Done::Failed as _;
use bmseeker::install::Done::Owned as _;
use bmseeker::install::Done::Page as _;
use bmseeker::install::Done::Placed as _;
use bmseeker::install::Done::Waiting as _;
use bmseeker::install::Entry as _;
use bmseeker::install::Failure as _;
use bmseeker::install::Failure::Net as _;
use bmseeker::install::Failure::NoChartLink as _;
use bmseeker::install::Failure::NotTheChart as _;
use bmseeker::install::Failure::Other as _;
use bmseeker::install::Session as _;
use bmseeker::library::Library as _;
use bmseeker::library::Owned as _;
use bmseeker::links::Links as _;
use bmseeker::manbow::mirror as _;
use bmseeker::manbow::mirror::parse_catalog as _;
use bmseeker::manbow::mirror::parse_index as _;
use bmseeker::manbow::mirror::Event as _;
use bmseeker::manbow::mirror::File as _;
use bmseeker::manbow::mirror::Work as _;
use bmseeker::manbow::mirror::MAX_BYTES as _;
use bmseeker::net::too_large as _;
use bmseeker::net::Answer as _;
use bmseeker::net::Kind as _;
use bmseeker::net::Kind::Api as _;
use bmseeker::net::Kind::Header as _;
use bmseeker::net::Kind::Page as _;
use bmseeker::net::NetError as _;
use bmseeker::net::NetError::Cancelled as _;
use bmseeker::net::NetError::Other as _;
use bmseeker::net::NetError::Stalled as _;
use bmseeker::net::NetError::Status as _;
use bmseeker::net::Next as _;
use bmseeker::net::Next::Ask as _;
use bmseeker::net::Next::Done as _;
use bmseeker::net::Request as _;
use bmseeker::net::Request::Get as _;
use bmseeker::net::Request::Post as _;
use bmseeker::net::Task as _;
use bmseeker::placement::bring as _;
use bmseeker::placement::chart_info as _;
use bmseeker::placement::is_chart as _;
use bmseeker::placement::locate as _;
use bmseeker::placement::ChartInfo as _;
use bmseeker::placement::Located as _;
use bmseeker::placement::Located::Ask as _;
use bmseeker::placement::Located::Here as _;
use bmseeker::placement::Located::Move as _;
use bmseeker::placement::Located::NoSong as _;
use bmseeker::placement::Rules as _;
use bmseeker::placement::SongKey as _;
use bmseeker::progress::Progress as _;
use bmseeker::progress::Stage as _;
use bmseeker::progress::Stage::Adding as _;
use bmseeker::progress::Stage::Downloading as _;
use bmseeker::progress::Stage::Finding as _;
use bmseeker::progress::Stage::Unpacking as _;
use bmseeker::progress::Stage::Waiting as _;
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
#[cfg(all(feature = "testkit", not(target_arch = "wasm32")))]
use bmseeker::testkit::serve as _;
#[cfg(feature = "testkit")]
use bmseeker::testkit::tar as _;
#[cfg(feature = "testkit")]
use bmseeker::testkit::tar_gz as _;
#[cfg(feature = "testkit")]
use bmseeker::testkit::zip as _;
#[cfg(feature = "testkit")]
use bmseeker::testkit::Reply as _;
#[cfg(feature = "testkit")]
use bmseeker::testkit::SIZELESS_RAR as _;
#[cfg(feature = "testkit")]
use bmseeker::testkit::SONG_RAR as _;
use bmseeker::unpack::archive_folder as _;
use bmseeker::unpack::is_archive as _;
use bmseeker::unpack::member_path as _;
use bmseeker::unpack::read as _;
use bmseeker::unpack::sniff as _;
use bmseeker::unpack::unpack as _;
use bmseeker::unpack::unpack_bytes as _;
use bmseeker::unpack::Blob as _;
use bmseeker::unpack::Blob::Bytes as _;
use bmseeker::unpack::Blob::File as _;
use bmseeker::unpack::Error as _;
use bmseeker::unpack::Error::TooMany as _;
use bmseeker::unpack::Format as _;
use bmseeker::unpack::Format::Rar as _;
use bmseeker::unpack::Limits as _;
use bmseeker::unpack::Member as _;
use bmseeker::unpack::Sink as _;
use bmseeker::unpack::Take as _;
use bmseeker::unpack::Take::Skip as _;
use bmseeker::unpack::Take::Stop as _;
use bmseeker::unpack::Take::Write as _;
use bmseeker::unpack::BUDGET as _;
use bmseeker::unpack::HEAD as _;
use bmseeker::web::proxied as _;
use bmseeker::web::sends_cors as _;
use bmseeker::web::served as _;
use bmseeker::web::unserved as _;
use bmseeker::web::DOWNLOAD_MAX as _;
use bmseeker::web::PROXY as _;

fn _associated() {
    let _ = bmseeker::config::Config::default;
    let _ = bmseeker::hosts::Context::default;
    let _ = bmseeker::hosts::Context::new;
    let _ = bmseeker::install::Entry::from_table_in;
    let _ = bmseeker::install::EntryJob::new;
    let _ = bmseeker::install::Request::default;
    let _ = bmseeker::install::Session::new;
    let _ = bmseeker::install::job::Files::new;
    let _ = bmseeker::install::job::Files::slot;
    let _ = bmseeker::library::Library::new;
    let _ = bmseeker::links::Links::chart_links_in;
    let _ = bmseeker::links::Links::from_stella;
    let _ = bmseeker::links::Links::song_links_in;
    let _ = bmseeker::manbow::mirror::Work::default;
    #[cfg(not(target_arch = "wasm32"))]
    let _ = bmseeker::native::Native::new;
    let _ = bmseeker::placement::Rules::default;
    let _ = bmseeker::placement::SongKey::of;
    let _ = bmseeker::progress::Progress::new;
    let _ = bmseeker::refusal::Guarded::new;
    let _ = bmseeker::sources::Sources::from_config;
    let _ = bmseeker::table::TableLoader::new;
}

fn _fields_0(v: bmseeker::config::Config) {
    let _ = matches!(
        v,
        bmseeker::config::Config {
            folder_naming: _,
            local_hosts: _,
            sources: _,
            third_party: _,
            user_agent: _,
            ..
        }
    );
}

fn _fields_1(v: bmseeker::fs::Entry) {
    let _ = matches!(
        v,
        bmseeker::fs::Entry {
            is_dir: _,
            path: _,
            ..
        }
    );
}

fn _fields_2(v: bmseeker::hosts::Context) {
    let _ = matches!(
        v,
        bmseeker::hosts::Context {
            failures_are_pages: _,
            ..
        }
    );
}

fn _fields_3(v: bmseeker::hosts::Target) {
    let _ = matches!(v, bmseeker::hosts::Target { mega_key: _, .. });
}

fn _fields_4(v: bmseeker::hosts::Want) {
    let _ = matches!(
        v,
        bmseeker::hosts::Want {
            artist: _,
            title: _,
            ..
        }
    );
}

fn _fields_5(v: bmseeker::install::Entry) {
    let _ = matches!(
        v,
        bmseeker::install::Entry {
            md5: _,
            song: _,
            want: _,
            ..
        }
    );
}

fn _fields_6(v: bmseeker::install::job::Effect) {
    let _ = matches!(
        v,
        bmseeker::install::job::Effect::Download {
            slot: _,
            target: _,
            ..
        }
    );
}

fn _fields_7(v: bmseeker::install::job::FileLink) {
    let _ = matches!(
        v,
        bmseeker::install::job::FileLink {
            plain: _,
            url: _,
            ..
        }
    );
}

fn _fields_8(v: bmseeker::install::job::Unserved) {
    let _ = matches!(
        v,
        bmseeker::install::job::Unserved {
            at: _,
            error: _,
            ..
        }
    );
}

fn _fields_9(v: bmseeker::library::Owned) {
    let _ = matches!(
        v,
        bmseeker::library::Owned {
            artist: _,
            md5: _,
            path: _,
            sha256: _,
            title: _,
            ..
        }
    );
}

fn _fields_10(v: bmseeker::manbow::mirror::Event) {
    let _ = matches!(
        v,
        bmseeker::manbow::mirror::Event {
            index: _,
            tag: _,
            ..
        }
    );
}

fn _fields_11(v: bmseeker::manbow::mirror::File) {
    let _ = matches!(
        v,
        bmseeker::manbow::mirror::File {
            name: _,
            original: _,
            size: _,
            url: _,
            ..
        }
    );
}

fn _fields_12(v: bmseeker::manbow::mirror::Work) {
    let _ = matches!(
        v,
        bmseeker::manbow::mirror::Work {
            artist: _,
            num: _,
            page: _,
            title: _,
            ..
        }
    );
}

fn _fields_13(v: bmseeker::net::Request) {
    let _ = matches!(
        v,
        bmseeker::net::Request::Post {
            body: _,
            url: _,
            ..
        }
    );
}

fn _fields_14(v: bmseeker::sources::Query) {
    let _ = matches!(
        v,
        bmseeker::sources::Query {
            diff: _,
            md5: _,
            want: _,
            ..
        }
    );
}

fn _fields_15(v: bmseeker::table::Course) {
    let _ = matches!(
        v,
        bmseeker::table::Course {
            md5: _,
            name: _,
            trophy: _,
            ..
        }
    );
}

fn _fields_16(v: bmseeker::table::DifficultyTable) {
    let _ = matches!(
        v,
        bmseeker::table::DifficultyTable {
            entries: _,
            fetched_at: _,
            header: _,
            source_url: _,
            ..
        }
    );
}

fn _fields_17(v: bmseeker::table::TableEntry) {
    let _ = matches!(
        v,
        bmseeker::table::TableEntry {
            artist: _,
            level: _,
            md5: _,
            org_md5: _,
            sha256: _,
            title: _,
            url: _,
            url_diff: _,
            ..
        }
    );
}

fn _fields_18(v: bmseeker::table::TableHeader) {
    let _ = matches!(
        v,
        bmseeker::table::TableHeader {
            name: _,
            symbol: _,
            ..
        }
    );
}

fn _fields_19(v: bmseeker::unpack::Limits) {
    let _ = matches!(
        v,
        bmseeker::unpack::Limits {
            bytes: _,
            members: _,
            ..
        }
    );
}

fn _fields_20(v: bmseeker::unpack::Member) {
    let _ = matches!(
        v,
        bmseeker::unpack::Member {
            mtime: _,
            rel: _,
            size: _,
            ..
        }
    );
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
