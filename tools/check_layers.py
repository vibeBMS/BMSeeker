#!/usr/bin/env python3
"""tools/check_layers.py [src]: no module uses an item of a higher layer
(DESIGN.md, Layers), but the uses listed in UPWARD (CI's test job runs it).

Every .rs file under src is a module (a/b.rs and a/b/mod.rs are a::b), in
the layer of the longest prefix of its path that LAYERS names. Its paths
from `crate::`, `super::` or `self::` (`bmseeker::` in src/bin) are read
with comments, strings and tests left out (tests.rs files; a file from its
`#[cfg(test)]` items); the target is the longest prefix that is a module
(lib.rs's own items: the crate's root), the rest the item. A use whose
target's layer is higher than its own fails, unless UPWARD lists it; a use
UPWARD lists that no longer exists fails too, so the list is shortened when
one goes. What a value's method reaches without naming its type is not
seen; nor is what the script cannot place, so that fails too: a module
declared with `#[path]` (it would be read in its file's layer, not its
`mod`'s), and a path a macro builds from a token (`crate::$m::…`).
"""
import os
import re
import sys
from collections import defaultdict

sys.dont_write_bytecode = True
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from rust_paths import blank, cut_tests, paths  # noqa: E402

# lowest first; a module's layer is that of the longest prefix named here
LAYERS = [
    ("base", ["text", "json", "progress", "fs", "net"]),
    ("parsing and data", ["chart", "titles", "naming", "table", "manbow", "config", "links"]),
    ("network model", ["refusal"]),
    ("hosts and sources", ["hosts", "bmssearch", "sources"]),
    ("archives", ["unpack"]),
    ("library", ["library", "placement", "package"]),
    ("install", ["install", "import"]),
    ("drivers", ["native", "web"]),
    ("front ends", ["cli", "probe", "export", "testkit", "bin", ""]),  # "": lib.rs
]

# (from module, to module, items, why it stays)
UPWARD = [
    ("refusal", "hosts", ["Context", "Link", "NOT_WEB", "DRIVE_QUOTA", "drive_quota", "service_of"],
     "a run's refusals live in its hosts::Context, which Guarded keeps to; a refusal is charged to "
     "a host's service, and the guard holds back what the hosts' rules refuse (no web link, Drive's quota)"),
    ("hosts::drive", "unpack", ["is_archive"], "the archive formats' names, beside the formats unpack reads"),
    ("hosts::mega", "unpack", ["is_archive"], "the same"),
    ("sources", "unpack", ["is_archive"], "the same"),
]


def module_of(src, path):
    rel = os.path.relpath(path, src)[: -len(".rs")].split(os.sep)
    if rel[-1] in ("mod", "lib", "main"):
        rel = rel[:-1]
    return "::".join(rel)


def layer_of(module):
    best = None
    for i, (_, mods) in enumerate(LAYERS):
        for m in mods:
            if module == m or (m and module.startswith(m + "::")):
                if best is None or len(m) > len(best[1]):
                    best = (i, m)
    return best[0] if best else None


def uses(src):
    """(from module, to module, item) -> ["file:line", ...]"""
    files = [os.path.join(r, n) for r, _, ns in os.walk(src) for n in ns if n.endswith(".rs")]
    modules = {module_of(src, f) for f in files}
    found = defaultdict(list)
    unread = []
    for f in sorted(files):
        text = blank(open(f, encoding="utf-8").read())
        at = lambda m: "%s:%d" % (os.path.relpath(f, src), text.count("\n", 0, m.start()) + 1)  # noqa: E731
        unread += ["%s: a module's #[path] (keep each module in its own file)" % at(m)
                   for m in re.finditer(r"#!?\[[^\]]*\bpath\s*=", text)]
        unread += ["%s: a path a macro builds from a token (crate::$…; name the module)" % at(m)
                   for m in re.finditer(r"\bcrate\s*::\s*\$", text)]
        if os.path.basename(f) == "tests.rs":
            continue
        mod = module_of(src, f)
        roots = ["crate", "super", "self"]
        if mod.split("::")[0] == "bin":
            roots.append("bmseeker")
        text = cut_tests(text)
        for line, p in paths(text, roots):
            segs = p.split("::")
            here = mod.split("::") if mod else []
            if segs[0] in ("crate", "bmseeker"):
                target = segs[1:]
            elif segs[0] == "self":
                target = here + segs[1:]
            else:
                k = 0
                while k < len(segs) and segs[k] == "super":
                    k += 1
                target = here[: len(here) - k] + segs[k:]
            for k in range(len(target), -1, -1):
                if "::".join(target[:k]) in modules:
                    to = "::".join(target[:k])
                    item = "::".join(target[k:]) or "(module)"
                    if to != mod:
                        found[(mod, to, item.split("::")[0])].append("%s:%d" % (os.path.relpath(f, src), line))
                    break
    return modules, found, unread


def main():
    root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    src = sys.argv[1] if len(sys.argv) > 1 else os.path.join(root, "src")
    modules, found, bad = uses(src)
    bad += ["%s: no layer (add it to LAYERS in tools/check_layers.py)" % m
            for m in sorted(modules) if layer_of(m) is None]
    listed = {(a, b, i) for a, b, items, _ in UPWARD for i in items}
    upward = {k: at for k, at in found.items() if layer_of(k[1]) > layer_of(k[0])}
    for (a, b, i), at in sorted(upward.items()):
        if (a, b, i) not in listed:
            bad.append("%s (%s) uses %s::%s (%s), a higher layer: %s"
                       % (a, LAYERS[layer_of(a)][0], b or "crate", i, LAYERS[layer_of(b)][0], " ".join(at)))
    for a, b, i in sorted(listed - upward.keys()):
        bad.append("%s no longer uses %s::%s: take it out of UPWARD in tools/check_layers.py" % (a, b, i))
    for line in bad:
        print(line)
    if bad:
        return 1
    print("layers: %d modules, %d upward uses, each listed" % (len(modules), len(upward)))
    return 0


if __name__ == "__main__":
    sys.exit(main())
