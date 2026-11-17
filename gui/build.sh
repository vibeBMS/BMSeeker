#!/usr/bin/env bash
# The two release binaries: target/release/bmseeker (the CLI) and
# target/release/bmseeker-gui (the window, with the CLI inside it), in
# $CARGO_TARGET_DIR when it is set.
#
#     gui/build.sh            for this system
#     GOOS=... GOARCH=... CARGO_TARGET=... gui/build.sh   for another
set -euo pipefail
cd "$(dirname "$0")/.."
# (no arrays: macOS has bash 3.2, where an empty one is unbound)
cargo build --release --features cli ${CARGO_TARGET:+--target "$CARGO_TARGET"}
out="${CARGO_TARGET_DIR:-target}/${CARGO_TARGET:+$CARGO_TARGET/}release"
exe=bmseeker
case "${GOOS:-$(go env GOOS)}" in windows) exe=bmseeker.exe ;; esac
rm -f gui/bin/bmseeker gui/bin/bmseeker.exe
cp "$out/$exe" gui/bin/
gui_exe=bmseeker-gui
[[ "$exe" == *.exe ]] && gui_exe=bmseeker-gui.exe
. gui/identity.sh
(cd gui && go build -trimpath -ldflags "-s -w $identity" -o "$gui_exe" .)
mv "gui/$gui_exe" "$out/"
echo "$out/$exe"
echo "$out/$gui_exe"
