#!/usr/bin/env bash
# The two release binaries: target/release/bmseeker (the CLI) and
# target/release/bmseeker-gui (the window, with the CLI inside it).
#
#     gui/build.sh            for this system
#     GOOS=... GOARCH=... CARGO_TARGET=... gui/build.sh   for another
set -euo pipefail
cd "$(dirname "$0")/.."
target_flag=()
out=target/release
if [[ -n "${CARGO_TARGET:-}" ]]; then
  target_flag=(--target "$CARGO_TARGET")
  out="target/$CARGO_TARGET/release"
fi
cargo build --release --features cli "${target_flag[@]}"
exe=bmseeker
case "${GOOS:-$(go env GOOS)}" in windows) exe=bmseeker.exe ;; esac
rm -f gui/bin/bmseeker gui/bin/bmseeker.exe
cp "$out/$exe" gui/bin/
gui_exe=bmseeker-gui
[[ "$exe" == *.exe ]] && gui_exe=bmseeker-gui.exe
(cd gui && go build -trimpath -ldflags "-s -w" -o "../$out/$gui_exe" .)
echo "$out/$exe"
echo "$out/$gui_exe"
