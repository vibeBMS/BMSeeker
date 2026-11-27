#!/usr/bin/env python3
"""tools/api_vibe.py <vibeBMS checkout> > tests/api_vibe.rs: the test that
names every BMSeeker item vibeBMS's crates name (crates/**/*.rs, their
tests too), so BMSeeker's own tests fail where vibe would.

A `bmseeker::…` path (comments and strings left out, `use` groups
expanded) is split where BMSeeker's modules end: what follows is an item
(`use … as _`), an enum's variant (`use` too), or a type's associated item
(`let _ = …;`, a trait's in a function generic over it). So is a path
that starts with a name a file imports from BMSeeker (`Effect::Http` after
`use bmseeker::install::job::Effect`, `install::take` after `use
bmseeker::install`), directly or through a `pub use` of a vibe module
(`crate::placement::Located`). A struct's, or an enum variant's, named
fields in a pattern or a literal (`Unserved { error, at }`) are named in a
pattern of their own. An associated function with type parameters of its
own, or of its type's, cannot be named without a call: only its type is. Each is named under
the `cfg` BMSeeker gives its module (src/lib.rs) or its definition, `test`
left out (an integration test sees the library as vibe does).

What a text scan cannot see: a field or method reached through a value
(`served.head`, `lib.add(…)`), names brought in by a glob or by `super::`
or `self::` paths, a `pub use` of a `pub use`, and a type alias.
"""
import os
import re
import sys
from collections import defaultdict

sys.dont_write_bytecode = True
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from rust_paths import blank, expand, paths  # noqa: E402

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
// (a field named in a pattern as `_`, beside `..`, is named all the same)
#![allow(clippy::unneeded_wildcard_pattern)]
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
        return None
    cfgs = [c for c in re.findall(r"#\[cfg\((.*)\)\]", m.group(1)) if without_test(c)]
    generic = False
    if m.group(2) == "fn":
        rest = text[m.end():]
        sig = rest[: rest.find("{")]
        generic = sig.lstrip().startswith("<") or "impl " in sig
    return (without_test(cfgs[-1]) if cfgs else None), generic


def imports(text):
    """name -> path of each `use` in blanked `text` (groups expanded; an
    `as` name, `self` for its module)."""
    out = {}
    for m in re.finditer(r"\buse\s+([^;{]*(?:\{[^;]*)?);", text):
        for e in expand(re.sub(r"\s+", " ", m.group(1))):
            path, _, alias = e.partition(" as ")
            segs = re.sub(r"\s*::\s*", "::", path).strip().split("::")
            if segs[-1] == "self":
                segs = segs[:-1]
            name = alias.strip() or segs[-1]
            if segs and segs[-1] != "*" and name != "_":
                out[name] = "::".join(segs)
    return out


# a path, or what may be a struct's pattern or literal (`Name {`)
CHAIN = re.compile(r"(?<![\w:$])(\w+(?:\s*::\s*\w+)*)(\s*\{)?")
# what comes before a type that is followed by a block, not by its fields
NOT_FIELDS = re.compile(r"(->|\b(?:impl|for|dyn|where|struct|enum|union|trait|mod|fn))\s*$")


def fields(text, i):
    """The field names of the `{ … }` at `i` (`a`, `b: _`, not `..`)."""
    depth, part, parts = 0, "", []
    for c in text[i + 1:]:
        if c in "([{":
            depth += 1
        elif c in ")]}":
            if depth == 0:
                break
            depth -= 1
        if c == "," and depth == 0:
            parts.append(part)
            part = ""
        else:
            part += c
    parts.append(part)
    names = []
    for p in parts:
        m = re.fullmatch(r"\s*(?:ref\s+)?(?:mut\s+)?(\w+)\s*(:.*)?", p, re.S)
        if m:
            names.append(m.group(1))
    return names


