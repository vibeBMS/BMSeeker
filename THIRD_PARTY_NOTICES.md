# Third-party notices

BMSeeker (`MIT OR Apache-2.0`, see LICENSE-MIT and LICENSE-APACHE) is derived in
part from the works below. The Rust crates (and, for the GUI, Go modules) it is
built from keep their own licences, listed with their texts in
THIRD_PARTY_LICENSES.md (made by `tools/gen_licenses.sh`). The programs carry
these four files, compiled in: `bmseeker licenses` prints them, and the window
and the web page show them in Settings (Licences).

## BeMusicSeeker (unofficial fork)

The folder naming patterns (`src/naming.rs`), the title/artist comparison
(`src/titles.rs`) and the chart placement and package rules (`src/placement.rs`,
`src/package.rs`, `src/export.rs`) follow the BeMusicSeeker unofficial fork
(https://github.com/Neeted/bemusicseeker-unofficial-fork), some of its regular
expressions as they are. Its licence:

```
Copyright (c) 2026 ﾏﾝﾊｯﾀﾝｶﾞｯﾌｪ@CyanHowan
Copyright (c) 2023 Sayaka / 黒皇帝
Copyright (c) 2015 ribbit.xyz

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
```

## UnRAR (through unrar-rs, Apache-2.0)

```
UnRAR source code may be used in any software to handle RAR archives without limitations free of charge, but cannot be used to develop RAR (WinRAR) compatible archiver and to re-create RAR compression algorithm, which is proprietary. Distribution of modified UnRAR source code in separate form or as a part of other software is permitted, provided that full text of this paragraph, starting from "UnRAR source code" words, is included in license, or in documentation if license is not available, and in source code comments of resulting package.
```
