#!/usr/bin/env python3
"""THIRD_PARTY_LICENSES.md from cargo-about's JSON and the window's Go
modules (tools/gen_licenses.sh runs it).

  gen_licenses.py <cargo-about json> <go build tags>    the Markdown on stdout

Each licence text once, then the crates (or Go modules) under it by name: no
versions, so a version bump leaves the file as it is and only a crate or
module coming or going, or a licence text changing, changes it. The
workspace's own crates (no source) and the window's own module are left out.
The same input gives the same bytes: everything is sorted, and nothing of the
machine (paths, dates) is written.

The Go part: the packages `go list -deps` gives for gui/ on each system the
window is built for (with the release's build tags), and for each the licence
files (LICENSE, COPYING, NOTICE, PATENTS, ...) in its folder and the folders
above it up to its module's root; the standard library's are Go's own
(GOROOT). A text that is none of GO_KINDS stops it, for a person to read it.
"""
import json
import os
import re
import subprocess
import sys

HEADER = """\
# Third-party licences

BMSeeker (the command-line tool, the window and the web page) is built from
the Rust crates and, for the window, the Go modules below, each under its own
licence. For each licence text: the text, then the crates or modules under it.
A crate that offers a choice (`MIT OR Apache-2.0`) is listed under the licence
used; its own expression is beside its name. Code BMSeeker took or derived
directly (the BeMusicSeeker fork, UnRAR) is in
[THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md).

Made by `tools/gen_licenses.sh` with cargo-about, from Cargo.lock, about.toml
and gui/go.sum: do not edit it (CI checks that it is current).
"""

# what a licence's standard text has where a crate's own file names its holders
# (not the "[yyyy] [name of copyright owner]" of the Apache licence's appendix,
# which shows how to apply it, in every copy of it)
PLACEHOLDERS = ("<copyright holders>", "<year>", "<owner>", "<OWNER>", "<YEAR>")

# the systems the window is built for (.github/workflows/build.yml; js/wasm:
# web/build.sh)
GO_PLATFORMS = ["darwin/amd64", "darwin/arm64", "js/wasm", "linux/amd64", "linux/arm64", "windows/amd64",
                "windows/arm64"]
GO_FILE = re.compile(r"(LICEN[CS]E|COPYING|NOTICE|PATENTS|UNLICENSE)([._-].*)?", re.I)
# the kinds of Go licence texts accepted: (name, what the text has)
GO_KINDS = [
    ("Apache License 2.0 (`Apache-2.0`)", ["Apache License", "Version 2.0, January 2004"]),
    ("MIT License (`MIT`)", ["Permission is hereby granted, free of charge"]),
    ("The Unlicense or BSD 3-Clause License (`Unlicense OR BSD-3-Clause`)",
     ["SPDX-License-Identifier: Unlicense OR BSD-3-Clause"]),
    ('BSD 3-Clause "New" or "Revised" License (`BSD-3-Clause`)',
     ["Redistribution and use in source and binary forms", "Neither the name"]),
    ("Go's additional IP rights grant (patents)", ["Additional IP Rights Grant (Patents)"]),
]


def norm(text):
    lines = [line.rstrip() for line in text.replace("\r\n", "\n").replace("\r", "\n").split("\n")]
    while lines and not lines[0]:
        lines.pop(0)
    while lines and not lines[-1]:
        lines.pop()
    return "\n".join(lines)


def plural(n, word):
    return f"{n} {word}{'' if n == 1 else 's'}"


def joined(values):
    return ", ".join(sorted(v for v in values if v))


def rust(path):
    """[(kind, text, notes, [crate lines])] and the number of crates"""
    with open(path, encoding="utf-8") as f:
        about = json.load(f)
    # (id, text, the standard text?) -> name -> its versions' fields
    sections = {}
    for lic in about["licenses"]:
        key = (lic["id"], norm(lic["text"]), lic["source_path"] is None)
        for used in lic["used_by"]:
            c = used["crate"]
            if c["source"] is None:
                continue
            crate = sections.setdefault(key, {}).setdefault(c["name"], {"expr": set(), "repo": set(), "authors": set()})
            crate["expr"].add(c["license"] or "")
            crate["repo"].add(c["repository"] or "")
            crate["authors"].update(c["authors"])
    names_of = {lic["id"]: lic["name"] for lic in about["licenses"]}

    out = []
    for lid, text, generic in sorted(sections, key=lambda k: (k[0], min(sections[k]), k[1])):
        crates = sections[(lid, text, generic)]
        placeholder = generic and any(p in text for p in PLACEHOLDERS)
        notes = []
        if generic:
            notes.append("These crates ship no file with this licence's text: it is the licence's standard text"
                         + (", and their copyright holders are the authors each names." if placeholder else "."))
        lines = []
        for name in sorted(crates):
            c = crates[name]
            line = f"- `{name}`" + (f" ({joined(c['expr'])})" if joined(c["expr"]) else "")
            if joined(c["repo"]):
                line += f": {joined(c['repo'])}"
            if placeholder:
                line += f"; authors: {joined(c['authors']) or 'none named (see its repository)'}"
            lines.append(line)
        out.append((f"{names_of[lid]} (`{lid}`)", text, notes, lines, set(crates)))
    return out