def crate_files(vibe):
    """(crate's src folder, module path, file) of each .rs file of vibe's
    crates' sources and tests."""
    for r, _, names in os.walk(os.path.join(vibe, "crates")):
        for n in names:
            if n.endswith(".rs"):
                f = os.path.join(r, n)
                rel = os.path.relpath(f, os.path.join(vibe, "crates")).split(os.sep)
                src = os.path.join(vibe, "crates", rel[0], "src")
                mod = os.path.relpath(f, src)[: -len(".rs")].split(os.sep)
                if mod[-1] in ("mod", "lib", "main"):
                    mod = mod[:-1]
                yield src, "::".join(mod), f


def named(vibe):
    """(module, item path) -> vibe files that name it; (struct or variant
    path) -> its field names -> vibe files."""
    mods = modules()
    out = defaultdict(set)
    pats = defaultdict(lambda: defaultdict(set))
    files = [(src, mod, f, blank(open(f, encoding="utf-8").read())) for src, mod, f in crate_files(vibe)]
    # (crate, module, name) -> the BMSeeker path a vibe module's `pub use` gives it
    reexports = {}
    for src, mod, f, text in files:
        for m in re.finditer(r"\bpub(?:\([\w:]+\))?\s+use\s+bmseeker\b[^;]*;", text):
            for name, path in imports(m.group(0)[m.group(0).index("use"):]).items():
                reexports[(src, mod, name)] = path

    def resolve(src, local, path):
        segs = path.split("::")
        if segs[0] in local:
            segs = local[segs[0]].split("::") + segs[1:]
        if segs[0] == "crate":
            for k in range(len(segs) - 1, 0, -1):
                to = reexports.get((src, "::".join(segs[1:k]), segs[k]))
                if to:
                    segs = to.split("::") + segs[k + 1:]
                    break
        return segs if segs[0] == "bmseeker" else None

    def record(segs, f):
        # (`use a::b::{self, C}` names the module `a::b`)
        segs = segs[1:-1] if segs[-1] == "self" else segs[1:]
        for k in range(len(segs), 0, -1):
            if "::".join(segs[:k]) in mods:
                return out[("::".join(segs[:k]), "::".join(segs[k:]))].add(os.path.relpath(f, vibe)) or True
        out[("", "::".join(segs))].add(os.path.relpath(f, vibe))

    for src, mod, f, text in files:
        for _, p in paths(text, ["bmseeker"]):
            record(p.split("::"), f)
        # (a name of a module or a type: a function's name is no path's
        # start, `zip::…` after `use bmseeker::testkit::zip` is the crate's)
        local = {n: p for n, p in imports(text).items()
                 if (r := resolve(src, {}, p)) and ("::".join(r[1:]) in mods or r[-1][:1].isupper())}
        uses = [m for m in re.finditer(r"\buse\s+[^;]*;", text)]
        for m in CHAIN.finditer(text):
            if any(u.start() <= m.start() < u.end() for u in uses):
                continue
            path = re.sub(r"\s+", "", m.group(1))
            segs = resolve(src, local, path)
            if not segs or (segs[0] == "bmseeker" and path.split("::")[0] == "bmseeker"):
                if not (segs and m.group(2)):
                    continue
            elif len(path.split("::")) > 1:
                record(segs, f)
            if m.group(2) and segs and segs[-1][:1].isupper() and not NOT_FIELDS.search(text[: m.start()]):
                for name in fields(text, m.end() - 1):
                    pats["::".join(segs)][name].add(os.path.relpath(f, vibe))
    return mods, out, pats


def generics(text, name):
    """The type arguments a use of the type `name` (defined in `text`)
    needs: `()` for each type parameter, `'static` for each lifetime."""
    m = re.search(r"\bpub\s+(?:struct|enum)\s+%s\s*<([^>]*)>" % re.escape(name), text)
    if not m:
        return ""
    args = ["'static" if a.strip().startswith("'") else "()" for a in m.group(1).split(",") if a.strip()]
    return "<%s>" % ", ".join(args)


