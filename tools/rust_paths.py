"""The paths a Rust file names, read from its text (no cargo): what
check_layers.py and api_vibe.py share.

Comments and string literals are blanked first (their newlines kept, so a
line number stays right), and `use a::{b, c::{d}}` groups are expanded.
"""
import re

_CHAR = re.compile(r"'(\\.|[^\\'])'")
_RAW = re.compile(r'r(#*)"')


def blank(text):
    """`text` with its comments and strings as spaces, newlines kept."""
    out = []
    i, n = 0, len(text)
    while i < n:
        c = text[i]
        if text.startswith("//", i):
            j = text.find("\n", i)
            j = n if j < 0 else j
            out.append(" " * (j - i))
        elif text.startswith("/*", i):
            j = text.find("*/", i)
            j = n if j < 0 else j + 2
            out.append(re.sub(r"[^\n]", " ", text[i:j]))
        elif c == '"':
            j = i + 1
            while j < n and text[j] != '"':
                j += 2 if text[j] == "\\" else 1
            j += 1
            out.append('"' + re.sub(r"[^\n]", " ", text[i + 1 : j - 1]) + '"')
        elif c == "r" and (m := _RAW.match(text, i)):
            end = '"' + m.group(1)
            j = text.find(end, m.end())
            j = n if j < 0 else j + len(end)
            out.append(re.sub(r"[^\n]", " ", text[i:j]))
        elif c == "'" and (m := _CHAR.match(text, i)):
            j = m.end()
            out.append(" " * (j - i))
        else:
            j = i + 1
            out.append(c)
        i = j
    return "".join(out)


# `#[cfg(test)]`, or `all(…)` with `test` among its own terms (not in a
# `not(test)`)
_TEST = re.compile(r"#\[cfg\((?:test|all\((?:[^()]|\([^()]*\))*?\btest\b(?:[^()]|\([^()]*\))*\))\)\]")


def item_end(text, i):
    """Where the item starting at `i` ends: after its `;`, or its `{ … }`;
    or before a `,` or a closing bracket of what holds it (a field, a
    variant, a match arm, an argument)."""
    depth, angle, n = 0, 0, len(text)
    while i < n:
        c = text[i]
        if c in "([":
            depth += 1
        elif c in ")]":
            if depth == 0:
                return i
            depth -= 1
        elif depth == 0 and c == "<":
            angle += 1
        elif depth == 0 and c == ">" and angle and text[i - 1] not in "-=":
            angle -= 1
        elif depth == 0 and c == ";":
            return i + 1
        elif depth == 0 and c == "}" or depth == 0 and angle == 0 and c == ",":
            return i
        elif depth == 0 and c == "{":
            level = 0
            while i < n:
                level += {"{": 1, "}": -1}.get(text[i], 0)
                if level == 0:
                    return i + 1
                i += 1
        i += 1
    return n


def cut_tests(text):
    """Blanked `text` with each item under `#[cfg(test)]` (or `all(test,
    …)`) as spaces, newlines kept."""
    for m in reversed(list(_TEST.finditer(text))):
        j = item_end(text, m.end())
        text = text[: m.start()] + re.sub(r"[^\n]", " ", text[m.start() : j]) + text[j:]
    return text


def expand(path):
    """`a::{b, c::{d, e}}` as [`a::b`, `a::c::d`, `a::c::e`]."""
    i = path.find("{")
    if i < 0:
        return [path]
    depth, j = 0, i
    for j in range(i, len(path)):
        depth += {"{": 1, "}": -1}.get(path[j], 0)
        if depth == 0:
            break
    head, body, tail = path[:i], path[i + 1 : j], path[j + 1 :]
    parts, depth, cur = [], 0, ""
    for ch in body:
        depth += {"{": 1, "}": -1}.get(ch, 0)
        if ch == "," and depth == 0:
            parts.append(cur)
            cur = ""
        else:
            cur += ch
    parts.append(cur)
    return [head + e + tail for p in parts if p for e in expand(p)]


def paths(text, roots):
    """(line, path) for every path in blanked `text` that starts with one of
    `roots` (`crate`, `super`, `self`, a crate's name), groups expanded,
    an `as name` dropped."""
    pat = re.compile(r"\b(%s)((?:\s*::\s*(?:\w+|\{|\*))+)" % "|".join(roots))
    for m in pat.finditer(text):
        start, i, s = m.start(), m.end(), m.group(0)
        if s.rstrip().endswith("{"):
            depth = 1
            while i < len(text) and depth:
                depth += {"{": 1, "}": -1}.get(text[i], 0)
                i += 1
            s = text[start:i]
        s = re.sub(r"\s+as\s+\w+", "", s)
        s = re.sub(r"\s+", "", s)
        line = text.count("\n", 0, start) + 1
        for e in expand(s):
            yield line, e
