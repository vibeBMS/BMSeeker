#!/usr/bin/env python3
"""tools/api_vibe.py <vibeBMS checkout> > tests/api_vibe.rs: the test that
names every BMSeeker item vibeBMS's crates name (crates/**/*.rs, their
tests too), so BMSeeker's own tests fail where vibe would.

A `bmseeker::…` path (comments and strings left out, `use` groups
expanded) is split where BMSeeker's modules end: what follows is an item
(`use … as _`), an enum's variant (`use` too), or a type's associated item
(`let _ = …;`, a trait's in a function generic over it). An associated
function with type parameters of its own cannot be named without a call:
only its type is. Each is named under the `cfg` BMSeeker gives its module
(src/lib.rs) or its definition, `test` left out (an integration test sees
the library as vibe does).
"""
import os
import re
import sys
from collections import defaultdict

sys.dont_write_bytecode = True
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from rust_paths import blank, paths  # noqa: E402

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
SRC = os.path.join(ROOT, "src")

HEADER = """\
//! Every BMSeeker item vibeBMS (the player built on BMSeeker) names, named
//! once: a rename, a move without a `pub use` at the old path, or a
//! narrower visibility fails here, in BMSeeker's own tests. A change to
//! this file means vibe must change too. Made by
//! `python3 tools/api_vibe.py <vibeBMS checkout> > tests/api_vibe.rs`
//! (it names; it does not check signatures).
#![allow(unused_imports)]
"""


def modules():
    """BMSeeker's module paths -> their source file."""
    out = {}
    for r, _, names in os.walk(SRC):
        for n in names:
            if n.endswith(".rs") and n != "tests.rs":
                rel = os.path.relpath(os.path.join(r, n), SRC)[: -len(".rs")].split(os.sep)
                if rel[-1] in ("mod", "lib"):
                    rel = rel[:-1]
                out["::".join(rel)] = os.path.join(r, n)
    return out


def without_test(cfg):
    """`any(test, X)` -> `X`; `test` alone -> None."""
    m = re.fullmatch(r"any\(test,\s*(.+)\)", cfg)
    cfg = m.group(1) if m else cfg
    return None if cfg == "test" else cfg


def module_cfgs():
    """Top-level module -> its cfg in lib.rs."""
    lib = open(os.path.join(SRC, "lib.rs"), encoding="utf-8").read()
    return {m.group(2): without_test(m.group(1))
            for m in re.finditer(r"#\[cfg\((.+)\)\]\s*pub mod (\w+);", lib)}


ITEM = r"((?:[ \t]*#\[.*\]\n)*)[ \t]*pub(?:\([^)]*\))?\s+(?:const\s+|unsafe\s+)?(fn|const|static|struct|enum|trait|type|mod)\s+%s\b"


def definition(text, name, owner=None):
    """The cfg of `name`'s definition in `text` (in an `impl` of `owner`
    when given), and whether it is a function with type parameters of its
    own (`fn f<T>`, `impl Trait` arguments)."""
    if owner:
        blocks = re.finditer(r"^impl\b[^{;]*?\b%s\b[^{;]*\{(.*?)^\}" % re.escape(owner), text, re.M | re.S)
        text = "\n".join(b.group(1) for b in blocks)
    m = re.search(ITEM % re.escape(name), text)
    if not m:
        return None, False
    cfgs = [c for c in re.findall(r"#\[cfg\((.*)\)\]", m.group(1)) if without_test(c)]
    generic = False
    if m.group(2) == "fn":
        rest = text[m.end():]
        sig = rest[: rest.find("{")]
        generic = sig.lstrip().startswith("<") or "impl " in sig
    return (without_test(cfgs[-1]) if cfgs else None), generic


def named(vibe):
    """(module, item path) -> vibe files that name it."""
    mods = modules()
    out = defaultdict(set)
    for r, _, names in os.walk(os.path.join(vibe, "crates")):
        for n in names:
            if not n.endswith(".rs"):
                continue
            f = os.path.join(r, n)
            text = blank(open(f, encoding="utf-8").read())
            for _, p in paths(text, ["bmseeker"]):
                segs = p.split("::")[1:]
                for k in range(len(segs), 0, -1):
                    if "::".join(segs[:k]) in mods:
                        out[("::".join(segs[:k]), "::".join(segs[k:]))].add(os.path.relpath(f, vibe))
                        break
                else:
                    out[("", "::".join(segs))].add(os.path.relpath(f, vibe))
    return mods, out


def main():
    vibe = sys.argv[1]
    mods, items = named(vibe)
    top = module_cfgs()
    # (a module's text with its submodules': an item may be defined below
    # where it is named; raw, as a cfg's strings count)
    raw = {m: open(f, encoding="utf-8").read() for m, f in mods.items()}
    texts = {m: "\n".join(t for n, t in sorted(raw.items()) if n == m or n.startswith(m + "::") or not m)
             for m in mods}
    uses, lets, traits, unnamed = {}, [], defaultdict(list), []
    for (mod, item) in sorted(items):
        if item == "*":
            sys.exit("%s::* (in %s): name its items instead" % (mod, ", ".join(sorted(items[(mod, item)]))))
        base = "bmseeker::" + mod if mod else "bmseeker"
        cfgs = [top.get(mod.split("::")[0])]
        parts = item.split("::") if item else []
        owner = parts[-2] if len(parts) > 1 else None
        cfg, generic = definition(texts[mod], parts[-1], owner) if parts else (None, False)
        cfgs.append(cfg)
        cfgs = [c for c in cfgs if c]
        attr = "#[cfg(%s)]\n" % cfgs[0] if len(cfgs) == 1 else "#[cfg(all(%s))]\n" % ", ".join(cfgs) if cfgs else ""
        path = "%s::%s" % (base, item) if item else base
        if len(parts) <= 1:
            uses[path] = attr
            continue
        owner, last = parts[:-1], parts[-1]
        is_trait = re.search(r"\bpub\s+trait\s+%s\b" % re.escape(owner[-1]), texts[mod])
        if re.fullmatch(r"[A-Z][A-Za-z0-9]*[a-z][A-Za-z0-9]*", last):
            uses[path] = attr
        elif is_trait:
            traits["%s::%s" % (base, "::".join(owner))].append((attr, last))
        elif generic:
            uses["%s::%s" % (base, "::".join(owner))] = attr
            unnamed.append(path)
        else:
            lets.append("    " + attr.replace("\n", "\n    ") + "let _ = %s;" % path)
    out = [HEADER]
    if unnamed:
        out.append("// (only their types: with type parameters of their own, they cannot be\n"
                   "// named without a call) %s\n" % ", ".join(unnamed))
    # (in rustfmt's order: by segment, snake_case, then CamelCase, then
    # SCREAMING_CASE)
    def case(s):
        return 0 if not s[0].isupper() else 2 if len(s) > 1 and s == s.upper() else 1

    for path in sorted(uses, key=lambda p: [(case(s), s) for s in p.split("::")]):
        out.append(uses[path] + "use %s as _;" % path)
    out.append("")
    out.append("fn _associated() {")
    out.extend(lets)
    out.append("}")
    for i, (trait, methods) in enumerate(sorted(traits.items())):
        out.append("")
        out.append("fn _trait_%d<T: %s + ?Sized>() {" % (i, trait))
        for attr, m in methods:
            out.append("    " + attr.replace("\n", "\n    ") + "let _ = T::%s;" % m)
        out.append("}")
    print("\n".join(out))


if __name__ == "__main__":
    main()
