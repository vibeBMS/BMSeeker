#!/usr/bin/env bash
# BMSeeker web into web/dist (or $OUT), a static site: the page and its
# script (web/www), the commands' wasm (bmseeker-web, with wasm-bindgen's
# glue) and the window's (gui/, Go's js/wasm, with Go's loader), with the
# version (gui/identity.sh) at the page's foot. Needs the
# wasm32-unknown-unknown target and wasm-bindgen-cli of the version in
# Cargo.lock.
set -euo pipefail
cd "$(dirname "$0")/.."
out=${OUT:-web/dist}
target=${CARGO_TARGET_DIR:-target}
cargo build -p bmseeker-web --target wasm32-unknown-unknown --profile web
rm -rf "$out"
mkdir -p "$out"
out=$(cd "$out" && pwd)
cp web/www/* "$out/"
. gui/identity.sh
perl -pi -e "s/\@VERSION\@/$name $version/" "$out/index.html"
wasm-bindgen --target web --no-typescript --remove-name-section --remove-producers-section \
  --out-dir "$out/pkg" \
  "$target/wasm32-unknown-unknown/web/bmseeker_web.wasm"
if command -v wasm-opt >/dev/null 2>&1; then
  wasm-opt -Os -o "$out/pkg/bmseeker_web_bg.wasm" "$out/pkg/bmseeker_web_bg.wasm"
fi
cp "$(go env GOROOT)/lib/wasm/wasm_exec.js" "$out/"
(cd gui && GOOS=js GOARCH=wasm go build -trimpath \
  -ldflags "$release_ldflags" -tags "$release_tags" -o "$out/gui.wasm" .)
echo "$out"
