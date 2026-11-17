#!/usr/bin/env bash
# latest.json and builds.json for the release of the executables in <dir>
# (.github/workflows/build.yml's), written into <dir> as vibeBMS's nightly
# has them. Both live on the nightly release, where the previous ones are
# read from (GH_TOKEN, GH_REPO); the channel and version are
# tools/ci_version.sh's (BMSEEKER_CHANNEL, BMSEEKER_VERSION).
# - latest.json, what an updater reads: per channel, its version and each
#   executable's version, SHA-256 and size,
#     {"channels": {"nightly": {"version": "0.1.0-nightly.12+abc1234",
#       "files": {"bmseeker-linux-x64": {"version": ..., "sha256": ..., "size": ...}}}}}
#   This channel's part is written and the others kept. The nightly keeps
#   the entry of an executable whose build failed, as its file stays on the
#   release.
# - builds.json, each published build's executables' SHA-256 and size,
#   appended to: nightlies kept 90 days (500 at most), releases always.
set -euo pipefail
dir=$1
channel=${BMSEEKER_CHANNEL:?}
version=${BMSEEKER_VERSION:?}
names=""
for t in linux-x64 linux-arm64 windows-x64.exe windows-arm64.exe macos-universal; do
  names="$names bmseeker-$t bmseeker-gui-$t"
done

prev=$(mktemp -d)
for f in latest.json builds.json; do
  gh release download nightly -p "$f" -D "$prev" --clobber 2>/dev/null || true
done
jq -e 'type == "object"' "$prev/latest.json" >/dev/null 2>&1 || echo '{}' > "$prev/latest.json"
jq -e '.builds | type == "array"' "$prev/builds.json" >/dev/null 2>&1 || echo '{"builds": []}' > "$prev/builds.json"

files='{}'
for n in $names; do
  [ -f "$dir/$n" ] || continue
  files="$(jq -c --arg n "$n" --arg sha "$(sha256sum "$dir/$n" | cut -d' ' -f1)" \
    --argjson size "$(stat -c %s "$dir/$n")" '. + {($n): {sha256: $sha, size: $size}}' <<< "$files")"
done

jq --arg channel "$channel" --arg version "$version" --arg names "$names" --argjson files "$files" '
  (if (.channels | type) == "object" then .channels else {} end) as $ch
  | (if $channel == "nightly"
      then ($ch.nightly.files // {}) | with_entries(select(.key | IN($names | splits(" "))))
      else {} end) as $old
  | {channels: ($ch + {($channel): {version: $version,
      files: ($old + ($files | map_values({version: $version} + .)))}})}
' "$prev/latest.json" > "$dir/latest.json"

jq --arg version "$version" --arg commit "$GITHUB_SHA" --argjson files "$files" \
  --arg date "$(date -u +%Y-%m-%dT%H:%M:%SZ)" --arg since "$(date -u -d '90 days ago' +%Y-%m-%dT%H:%M:%SZ)" '
  def nightly: (.version // "-") | test("-");
  # a run done again replaces its entry; a run that built nothing adds none
  .builds
  | if $files == {} then . else
      [.[] | select(.version != $version)] + [{version: $version, commit: $commit, date: $date, files: $files}]
    end
  | ([.[] | select(nightly)] | length - 500) as $over
  | {v: 1, builds: (reduce .[] as $b ({n: 0, out: []};
      if ($b | nightly) then
        .n += 1 | if .n > $over and $b.date >= $since then .out += [$b] else . end
      else .out += [$b] end) | .out)}
' "$prev/builds.json" > "$dir/builds.json"
rm -r "$prev"
cat "$dir/latest.json"
jq -c '.builds[-1]' "$dir/builds.json"
