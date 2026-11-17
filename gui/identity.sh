# shellcheck shell=bash disable=SC2034 # (the variables are the sourcing script's)
# Sourced by gui/build.sh and web/build.sh: the app's version, Cargo.toml's
# (tools/ci_version.sh writes a CI build's there, so it is the CLI's too),
# its name and id (a channel's in CI), and the -ldflags that link them into
# the window, as `mygo build` links what a mygo.json says.
version=$(perl -ne 'if (/^\[workspace\.package\]/) { $w = 1 } elsif (/^\[/) { $w = 0 }
  if ($w && /^version = "([^"]+)"/) { print $1; exit }' Cargo.toml)
name=${BMSEEKER_APP_NAME:-BMSeeker}
id=${BMSEEKER_APP_ID:-io.github.vibebms.bmseeker}
mygo=github.com/egoist/mygo
identity="-X '$mygo.packageName=$name' -X $mygo.packageVersion=$version -X $mygo.packageIdentifier=$id"
