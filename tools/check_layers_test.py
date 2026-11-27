#!/usr/bin/env python3
"""Tests of tools/check_layers.py on copies of src with a change made:
python3 tools/check_layers_test.py (CI's test job runs it)."""
import os
import shutil
import subprocess
import sys
import tempfile
import unittest

TOOLS = os.path.dirname(os.path.abspath(__file__))
SRC = os.path.join(os.path.dirname(TOOLS), "src")
CHECK = os.path.join(TOOLS, "check_layers.py")

# a use of a higher layer's item (text is base, install a higher layer)
UP = "crate::install::safe_name"


def check(changes):
    """check_layers.py's exit code and output on a copy of src with
    `changes` (path in src -> text added at its end, or a new file's)."""
    with tempfile.TemporaryDirectory() as d:
        src = os.path.join(d, "src")
        shutil.copytree(SRC, src)
        for path, text in changes.items():
            with open(os.path.join(src, path), "a", encoding="utf-8") as f:
                f.write("\n" + text + "\n")
        out = subprocess.run([sys.executable, "-B", CHECK, src], capture_output=True, text=True)
        return out.returncode, out.stdout


class Layers(unittest.TestCase):
    def assertFails(self, changes, says):
        code, out = check(changes)
        self.assertEqual(code, 1, out)
        self.assertIn(says, out)

    def assertPasses(self, changes):
        code, out = check(changes)
        self.assertEqual(code, 0, out)

    def test_the_tree(self):
        self.assertPasses({})

    def test_an_upward_use(self):
        self.assertFails({"text/probe.rs": "fn up() { %s(\"a\"); }" % UP}, "text::probe (base) uses install::safe_name")

    def test_code_after_a_test_module(self):
        self.assertFails({"text/probe.rs": "#[cfg(test)]\nmod early {}\nfn up() { %s(\"a\"); }" % UP},
                         "uses install::safe_name")

    def test_test_items_outside_a_test_module(self):
        self.assertPasses({"text/probe.rs": "#[cfg(test)]\nfn helper() { %s(\"a\"); }\n"
                                      "#[cfg(test)]\nuse %s as _;\n"
                                      "#[cfg(all(test, unix))]\nmod more { fn f() { %s(\"a\"); } }" % (UP, UP, UP)})

    def test_not_test_is_real_code(self):
        self.assertFails({"text/probe.rs": "#[cfg(all(unix, not(test)))]\nfn up() { %s(\"a\"); }" % UP},
                         "uses install::safe_name")
        self.assertFails({"text/probe.rs": "#[cfg(any(test, unix))]\nfn up() { %s(\"a\"); }" % UP},
                         "uses install::safe_name")

    def test_a_test_field_variant_or_arm_ends_at_its_comma(self):
        for item in (
            "struct S {\n    #[cfg(test)]\n    held: std::collections::HashMap<u8, u8>,\n    n: u8,\n}\n"
            "static UP: fn(&str) -> String = %s;" % UP,
            "enum E {\n    #[cfg(test)]\n    T(u8, u8),\n    R,\n}\nfn up() { %s(\"a\"); }" % UP,
            "fn f(e: u8) -> u8 {\n    match e {\n        #[cfg(test)]\n        0 => 1,\n        _ => 2,\n    }\n}\n"
            "fn up() { %s(\"a\"); }" % UP,
            "struct S {\n    #[cfg(test)]\n    last: u8\n}\nfn up() { %s(\"a\"); }" % UP,
            "fn f(n: u8, #[cfg(test)] t: u8) {}\nfn up() { %s(\"a\"); }" % UP,
        ):
            self.assertFails({"text/probe.rs": item}, "uses install::safe_name")

    def test_a_root_item(self):
        self.assertFails({"lib.rs": "pub use install::safe_name;", "text/probe.rs": "fn up() { crate::safe_name(\"a\"); }"},
                         "text::probe (base) uses crate::safe_name (front ends)")

    def test_a_modules_path(self):
        self.assertFails({"text/probe.rs": "#[path = \"install/low.rs\"]\nmod low;"}, "#[path]")
        self.assertFails({"text/probe.rs": "#[cfg_attr(unix, path = \"install/low.rs\")]\nmod low;"}, "#[path]")

    def test_a_path_a_macro_builds(self):
        self.assertFails({"text/probe.rs": "macro_rules! up {\n    ($m:ident) => { crate::$m::safe_name(\"a\") };\n}"},
                         "crate::$")
        self.assertFails({"text/probe.rs": "macro_rules! up {\n    () => { $crate::install::safe_name(\"a\") };\n}"},
                         "uses install::safe_name")


if __name__ == "__main__":
    unittest.main()
