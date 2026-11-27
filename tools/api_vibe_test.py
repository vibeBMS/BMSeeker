#!/usr/bin/env python3
"""Tests of tools/api_vibe.py on a made-up vibeBMS checkout:
python3 tools/api_vibe_test.py (CI's test job runs it)."""
import os
import subprocess
import sys
import tempfile
import unittest

TOOLS = os.path.dirname(os.path.abspath(__file__))
GENERATE = os.path.join(TOOLS, "api_vibe.py")

APP = """\
use bmseeker::install::job::{Effect, Fed, Unserved};
use bmseeker::install;
use bmseeker::library::Library as Lib;
use bmseeker::testkit::zip;
pub use bmseeker::placement::Located;

fn run(e: Effect) -> Fed {
    match e {
        Effect::Download { target, slot } => Fed::Downloaded(Err(Unserved { error, at: None })),
        Effect::Sleep(_) => Fed::Woke,
        // Effect::Http in a comment, "Fed::Http" in a string
        _ => todo!(),
    }
}

impl Effect {
    fn local(&self) {}
}

fn f() -> Lib {
    let _ = install::take;
    let _ = zip::result::ZipError::FileNotFound;
    Lib::from_catalog(x, [])
}
"""

OTHER = """\
use crate::placement::Located;

fn g(l: Located) {
    if let Located::Move(d) = l {}
    let _ = crate::placement::Located::NoSong;
}
"""


def generate(files):
    """tests/api_vibe.rs as made from a checkout of `files` (path -> text)."""
    with tempfile.TemporaryDirectory() as d:
        for path, text in files.items():
            os.makedirs(os.path.dirname(os.path.join(d, path)), exist_ok=True)
            with open(os.path.join(d, path), "w", encoding="utf-8") as f:
                f.write(text)
        out = subprocess.run([sys.executable, "-B", GENERATE, d], capture_output=True, text=True, check=True)
        return out.stdout


class Generated(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.out = generate({"crates/app/src/lib.rs": "", "crates/app/src/placement.rs": APP,
                            "crates/app/src/web.rs": OTHER})

    def test_variants_after_an_import(self):
        for v in ("Effect::Download", "Effect::Sleep", "Fed::Downloaded", "Fed::Woke"):
            self.assertIn("use bmseeker::install::job::%s as _;" % v, self.out)
        self.assertNotIn("Effect::Http", self.out)
        self.assertNotIn("Fed::Http", self.out)

    def test_through_a_modules_pub_use(self):
        self.assertIn("use bmseeker::placement::Located::Move as _;", self.out)
        self.assertIn("use bmseeker::placement::Located::NoSong as _;", self.out)

    def test_fields_in_patterns_and_literals(self):
        self.assertIn("bmseeker::install::job::Effect::Download {\n            slot: _,\n            target: _,",
                      self.out)
        self.assertIn("bmseeker::install::job::Unserved {\n            at: _,\n            error: _,", self.out)
        # (an impl's block is no struct's fields)
        self.assertNotIn("Effect { ", self.out)

    def test_modules_aliases_and_functions(self):
        self.assertIn("use bmseeker::install::take as _;", self.out)
        self.assertIn("bmseeker::library::Library::from_catalog", self.out)
        # (`zip` names a function here: `zip::…` is the zip crate's)
        self.assertNotIn("ZipError", self.out)


if __name__ == "__main__":
    unittest.main()
