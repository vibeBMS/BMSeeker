#!/usr/bin/env python3
"""THIRD_PARTY_LICENSES.md from cargo-about's JSON and the window's Go
modules (tools/gen_licenses.sh runs it).

  gen_licenses.py <cargo-about json> <go build tags>    the Markdown on stdout

Each licence text once, then the crates (or Go modules) under it by name,
each with the copyright notices its own licence file has: no versions, so a
version bump leaves the file as it is and only a crate or module coming or
going, a licence text or a notice changing, changes it. The workspace's own
crates (no source) and the window's own module are left out. The same input
gives the same bytes: everything is sorted, and nothing of the machine (paths,
dates) is written.

Two files of one licence are one text when they are equal once their notices
are taken out (merge_key says what else is ignored): exact, no similarity
measure, so a file with other words (a clause, an exception, a note, another
name in BSD's third clause) keeps a text of its own. The text printed for
several is the licence's standard text when one of the crates has no file
(cargo-about uses it for those), else the form most of their files have (the
first in sorted order on a tie), without its notices.

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
licence. Each licence text is here once, then the crates or modules under it,
each with the copyright notices its own licence file has (files that differ
only in white space, letter case, a title line, `http`/`https` or the Apache
licence's appendix are one text). A crate that offers a choice
(`MIT OR Apache-2.0`) is listed under the licence used; its own expression is
beside its name. Code BMSeeker took or derived directly (the BeMusicSeeker
fork, UnRAR) is in [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md).

Made by `tools/gen_licenses.sh` with cargo-about, from Cargo.lock, about.toml
and gui/go.sum: do not edit it (CI checks that it is current).
"""

# what a licence's standard text has where a crate's own file names its holders
# (not the "[yyyy] [name of copyright owner]" of the Apache licence's appendix,
# which shows how to apply it, in every copy of it)
PLACEHOLDERS = ("<copyright holders>", "<year>", "<owner>", "<OWNER>", "<YEAR>")
# a template's line, not a notice: dropped from the comparison, and kept where
# the printed text has it
TEMPLATE = re.compile(r"[\[{<](yyyy|year|name of copyright owner|copyright holders)[\]}>]", re.I)
# a notice: a line that begins (after a comment marker or bullet) with one of
# these, and then not with the words of a licence's own sentence that a line
# break put there ("copyright holders be liable", "(c) You must retain")
NOTICE = re.compile(r"\s*(?:(?://|#+|\*|;|--|-|•)\s*)?(portions\s+copyright\b|copyright\b|\(c\)|©)(.*)", re.I)
PROSE = re.compile(r"\W*(holders?|and|notice|licen[cs]e|owner|statement)\b", re.I)

# kept as the files have them, the notices in them: texts with more than the
# licence (ring's two, unrar-rs's UnRAR condition) and licences whose text is
# its own notice
WHOLE_CRATES = {"ring", "unrar-rs"}
WHOLE_KINDS = {"Unicode-3.0", "CDLA-Permissive-2.0", "CC0-1.0", "bzip2-1.0.6"}
# a first line that only names the licence
TITLES = {"MIT": r"(the )?mit licen[cs]e( \(mit\))?", "Zlib": r"(the )?zlib licen[cs]e"}
APACHE_URL = re.compile(r"http://(www\.)?apache\.org/licenses/(license-2\.0(\.txt|\.html)?)?")
# the Apache licence after its terms, as merge_key leaves it
APACHE_APPENDIX = (
    'appendix: how to apply the apache license to your work. to apply the apache license to your work, attach the'
    ' following boilerplate notice, with the fields enclosed by brackets "[]" replaced with your own identifying'
    " information. (don't include the brackets!) the text should be enclosed in the appropriate comment syntax for"
    " the file format. we also recommend that a file or class name and description of purpose be included on the"
    ' same "printed page" as the copyright notice for easier identification within third-party archives.')
APACHE_BOILERPLATE = (
    'licensed under the apache license, version 2.0 (the "license"); you may not use this file except in compliance'
    " with the license. you may obtain a copy of the license at http://www.apache.org/licenses/ unless required by"
    ' applicable law or agreed to in writing, software distributed under the license is distributed on an "as is"'
    " basis, without warranties or conditions of any kind, either express or implied. see the license for the"
    " specific language governing permissions and limitations under the license.")
APACHE_END = "end of terms and conditions"