def braced(text, owner, name):
    """Whether `name` is a struct with named fields (owner None) or a
    variant of the enum `owner` with them, in `text`."""
    if owner is None:
        return re.search(r"\bpub\s+struct\s+%s\b[^;({]*\{" % re.escape(name), text) is not None
    m = re.search(r"\bpub\s+enum\s+%s\b[^{]*\{" % re.escape(owner), text)
    if not m:
        return False
    depth, i = 1, m.end()
    while i < len(text) and depth:
        depth += {"{": 1, "}": -1}.get(text[i], 0)
        if depth == 1 and re.match(r"\b%s\s*\{" % re.escape(name), text[i:]) and not re.match(r"\w", text[i - 1]):
            return True
        i += 1
    return False


def main():
    vibe = sys.argv[1]
    mods, items, pats = named(vibe)
    top = module_cfgs()
    # (a module's text with its submodules': an item may be defined below
    # where it is named; raw, as a cfg's strings count)
    raw = {m: open(f, encoding="utf-8").read() for m, f in mods.items()}
    texts = {m: "\n".join(t for n, t in sorted(raw.items()) if n == m or n.startswith(m + "::") or not m)
             for m in mods}
    every = blank(texts[""])
    uses, lets, traits, unnamed = {}, [], defaultdict(list), []
    for (mod, item) in sorted(items):
        if item == "*":
            sys.exit("%s::* (in %s): name its items instead" % (mod, ", ".join(sorted(items[(mod, item)]))))
        base = "bmseeker::" + mod if mod else "bmseeker"
        cfgs = [top.get(mod.split("::")[0])]
        parts = item.split("::") if item else []
        owner = parts[-2] if len(parts) > 1 else None
        # (defined in the module, else where it `pub use`s it from)
        found = parts and (definition(texts[mod], parts[-1], owner) or definition(texts[""], parts[-1], owner))
        cfg, generic = found or (None, False)
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
        elif generic or "()" in generics(every, owner[-1]):
            uses["%s::%s" % (base, "::".join(owner))] = attr
            unnamed.append(path)
        else:
            lets.append("    " + attr.replace("\n", "\n    ") + "let _ = %s;" % path)
    out = [HEADER]
    if unnamed:
        out.append("// (only their types: with type parameters of their own or of their\n"
                   "// type, they cannot be named without a call) %s\n" % ", ".join(unnamed))
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
    # (each struct's or variant's fields vibe names, in a pattern)
    n = 0
    for path in sorted(pats):
        segs = path.split("::")[1:]
        k = next((k for k in range(len(segs), 0, -1) if "::".join(segs[:k]) in mods), 0)
        mod, item = "::".join(segs[:k]), segs[k:]
        # (defined anywhere: a module may `pub use` another's)
        text = every
        variant = len(item) > 1 and braced(text, item[-2], item[-1])
        if not variant and not braced(text, None, item[-1]):
            continue
        ty = "::".join(["bmseeker"] + segs[:-1] if variant else ["bmseeker"] + segs)
        cfgs = [c for c in (top.get(mod.split("::")[0]), (definition(texts[""], ty.split("::")[-1]) or [None])[0]) if c]
        attr = "#[cfg(%s)]\n" % cfgs[0] if len(cfgs) == 1 else "#[cfg(all(%s))]\n" % ", ".join(cfgs) if cfgs else ""
        out.append("")
        out.append("%sfn _fields_%d(v: %s%s) {" % (attr, n, ty, generics(text, ty.split("::")[-1])))
        body = "".join("%s: _, " % f for f in sorted(pats[path])) + ".."
        line = "    let _ = matches!(v, %s { %s });" % (path, body)
        # (as rustfmt lays it out: a pattern's fields on a line only while
        # they are short)
        if len(body) <= 18 and len(line) <= 100:
            out.append(line)
        else:
            out.append("    let _ = matches!(\n        v,\n        %s {" % path)
            out.extend("            %s: _," % f for f in sorted(pats[path]))
            out.append("            ..\n        }\n    );")
        out.append("}")
        n += 1
    for i, (trait, methods) in enumerate(sorted(traits.items())):
        out.append("")
        out.append("fn _trait_%d<T: %s + ?Sized>() {" % (i, trait))
        for attr, m in methods:
            out.append("    " + attr.replace("\n", "\n    ") + "let _ = T::%s;" % m)
        out.append("}")
    print("\n".join(out))


if __name__ == "__main__":
    main()