def stream(text):
    """the JSON values `go list -json` writes one after the other"""
    dec, i = json.JSONDecoder(), 0
    while True:
        while i < len(text) and text[i].isspace():
            i += 1
        if i == len(text):
            return
        value, i = dec.raw_decode(text, i)
        yield value


def go(gui, tags):
    """the same for the window's Go modules"""
    goroot = subprocess.run(["go", "env", "GOROOT"], cwd=gui, check=True, capture_output=True,
                            text=True).stdout.strip()
    # module path -> its licence files' paths; "" is Go's standard library
    files = {}
    for platform in GO_PLATFORMS:
        goos, goarch = platform.split("/")
        env = dict(os.environ, GOOS=goos, GOARCH=goarch, CGO_ENABLED="0")
        listed = subprocess.run(["go", "list", "-deps", "-tags", tags, "-json=Dir,Standard,Module", "."],
                                cwd=gui, env=env, check=True, capture_output=True, text=True).stdout
        for pkg in stream(listed):
            if pkg.get("Standard"):
                files.setdefault("", set()).update(
                    os.path.join(goroot, f) for f in os.listdir(goroot) if GO_FILE.fullmatch(f))
                continue
            mod = pkg["Module"]
            if mod.get("Main"):
                continue
            found = files.setdefault(mod["Path"], set())
            d = pkg["Dir"]
            while True:
                found.update(os.path.join(d, f) for f in os.listdir(d)
                             if GO_FILE.fullmatch(f) and os.path.isfile(os.path.join(d, f)))
                if os.path.samefile(d, mod["Dir"]) or os.path.dirname(d) == d:
                    break
                d = os.path.dirname(d)
    missing = sorted(m for m, f in files.items() if not f)
    if missing:
        sys.exit(f"no licence file in the Go module(s) {', '.join(missing)}: read their licences and say so here")

    sections = {}
    for mod, paths in files.items():
        for p in paths:
            with open(p, encoding="utf-8") as f:
                text = norm(f.read())
            kind = next((k for k, needles in GO_KINDS if all(n in text for n in needles)), None)
            if kind is None:
                sys.exit(f"{p} ({mod or 'Go'}): not a licence text gen_licenses.py knows: read it and add it to GO_KINDS")
            sections.setdefault((kind, text), set()).add(mod)
    out = []
    for kind, text in sorted(sections, key=lambda k: (k[0], min(sections[k]), k[1])):
        mods = sections[(kind, text)]
        lines = [("- Go (its standard library and runtime, in every Go program): https://go.dev" if m == ""
                  else f"- `{m}`: https://pkg.go.dev/{m}") for m in sorted(mods)]
        out.append((kind, text, [], lines, {m or "Go" for m in mods}))
    return out


def write(out, sections):
    for kind, text, notes, lines, _ in sections:
        out.append(f"### {kind}\n")
        out.extend(n + "\n" for n in notes)
        out.extend(lines)
        out.append("\n````text\n" + text + "\n````\n")


def summary(sections, what, also=""):
    names = {n for s in sections for n in s[4]}
    kinds = {}
    for s in sections:
        kinds.setdefault(s[0], []).append(s)
    out = [f"{plural(len(names), what)}{also}, {plural(len(sections), 'licence text')}:\n"]
    for kind in sorted(kinds):
        n = len({x for s in kinds[kind] for x in s[4]})
        out.append(f"- {kind}: {plural(n, what)}, {plural(len(kinds[kind]), 'text')}")
    out.append("")
    return out


def main(about_json, tags):
    gui = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "gui")
    crates = rust(about_json)
    modules = go(gui, tags)
    out = [HEADER, "## Rust crates\n"]
    out += summary(crates, "crate")
    write(out, crates)
    out += ["## Go modules (the window)\n",
            "The window (`bmseeker-gui`, and the web page's) is written in Go: these are the modules it links on"
            " every system it is built for, and Go's own standard library and runtime.\n"]
    out += summary(modules, "module", " (Go itself one of them)")
    write(out, modules)
    sys.stdout.write("\n".join(out).rstrip("\n") + "\n")


if __name__ == "__main__":
    if len(sys.argv) != 3:
        sys.exit("usage: gen_licenses.py <cargo-about json> <go build tags>")
    main(sys.argv[1], sys.argv[2])