# the systems the window is built for (.github/workflows/build.yml; js/wasm:
# web/build.sh)
GO_PLATFORMS = ["darwin/amd64", "darwin/arm64", "js/wasm", "linux/amd64", "linux/arm64", "windows/amd64",
                "windows/arm64"]
GO_FILE = re.compile(r"(LICEN[CS]E|COPYING|NOTICE|PATENTS|UNLICENSE)([._-].*)?", re.I)
# the kinds of Go licence texts accepted: (name, what the text has, the
# licence for merge_key; None: kept whole)
GO_KINDS = [
    ("Apache License 2.0 (`Apache-2.0`)", ["Apache License", "Version 2.0, January 2004"], "Apache-2.0"),
    ("MIT License (`MIT`)", ["Permission is hereby granted, free of charge"], "MIT"),
    ("The Unlicense or BSD 3-Clause License (`Unlicense OR BSD-3-Clause`)",
     ["SPDX-License-Identifier: Unlicense OR BSD-3-Clause"], None),
    ('BSD 3-Clause "New" or "Revised" License (`BSD-3-Clause`)',
     ["Redistribution and use in source and binary forms", "Neither the name"], "BSD-3-Clause"),
    ("Go's additional IP rights grant (patents)", ["Additional IP Rights Grant (Patents)"], None),
]


def norm(text):
    lines = [line.rstrip() for line in text.replace("\r\n", "\n").replace("\r", "\n").split("\n")]
    while lines and not lines[0]:
        lines.pop(0)
    while lines and not lines[-1]:
        lines.pop()
    return "\n".join(lines)


def is_notice(line):
    m = NOTICE.match(line)
    if not m or TEMPLATE.search(line):
        return False
    if m.group(1).lower() in ("(c)", "©"):
        return re.match(r"\s*(\d|copyright\b)", m.group(2), re.I) is not None
    return not PROSE.match(m.group(2))


def split(text):
    """the text's lines without its notices, and the notices (a notice goes on
    over the lines below it indented deeper, e.g. a list of holders)"""
    lines = norm(text).split("\n")
    body, notices, i = [], [], 0
    while i < len(lines):
        if not is_notice(lines[i]):
            body.append(lines[i])
            i += 1
            continue
        indent = len(lines[i]) - len(lines[i].lstrip())
        notice = [lines[i].strip()]
        i += 1
        while (i < len(lines) and lines[i].strip() and len(lines[i]) - len(lines[i].lstrip()) > indent
               and not is_notice(lines[i])):
            notice.append(lines[i].strip())
            i += 1
        notices.append("\n".join(notice))
    return body, notices


def tidy(lines):
    """the lines as a text, with no run of blank lines where a notice was"""
    out = []
    for line in lines:
        if line or (out and out[-1]):
            out.append(line)
    return norm("\n".join(out))


def merge_key(lid, body):
    """what two files of a licence must have equal to be one text: the words,
    without the template lines, white space, letter case, the first line if it
    only names the licence, http/https, and the Apache licence's URL form and
    standard appendix (which a crate's file may have or not)"""
    lines = [line for line in body if line.strip() and not TEMPLATE.search(line)]
    if lines and lid in TITLES and re.fullmatch(TITLES[lid], " ".join(lines[0].split()).casefold()):
        lines.pop(0)
    t = re.sub(r"https://", "http://", " ".join(" ".join(lines).split()).casefold())
    if lid.startswith("Apache-2.0"):
        t = APACHE_URL.sub("http://www.apache.org/licenses/", t)
        end = t.find(APACHE_END)
        if end >= 0:
            end += len(APACHE_END)
            tail = t[end:].replace('brackets "{}"', 'brackets "[]"').strip()
            tail = tail.removeprefix(APACHE_APPENDIX).strip().removeprefix(APACHE_BOILERPLATE).strip()
            if not tail:
                t = t[:end]
    return t


def plural(n, word):
    return f"{n} {word}{'' if n == 1 else 's'}"


def joined(values):
    return ", ".join(sorted(v for v in values if v))


def add(groups, kind, lid, text, generic, whole):
    """the group a licence file goes in (whole: by its exact text; else by
    merge_key under lid), and the file's notices"""
    if whole:
        key, form, notices = (kind, "whole", norm(text), generic), norm(text), []
    else:
        body, notices = split(text)
        key, form = (kind, "merged", merge_key(lid, body)), tidy(body)
    g = groups.setdefault(key, {"forms": {}, "names": {}})
    g["forms"].setdefault(form, [set(), False])[1] |= generic
    return g, form, notices


