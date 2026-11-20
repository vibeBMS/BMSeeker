# shellcheck shell=bash disable=SC2034 # (the variables are the sourcing script's)
# Sourced by gui/build.sh and web/build.sh: the app's version, Cargo.toml's
# (tools/ci_version.sh writes a CI build's there, so it is the CLI's too),
# its name and id (a channel's in CI), and the -ldflags that link them into
# the window, as `mygo build` links what a mygo.json says; and what a
# release build of the window is built with besides, as `mygo build` builds
# one: production (no web inspector in a page's window, no page trusted for
# being on localhost, nothing of `mygo dev` read from the environment),
# without the window's inspector and the downloader of native libraries.
version=$(perl -ne 'if (/^\[workspace\.package\]/) { $w = 1 } elsif (/^\[/) { $w = 0 }
  if ($w && /^version = "([^"]+)"/) { print $1; exit }' Cargo.toml)
name=${BMSEEKER_APP_NAME:-BMSeeker}
id=${BMSEEKER_APP_ID:-io.github.vibebms.bmseeker}
mygo=github.com/egoist/mygo
identity="-X '$mygo.packageName=$name' -X $mygo.packageVersion=$version -X $mygo.packageIdentifier=$id"
release_ldflags="-s -w -X $mygo.production=1 $identity"
release_tags=mygo_noinspector,mygo_nodownload
