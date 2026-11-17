#!/usr/bin/env bash
# The version and identity of a CI build, by channel (BMSEEKER_CHANNEL,
# default "nightly"; a release is "stable"), as vibeBMS's builds have them:
# - version: <version>-<channel>.<run>+<commit>, e.g. 0.1.0-nightly.12+e129ac9
#   (a stable build is the plain <version>). The run number only grows; the
#   commit says which source it is. Written into Cargo.toml, so
#   CARGO_PKG_VERSION (the CLI's --version) carries it, and gui/build.sh and
#   web/build.sh link it into the window and the page. Local builds keep the
#   committed <version>-dev.
# - app id: io.github.vibebms.bmseeker.<channel> and the name
#   "BMSeeker <Channel>" (a stable build is io.github.vibebms.bmseeker,
#   "BMSeeker"), which the window takes (gui/build.sh), so each channel keeps
#   its own data beside the others'.
# Perl, not bash 4: macOS has bash 3.2.
set -euo pipefail
channel="${BMSEEKER_CHANNEL:-nightly}"
[[ "$channel" =~ ^[a-z]+$ ]] || { echo "bad channel: $channel" >&2; exit 1; }
base="$(perl -ne 'if (/^\[workspace\.package\]/) { $w = 1 } elsif (/^\[/) { $w = 0 }
  if ($w && /^version = "(\d+\.\d+\.\d+)/) { print $1; exit }' Cargo.toml)"
[ -n "$base" ] || { echo "no [workspace.package] version in Cargo.toml" >&2; exit 1; }
run="${GITHUB_RUN_NUMBER:?}"
if [ "$channel" = stable ]; then
  version="$base"
  suffix=""
  name="BMSeeker"
else
  version="$base-$channel.$run+${GITHUB_SHA:0:7}"
  suffix=".$channel"
  name="BMSeeker $(printf %s "$channel" | perl -pe 's/^(.)/\u$1/')"
fi
perl -pi -e 'if (/^\[workspace\.package\]/) { $w = 1 } elsif (/^\[/) { $w = 0 }
  s/^version = "[^"]*"/version = "'"$version"'"/ if $w' Cargo.toml
{
  echo "BMSEEKER_CHANNEL=$channel"
  echo "BMSEEKER_VERSION=$version"
  echo "BMSEEKER_BASE_VERSION=$base"
  echo "BMSEEKER_BUILD=$run"
  echo "BMSEEKER_APP_ID=io.github.vibebms.bmseeker$suffix"
  echo "BMSEEKER_APP_NAME=$name"
} >> "$GITHUB_ENV"
echo "version $version ($name, io.github.vibebms.bmseeker$suffix)"