def printed(g):
    """the text printed for a group (the module's docstring)"""
    forms = g["forms"]
    return min(forms, key=lambda t: (not forms[t][1], -len(forms[t][0]), t))


def with_notices(line, notices):
    return [line] + ["  - " + n.replace("\n", "\n    ") for n in notices]


def rust(path):
    """[(kind, text, notes, [crate lines], crates)]"""
    with open(path, encoding="utf-8") as f:
        about = json.load(f)
    names_of = {lic["id"]: lic["name"] for lic in about["licenses"]}
    groups = {}
    # sorted, so that a crate's notices from several versions' files come in the same order every time
    for lic in sorted(about["licenses"], key=lambda lic: (lic["id"], norm(lic["text"]))):
        users = [u["crate"] for u in lic["used_by"] if u["crate"]["source"] is not None]
        if not users:
            continue
        generic = lic["source_path"] is None
        whole = lic["id"] in WHOLE_KINDS or any(c["name"] in WHOLE_CRATES for c in users)
        g, form, notices = add(groups, f"{names_of[lic['id']]} (`{lic['id']}`)", lic["id"], lic["text"], generic,
                               whole)
        for c in users:
            g["forms"][form][0].add(c["name"])
            crate = g["names"].setdefault(c["name"], {"expr": set(), "repo": set(), "authors": set(), "notices": [],
                                                      "no file": False})
            crate["expr"].add(c["license"] or "")
            crate["repo"].add(c["repository"] or "")
            crate["authors"].update(c["authors"])
            crate["notices"] += [n for n in notices if n not in crate["notices"]]
            crate["no file"] |= generic

    out = []
    for key in sorted(groups, key=lambda k: (k[0], min(groups[k]["names"]), printed(groups[k]))):
        g = groups[key]
        text = printed(g)
        whole_generic = key[1] == "whole" and key[3]
        placeholder = any(p in text for p in PLACEHOLDERS)
        notes = []
        if whole_generic:
            notes.append("These crates ship no file with this licence's text: it is the licence's standard text"
                         + (", and their copyright holders are the authors each names." if placeholder else "."))
        lines = []
        for name in sorted(g["names"]):
            c = g["names"][name]
            line = f"- `{name}`" + (f" ({joined(c['expr'])})" if joined(c["expr"]) else "")
            if joined(c["repo"]):
                line += f": {joined(c['repo'])}"
            if c["no file"] and not whole_generic:
                line += "; ships no file with this licence's text"
            if c["no file"] and placeholder:
                line += f"; authors: {joined(c['authors']) or 'none named (see its repository)'}"
            lines += with_notices(line, c["notices"])
        out.append((key[0], text, notes, lines, set(g["names"])))
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

    groups = {}
    for mod, paths in sorted(files.items()):
        for p in sorted(paths):
            with open(p, encoding="utf-8") as f:
                text = norm(f.read())
            kind = next(((k, lid) for k, needles, lid in GO_KINDS if all(n in text for n in needles)), None)
            if kind is None:
                sys.exit(f"{p} ({mod or 'Go'}): not a licence text gen_licenses.py knows: read it and add it to GO_KINDS")
            g, form, notices = add(groups, kind[0], kind[1], text, False, kind[1] is None)
            g["forms"][form][0].add(mod)
            have = g["names"].setdefault(mod, [])
            have += [n for n in notices if n not in have]
    out = []
    for key in sorted(groups, key=lambda k: (k[0], min(groups[k]["names"]), printed(groups[k]))):
        mods = groups[key]["names"]
        lines = [line for m in sorted(mods) for line in with_notices(
            "- Go (its standard library and runtime, in every Go program): https://go.dev" if m == ""
            else f"- `{m}`: https://pkg.go.dev/{m}", mods[m])]
        out.append((key[0], printed(groups[key]), [], lines, {m or "Go" for m in mods}))
    return out


def write(out, sections):
    for kind, text, notes, lines, _ in sections:
        out.append(f"### {kind}\n")
        out.extend(n + "\n" for n in notes)
        out.append("````text\n" + text + "\n````\n")
        out.extend(lines)
        out.append("")


def summary(sections, what, also=""):
    names = {n for s in sections for n in s[4]}
    kinds = {}
    for s in sections:
        kinds.setdefault(s[0], []).append(s)
    out = [f"{plural(len(names), what)}{also}, {plural(len(sections), 'licence text')} (each once, then the"
           f" {what}s under it):\n"]
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
