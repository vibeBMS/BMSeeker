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

## Rust crates

194 crates, 18 licence texts (each once, then the crates under it):

- Apache License 2.0 (`Apache-2.0`): 150 crates, 5 texts
- BSD 3-Clause "New" or "Revised" License (`BSD-3-Clause`): 2 crates, 2 texts
- Community Data License Agreement Permissive 2.0 (`CDLA-Permissive-2.0`): 2 crates, 1 text
- Creative Commons Zero v1.0 Universal (`CC0-1.0`): 1 crate, 1 text
- ISC License (`ISC`): 3 crates, 3 texts
- MIT License (`MIT`): 18 crates, 2 texts
- Unicode License v3 (`Unicode-3.0`): 20 crates, 2 texts
- bzip2 and libbzip2 License v1.0.6 (`bzip2-1.0.6`): 1 crate, 1 text
- zlib License (`Zlib`): 2 crates, 1 text

### Apache License 2.0 (`Apache-2.0`)

````text
Apache License
Version 2.0, January 2004
http://www.apache.org/licenses/

TERMS AND CONDITIONS FOR USE, REPRODUCTION, AND DISTRIBUTION

1. Definitions.

"License" shall mean the terms and conditions for use, reproduction, and distribution as defined by Sections 1 through 9 of this document.

"Licensor" shall mean the copyright owner or entity authorized by the copyright owner that is granting the License.

"Legal Entity" shall mean the union of the acting entity and all other entities that control, are controlled by, or are under common control with that entity. For the purposes of this definition, "control" means (i) the power, direct or indirect, to cause the direction or management of such entity, whether by contract or otherwise, or (ii) ownership of fifty percent (50%) or more of the outstanding shares, or (iii) beneficial ownership of such entity.

"You" (or "Your") shall mean an individual or Legal Entity exercising permissions granted by this License.

"Source" form shall mean the preferred form for making modifications, including but not limited to software source code, documentation source, and configuration files.

"Object" form shall mean any form resulting from mechanical transformation or translation of a Source form, including but not limited to compiled object code, generated documentation, and conversions to other media types.

"Work" shall mean the work of authorship, whether in Source or Object form, made available under the License, as indicated by a copyright notice that is included in or attached to the work (an example is provided in the Appendix below).

"Derivative Works" shall mean any work, whether in Source or Object form, that is based on (or derived from) the Work and for which the editorial revisions, annotations, elaborations, or other modifications represent, as a whole, an original work of authorship. For the purposes of this License, Derivative Works shall not include works that remain separable from, or merely link (or bind by name) to the interfaces of, the Work and Derivative Works thereof.

"Contribution" shall mean any work of authorship, including the original version of the Work and any modifications or additions to that Work or Derivative Works thereof, that is intentionally submitted to Licensor for inclusion in the Work by the copyright owner or by an individual or Legal Entity authorized to submit on behalf of the copyright owner. For the purposes of this definition, "submitted" means any form of electronic, verbal, or written communication sent to the Licensor or its representatives, including but not limited to communication on electronic mailing lists, source code control systems, and issue tracking systems that are managed by, or on behalf of, the Licensor for the purpose of discussing and improving the Work, but excluding communication that is conspicuously marked or otherwise designated in writing by the copyright owner as "Not a Contribution."

"Contributor" shall mean Licensor and any individual or Legal Entity on behalf of whom a Contribution has been received by Licensor and subsequently incorporated within the Work.

2. Grant of Copyright License. Subject to the terms and conditions of this License, each Contributor hereby grants to You a perpetual, worldwide, non-exclusive, no-charge, royalty-free, irrevocable copyright license to reproduce, prepare Derivative Works of, publicly display, publicly perform, sublicense, and distribute the Work and such Derivative Works in Source or Object form.

3. Grant of Patent License. Subject to the terms and conditions of this License, each Contributor hereby grants to You a perpetual, worldwide, non-exclusive, no-charge, royalty-free, irrevocable (except as stated in this section) patent license to make, have made, use, offer to sell, sell, import, and otherwise transfer the Work, where such license applies only to those patent claims licensable by such Contributor that are necessarily infringed by their Contribution(s) alone or by combination of their Contribution(s) with the Work to which such Contribution(s) was submitted. If You institute patent litigation against any entity (including a cross-claim or counterclaim in a lawsuit) alleging that the Work or a Contribution incorporated within the Work constitutes direct or contributory patent infringement, then any patent licenses granted to You under this License for that Work shall terminate as of the date such litigation is filed.

4. Redistribution. You may reproduce and distribute copies of the Work or Derivative Works thereof in any medium, with or without modifications, and in Source or Object form, provided that You meet the following conditions:

     (a) You must give any other recipients of the Work or Derivative Works a copy of this License; and

     (b) You must cause any modified files to carry prominent notices stating that You changed the files; and

     (c) You must retain, in the Source form of any Derivative Works that You distribute, all copyright, patent, trademark, and attribution notices from the Source form of the Work, excluding those notices that do not pertain to any part of the Derivative Works; and

     (d) If the Work includes a "NOTICE" text file as part of its distribution, then any Derivative Works that You distribute must include a readable copy of the attribution notices contained within such NOTICE file, excluding those notices that do not pertain to any part of the Derivative Works, in at least one of the following places: within a NOTICE text file distributed as part of the Derivative Works; within the Source form or documentation, if provided along with the Derivative Works; or, within a display generated by the Derivative Works, if and wherever such third-party notices normally appear. The contents of the NOTICE file are for informational purposes only and do not modify the License. You may add Your own attribution notices within Derivative Works that You distribute, alongside or as an addendum to the NOTICE text from the Work, provided that such additional attribution notices cannot be construed as modifying the License.

     You may add Your own copyright statement to Your modifications and may provide additional or different license terms and conditions for use, reproduction, or distribution of Your modifications, or for any such Derivative Works as a whole, provided Your use, reproduction, and distribution of the Work otherwise complies with the conditions stated in this License.

5. Submission of Contributions. Unless You explicitly state otherwise, any Contribution intentionally submitted for inclusion in the Work by You to the Licensor shall be under the terms and conditions of this License, without any additional terms or conditions. Notwithstanding the above, nothing herein shall supersede or modify the terms of any separate license agreement you may have executed with Licensor regarding such Contributions.

6. Trademarks. This License does not grant permission to use the trade names, trademarks, service marks, or product names of the Licensor, except as required for reasonable and customary use in describing the origin of the Work and reproducing the content of the NOTICE file.

7. Disclaimer of Warranty. Unless required by applicable law or agreed to in writing, Licensor provides the Work (and each Contributor provides its Contributions) on an "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied, including, without limitation, any warranties or conditions of TITLE, NON-INFRINGEMENT, MERCHANTABILITY, or FITNESS FOR A PARTICULAR PURPOSE. You are solely responsible for determining the appropriateness of using or redistributing the Work and assume any risks associated with Your exercise of permissions under this License.

8. Limitation of Liability. In no event and under no legal theory, whether in tort (including negligence), contract, or otherwise, unless required by applicable law (such as deliberate and grossly negligent acts) or agreed to in writing, shall any Contributor be liable to You for damages, including any direct, indirect, special, incidental, or consequential damages of any character arising as a result of this License or out of the use or inability to use the Work (including but not limited to damages for loss of goodwill, work stoppage, computer failure or malfunction, or any and all other commercial damages or losses), even if such Contributor has been advised of the possibility of such damages.

9. Accepting Warranty or Additional Liability. While redistributing the Work or Derivative Works thereof, You may choose to offer, and charge a fee for, acceptance of support, warranty, indemnity, or other liability obligations and/or rights consistent with this License. However, in accepting such obligations, You may act only on Your own behalf and on Your sole responsibility, not on behalf of any other Contributor, and only if You agree to indemnify, defend, and hold each Contributor harmless for any liability incurred by, or claims asserted against, such Contributor by reason of your accepting any such warranty or additional liability.

END OF TERMS AND CONDITIONS

APPENDIX: How to apply the Apache License to your work.

To apply the Apache License to your work, attach the following boilerplate notice, with the fields enclosed by brackets "[]" replaced with your own identifying information. (Don't include the brackets!)  The text should be enclosed in the appropriate comment syntax for the file format. We also recommend that a file or class name and description of purpose be included on the same "printed page" as the copyright notice for easier identification within third-party archives.

Copyright [yyyy] [name of copyright owner]

Licensed under the Apache License, Version 2.0 (the "License");
you may not use this file except in compliance with the License.
You may obtain a copy of the License at

http://www.apache.org/licenses/LICENSE-2.0

Unless required by applicable law or agreed to in writing, software
distributed under the License is distributed on an "AS IS" BASIS,
WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
See the License for the specific language governing permissions and
limitations under the License.
````

- `adler2` (0BSD OR MIT OR Apache-2.0): https://github.com/oyvindln/adler2
- `aes` (MIT OR Apache-2.0): https://github.com/RustCrypto/block-ciphers
- `anstream` (MIT OR Apache-2.0): https://github.com/rust-cli/anstyle.git
- `anstyle` (MIT OR Apache-2.0): https://github.com/rust-cli/anstyle.git
- `anstyle-parse` (MIT OR Apache-2.0): https://github.com/rust-cli/anstyle.git
- `anstyle-query` (MIT OR Apache-2.0): https://github.com/rust-cli/anstyle.git
- `anstyle-wincon` (MIT OR Apache-2.0): https://github.com/rust-cli/anstyle.git
- `arrayvec` (MIT OR Apache-2.0): https://github.com/bluss/arrayvec
- `autocfg` (Apache-2.0 OR MIT): https://github.com/cuviper/autocfg
- `base64` (MIT OR Apache-2.0): https://github.com/marshallpierce/rust-base64
- `bitflags` (MIT OR Apache-2.0): https://github.com/bitflags/bitflags
- `block-buffer` (MIT OR Apache-2.0): https://github.com/RustCrypto/utils
- `block-padding` (MIT OR Apache-2.0): https://github.com/RustCrypto/utils
- `bumpalo` (MIT OR Apache-2.0): https://github.com/fitzgen/bumpalo
- `bytemuck` (Zlib OR Apache-2.0 OR MIT): https://github.com/Lokathor/bytemuck
- `bytemuck_derive` (Zlib OR Apache-2.0 OR MIT): https://github.com/Lokathor/bytemuck
- `bzip2` (MIT OR Apache-2.0): https://github.com/trifectatechfoundation/bzip2-rs
- `cbc` (MIT OR Apache-2.0): https://github.com/RustCrypto/block-modes
- `cc` (MIT OR Apache-2.0): https://github.com/rust-lang/cc-rs
- `cfg-if` (MIT OR Apache-2.0): https://github.com/rust-lang/cfg-if
- `cipher` (MIT OR Apache-2.0): https://github.com/RustCrypto/traits
- `clap` (MIT OR Apache-2.0): https://github.com/clap-rs/clap
  - Copyright Individual contributors
- `clap_builder` (MIT OR Apache-2.0): https://github.com/clap-rs/clap
  - Copyright Individual contributors
- `clap_derive` (MIT OR Apache-2.0): https://github.com/clap-rs/clap
  - Copyright Individual contributors
- `clap_lex` (MIT OR Apache-2.0): https://github.com/clap-rs/clap
  - Copyright Individual contributors
- `cmov` (Apache-2.0 OR MIT): https://github.com/RustCrypto/utils
- `colorchoice` (MIT OR Apache-2.0): https://github.com/rust-cli/anstyle.git
- `const-oid` (Apache-2.0 OR MIT): https://github.com/RustCrypto/formats
- `constant_time_eq` (CC0-1.0 OR MIT-0 OR Apache-2.0): https://codeberg.org/cesarb/constant_time_eq, https://github.com/cesarb/constant_time_eq; ships no file with this licence's text
- `core-foundation` (MIT OR Apache-2.0): https://github.com/servo/core-foundation-rs
- `core-foundation-sys` (MIT OR Apache-2.0): https://github.com/servo/core-foundation-rs
- `core_detect` (MIT OR Apache-2.0): https://github.com/thomcc/core_detect
- `cpubits` (MIT OR Apache-2.0): https://github.com/RustCrypto/utils
- `cpufeatures` (MIT OR Apache-2.0): https://github.com/RustCrypto/utils
- `crc-turbo` ((MIT OR Apache-2.0) AND Zlib): https://github.com/scryer-media/crc-turbo
- `crc32fast` (MIT OR Apache-2.0): https://github.com/srijs/rust-crc32fast
- `crossbeam-deque` (MIT OR Apache-2.0): https://github.com/crossbeam-rs/crossbeam
- `crossbeam-epoch` (MIT OR Apache-2.0): https://github.com/crossbeam-rs/crossbeam
- `crossbeam-utils` (MIT OR Apache-2.0): https://github.com/crossbeam-rs/crossbeam
- `crypto-common` (MIT OR Apache-2.0): https://github.com/RustCrypto/traits
- `ctutils` (Apache-2.0 OR MIT): https://github.com/RustCrypto/utils
- `delharc` (MIT OR Apache-2.0): https://github.com/royaltm/rust-delharc
  - Copyright 2023 Rafal Michalski
- `digest` (MIT OR Apache-2.0): https://github.com/RustCrypto/traits
- `displaydoc` (MIT OR Apache-2.0): https://github.com/yaahc/displaydoc
- `either` (MIT OR Apache-2.0): https://github.com/rayon-rs/either
- `encoding_rs` ((Apache-2.0 OR MIT) AND BSD-3-Clause): https://github.com/hsivonen/encoding_rs
- `equivalent` (Apache-2.0 OR MIT): https://github.com/indexmap-rs/equivalent
- `errno` (MIT OR Apache-2.0): https://github.com/lambda-fairy/rust-errno
- `fastrand` (Apache-2.0 OR MIT): https://github.com/smol-rs/fastrand
- `filetime` (MIT OR Apache-2.0): https://github.com/alexcrichton/filetime
- `find-msvc-tools` (MIT OR Apache-2.0): https://github.com/rust-lang/cc-rs
- `flate2` (MIT OR Apache-2.0): https://github.com/rust-lang/flate2-rs
- `form_urlencoded` (MIT OR Apache-2.0): https://github.com/servo/rust-url
- `futures-core` (MIT OR Apache-2.0): https://github.com/rust-lang/futures-rs
  - Copyright (c) 2016 Alex Crichton
  - Copyright (c) 2017 The Tokio Authors
- `futures-task` (MIT OR Apache-2.0): https://github.com/rust-lang/futures-rs
  - Copyright (c) 2016 Alex Crichton
  - Copyright (c) 2017 The Tokio Authors
- `futures-util` (MIT OR Apache-2.0): https://github.com/rust-lang/futures-rs
  - Copyright (c) 2016 Alex Crichton
  - Copyright (c) 2017 The Tokio Authors
- `getrandom` (MIT OR Apache-2.0): https://github.com/rust-random/getrandom
- `hashbrown` (MIT OR Apache-2.0): https://github.com/rust-lang/hashbrown
- `heck` (MIT OR Apache-2.0): https://github.com/withoutboats/heck
- `hmac` (MIT OR Apache-2.0): https://github.com/RustCrypto/MACs
- `http` (MIT OR Apache-2.0): https://github.com/hyperium/http
  - Copyright 2017 http-rs authors
- `httparse` (MIT OR Apache-2.0): https://github.com/seanmonstar/httparse
- `hybrid-array` (MIT OR Apache-2.0): https://github.com/RustCrypto/hybrid-array
- `iana-time-zone` (MIT OR Apache-2.0): https://github.com/strawlab/iana-time-zone
  - Copyright 2020 Andrew Straw
- `idna` (MIT OR Apache-2.0): https://github.com/servo/rust-url/
- `idna_adapter` (Apache-2.0 OR MIT): https://github.com/hsivonen/idna_adapter
- `indexmap` (Apache-2.0 OR MIT): https://github.com/indexmap-rs/indexmap
- `inout` (MIT OR Apache-2.0): https://github.com/RustCrypto/utils
- `is_terminal_polyfill` (MIT OR Apache-2.0): https://github.com/polyfill-rs/is_terminal_polyfill
- `itoa` (MIT OR Apache-2.0): https://github.com/dtolnay/itoa; ships no file with this licence's text
- `js-sys` (MIT OR Apache-2.0): https://github.com/wasm-bindgen/wasm-bindgen/tree/master/crates/js-sys
- `libc` (MIT OR Apache-2.0): https://github.com/rust-lang/libc; ships no file with this licence's text
- `linux-raw-sys` (Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT): https://github.com/sunfishcode/linux-raw-sys
- `log` (MIT OR Apache-2.0): https://github.com/rust-lang/log
- `lzma-rust2` (Apache-2.0): https://github.com/hasenbanck/lzma-rust2/
- `md-5` (MIT OR Apache-2.0): https://github.com/RustCrypto/hashes
- `memmap2` (MIT OR Apache-2.0): https://github.com/RazrFalcon/memmap2-rs
  - Copyright [2015] [Dan Burkert]
- `miniz_oxide` (MIT OR Zlib OR Apache-2.0): https://github.com/Frommi/miniz_oxide/tree/master/miniz_oxide; ships no file with this licence's text
- `multiversion_no_op` (Apache-2.0 OR MIT): https://github.com/hsivonen/multiversion_no_op
- `num-traits` (MIT OR Apache-2.0): https://github.com/rust-num/num-traits
- `once_cell` (MIT OR Apache-2.0): https://github.com/matklad/once_cell
- `once_cell_polyfill` (MIT OR Apache-2.0): https://github.com/polyfill-rs/once_cell_polyfill
- `openssl-probe` (MIT OR Apache-2.0): https://github.com/rustls/openssl-probe
- `percent-encoding` (MIT OR Apache-2.0): https://github.com/servo/rust-url/
- `pin-project-lite` (Apache-2.0 OR MIT): https://github.com/taiki-e/pin-project-lite; ships no file with this licence's text
- `ppmd-turbo` (Apache-2.0): https://github.com/scryer-media/ppmd-turbo
- `proc-macro2` (MIT OR Apache-2.0): https://github.com/dtolnay/proc-macro2; ships no file with this licence's text
- `quote` (MIT OR Apache-2.0): https://github.com/dtolnay/quote; ships no file with this licence's text
- `rayon` (MIT OR Apache-2.0): https://github.com/rayon-rs/rayon
- `rayon-core` (MIT OR Apache-2.0): https://github.com/rayon-rs/rayon
- `reedsolomon-rs` (Apache-2.0): https://github.com/scryer-media/rarpar
- `rustix` (Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT): https://github.com/bytecodealliance/rustix
- `rustls` (Apache-2.0 OR ISC OR MIT): https://github.com/rustls/rustls
- `rustls-native-certs` (Apache-2.0 OR ISC OR MIT): https://github.com/rustls/rustls-native-certs
- `rustls-pki-types` (MIT OR Apache-2.0): https://github.com/rustls/pki-types
  - Copyright 2023 Dirkjan Ochtman
- `rustls-platform-verifier` (MIT OR Apache-2.0): https://github.com/rustls/rustls-platform-verifier
- `rustversion` (MIT OR Apache-2.0): https://github.com/dtolnay/rustversion; ships no file with this licence's text
- `scopeguard` (MIT OR Apache-2.0): https://github.com/bluss/scopeguard
- `security-framework` (MIT OR Apache-2.0): https://github.com/kornelski/rust-security-framework
- `security-framework-sys` (MIT OR Apache-2.0): https://github.com/kornelski/rust-security-framework
- `serde` (MIT OR Apache-2.0): https://github.com/serde-rs/serde; ships no file with this licence's text
- `serde_core` (MIT OR Apache-2.0): https://github.com/serde-rs/serde; ships no file with this licence's text
- `serde_derive` (MIT OR Apache-2.0): https://github.com/serde-rs/serde; ships no file with this licence's text
- `serde_json` (MIT OR Apache-2.0): https://github.com/serde-rs/json; ships no file with this licence's text
- `sevenz-rust2` (Apache-2.0): https://github.com/hasenbanck/sevenz-rust
- `sha2` (MIT OR Apache-2.0): https://github.com/RustCrypto/hashes
- `shlex` (MIT OR Apache-2.0): https://github.com/comex/rust-shlex; ships no file with this licence's text
- `simdutf8` (MIT OR Apache-2.0): https://github.com/rusticstuff/simdutf8; ships no file with this licence's text
- `smallvec` (MIT OR Apache-2.0): https://github.com/servo/rust-smallvec
- `stable_deref_trait` (MIT OR Apache-2.0): https://github.com/storyyeller/stable_deref_trait
- `syn` (MIT OR Apache-2.0): https://github.com/dtolnay/syn; ships no file with this licence's text
- `tar` (MIT OR Apache-2.0): https://github.com/composefs/tar-rs
- `tempfile` (MIT OR Apache-2.0): https://github.com/Stebalien/tempfile
- `thiserror` (MIT OR Apache-2.0): https://github.com/dtolnay/thiserror; ships no file with this licence's text
- `thiserror-impl` (MIT OR Apache-2.0): https://github.com/dtolnay/thiserror; ships no file with this licence's text
- `tinyvec` (Zlib OR Apache-2.0 OR MIT): https://github.com/Lokathor/tinyvec
- `typed-path` (MIT OR Apache-2.0): https://github.com/chipsenkbeil/typed-path; ships no file with this licence's text
- `typenum` (MIT OR Apache-2.0): https://github.com/paholg/typenum
  - Copyright 2014 Paho Lurie-Gregg
- `unicode-ident` ((MIT OR Apache-2.0) AND Unicode-3.0): https://github.com/dtolnay/unicode-ident; ships no file with this licence's text
- `unicode-normalization` ((MIT OR Apache-2.0) AND Unicode-3.0): https://github.com/unicode-rs/unicode-normalization
- `ureq` (MIT OR Apache-2.0): https://github.com/algesten/ureq
- `ureq-proto` (MIT OR Apache-2.0): https://github.com/algesten/ureq-proto
- `url` (MIT OR Apache-2.0): https://github.com/servo/rust-url
- `utf8-zero` (MIT OR Apache-2.0): https://github.com/algesten/utf8-zero; ships no file with this licence's text
- `utf8_iter` (Apache-2.0 OR MIT): https://github.com/hsivonen/utf8_iter
- `utf8parse` (Apache-2.0 OR MIT): https://github.com/alacritty/vte; ships no file with this licence's text
- `utf_types` (Apache-2.0 OR MIT): https://github.com/hsivonen/utf_types
- `version_check` (MIT OR Apache-2.0): https://github.com/SergioBenitez/version_check
- `wasm-bindgen` (MIT OR Apache-2.0): https://github.com/wasm-bindgen/wasm-bindgen
- `wasm-bindgen-macro` (MIT OR Apache-2.0): https://github.com/wasm-bindgen/wasm-bindgen/tree/master/crates/macro
- `wasm-bindgen-macro-support` (MIT OR Apache-2.0): https://github.com/wasm-bindgen/wasm-bindgen/tree/main/crates/macro-support
- `wasm-bindgen-shared` (MIT OR Apache-2.0): https://github.com/wasm-bindgen/wasm-bindgen/tree/master/crates/shared
- `web-sys` (MIT OR Apache-2.0): https://github.com/wasm-bindgen/wasm-bindgen/tree/master/crates/web-sys
- `web-time` (MIT OR Apache-2.0): https://github.com/daxpedda/web-time
  - Copyright 2023 dAxpeDDa
- `windows-core` (MIT OR Apache-2.0): https://github.com/microsoft/windows-rs
  - Copyright (c) Microsoft Corporation.
- `windows-implement` (MIT OR Apache-2.0): https://github.com/microsoft/windows-rs
  - Copyright (c) Microsoft Corporation.
- `windows-interface` (MIT OR Apache-2.0): https://github.com/microsoft/windows-rs
  - Copyright (c) Microsoft Corporation.
- `windows-link` (MIT OR Apache-2.0): https://github.com/microsoft/windows-rs
  - Copyright (c) Microsoft Corporation.
- `windows-result` (MIT OR Apache-2.0): https://github.com/microsoft/windows-rs
  - Copyright (c) Microsoft Corporation.
- `windows-strings` (MIT OR Apache-2.0): https://github.com/microsoft/windows-rs
  - Copyright (c) Microsoft Corporation.
- `windows-sys` (MIT OR Apache-2.0): https://github.com/microsoft/windows-rs
  - Copyright (c) Microsoft Corporation.
- `windows-targets` (MIT OR Apache-2.0): https://github.com/microsoft/windows-rs
  - Copyright (c) Microsoft Corporation.
- `windows_aarch64_msvc` (MIT OR Apache-2.0): https://github.com/microsoft/windows-rs
  - Copyright (c) Microsoft Corporation.
- `windows_x86_64_gnu` (MIT OR Apache-2.0): https://github.com/microsoft/windows-rs
  - Copyright (c) Microsoft Corporation.
- `windows_x86_64_msvc` (MIT OR Apache-2.0): https://github.com/microsoft/windows-rs
  - Copyright (c) Microsoft Corporation.
- `zeroize` (Apache-2.0 OR MIT): https://github.com/RustCrypto/utils
- `zeroize_derive` (Apache-2.0 OR MIT): https://github.com/RustCrypto/utils

### Apache License 2.0 (`Apache-2.0`)

````text
Rust-chrono is dual-licensed under The MIT License [1] and
Apache 2.0 License [2]. Copyright (c) 2014--2026, Kang Seonghoon and
contributors.

Nota Bene: This is same as the Rust Project's own license.

[1]: <http://opensource.org/licenses/MIT>, which is reproduced below:

~~~~
The MIT License (MIT)

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in
all copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN
THE SOFTWARE.
~~~~

[2]: <http://www.apache.org/licenses/LICENSE-2.0>, which is reproduced below:

~~~~
                              Apache License
                        Version 2.0, January 2004
                     http://www.apache.org/licenses/

TERMS AND CONDITIONS FOR USE, REPRODUCTION, AND DISTRIBUTION

1. Definitions.

   "License" shall mean the terms and conditions for use, reproduction,
   and distribution as defined by Sections 1 through 9 of this document.

   "Licensor" shall mean the copyright owner or entity authorized by
   the copyright owner that is granting the License.

   "Legal Entity" shall mean the union of the acting entity and all
   other entities that control, are controlled by, or are under common
   control with that entity. For the purposes of this definition,
   "control" means (i) the power, direct or indirect, to cause the
   direction or management of such entity, whether by contract or
   otherwise, or (ii) ownership of fifty percent (50%) or more of the
   outstanding shares, or (iii) beneficial ownership of such entity.

   "You" (or "Your") shall mean an individual or Legal Entity
   exercising permissions granted by this License.

   "Source" form shall mean the preferred form for making modifications,
   including but not limited to software source code, documentation
   source, and configuration files.

   "Object" form shall mean any form resulting from mechanical
   transformation or translation of a Source form, including but
   not limited to compiled object code, generated documentation,
   and conversions to other media types.

   "Work" shall mean the work of authorship, whether in Source or
   Object form, made available under the License, as indicated by a
   copyright notice that is included in or attached to the work
   (an example is provided in the Appendix below).

   "Derivative Works" shall mean any work, whether in Source or Object
   form, that is based on (or derived from) the Work and for which the
   editorial revisions, annotations, elaborations, or other modifications
   represent, as a whole, an original work of authorship. For the purposes
   of this License, Derivative Works shall not include works that remain
   separable from, or merely link (or bind by name) to the interfaces of,
   the Work and Derivative Works thereof.

   "Contribution" shall mean any work of authorship, including
   the original version of the Work and any modifications or additions
   to that Work or Derivative Works thereof, that is intentionally
   submitted to Licensor for inclusion in the Work by the copyright owner
   or by an individual or Legal Entity authorized to submit on behalf of
   the copyright owner. For the purposes of this definition, "submitted"
   means any form of electronic, verbal, or written communication sent
   to the Licensor or its representatives, including but not limited to
   communication on electronic mailing lists, source code control systems,
   and issue tracking systems that are managed by, or on behalf of, the
   Licensor for the purpose of discussing and improving the Work, but
   excluding communication that is conspicuously marked or otherwise
   designated in writing by the copyright owner as "Not a Contribution."

   "Contributor" shall mean Licensor and any individual or Legal Entity
   on behalf of whom a Contribution has been received by Licensor and
   subsequently incorporated within the Work.

2. Grant of Copyright License. Subject to the terms and conditions of
   this License, each Contributor hereby grants to You a perpetual,
   worldwide, non-exclusive, no-charge, royalty-free, irrevocable
   copyright license to reproduce, prepare Derivative Works of,
   publicly display, publicly perform, sublicense, and distribute the
   Work and such Derivative Works in Source or Object form.

3. Grant of Patent License. Subject to the terms and conditions of
   this License, each Contributor hereby grants to You a perpetual,
   worldwide, non-exclusive, no-charge, royalty-free, irrevocable
   (except as stated in this section) patent license to make, have made,
   use, offer to sell, sell, import, and otherwise transfer the Work,
   where such license applies only to those patent claims licensable
   by such Contributor that are necessarily infringed by their
   Contribution(s) alone or by combination of their Contribution(s)
   with the Work to which such Contribution(s) was submitted. If You
   institute patent litigation against any entity (including a
   cross-claim or counterclaim in a lawsuit) alleging that the Work
   or a Contribution incorporated within the Work constitutes direct
   or contributory patent infringement, then any patent licenses
   granted to You under this License for that Work shall terminate
   as of the date such litigation is filed.

4. Redistribution. You may reproduce and distribute copies of the
   Work or Derivative Works thereof in any medium, with or without
   modifications, and in Source or Object form, provided that You
   meet the following conditions:

   (a) You must give any other recipients of the Work or
       Derivative Works a copy of this License; and

   (b) You must cause any modified files to carry prominent notices
       stating that You changed the files; and

   (c) You must retain, in the Source form of any Derivative Works
       that You distribute, all copyright, patent, trademark, and
       attribution notices from the Source form of the Work,
       excluding those notices that do not pertain to any part of
       the Derivative Works; and

   (d) If the Work includes a "NOTICE" text file as part of its
       distribution, then any Derivative Works that You distribute must
       include a readable copy of the attribution notices contained
       within such NOTICE file, excluding those notices that do not
       pertain to any part of the Derivative Works, in at least one
       of the following places: within a NOTICE text file distributed
       as part of the Derivative Works; within the Source form or
       documentation, if provided along with the Derivative Works; or,
       within a display generated by the Derivative Works, if and
       wherever such third-party notices normally appear. The contents
       of the NOTICE file are for informational purposes only and
       do not modify the License. You may add Your own attribution
       notices within Derivative Works that You distribute, alongside
       or as an addendum to the NOTICE text from the Work, provided
       that such additional attribution notices cannot be construed
       as modifying the License.

   You may add Your own copyright statement to Your modifications and
   may provide additional or different license terms and conditions
   for use, reproduction, or distribution of Your modifications, or
   for any such Derivative Works as a whole, provided Your use,
   reproduction, and distribution of the Work otherwise complies with
   the conditions stated in this License.

5. Submission of Contributions. Unless You explicitly state otherwise,
   any Contribution intentionally submitted for inclusion in the Work
   by You to the Licensor shall be under the terms and conditions of
   this License, without any additional terms or conditions.
   Notwithstanding the above, nothing herein shall supersede or modify
   the terms of any separate license agreement you may have executed
   with Licensor regarding such Contributions.

6. Trademarks. This License does not grant permission to use the trade
   names, trademarks, service marks, or product names of the Licensor,
   except as required for reasonable and customary use in describing the
   origin of the Work and reproducing the content of the NOTICE file.

7. Disclaimer of Warranty. Unless required by applicable law or
   agreed to in writing, Licensor provides the Work (and each
   Contributor provides its Contributions) on an "AS IS" BASIS,
   WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or
   implied, including, without limitation, any warranties or conditions
   of TITLE, NON-INFRINGEMENT, MERCHANTABILITY, or FITNESS FOR A
   PARTICULAR PURPOSE. You are solely responsible for determining the
   appropriateness of using or redistributing the Work and assume any
   risks associated with Your exercise of permissions under this License.

8. Limitation of Liability. In no event and under no legal theory,
   whether in tort (including negligence), contract, or otherwise,
   unless required by applicable law (such as deliberate and grossly
   negligent acts) or agreed to in writing, shall any Contributor be
   liable to You for damages, including any direct, indirect, special,
   incidental, or consequential damages of any character arising as a
   result of this License or out of the use or inability to use the
   Work (including but not limited to damages for loss of goodwill,
   work stoppage, computer failure or malfunction, or any and all
   other commercial damages or losses), even if such Contributor
   has been advised of the possibility of such damages.

9. Accepting Warranty or Additional Liability. While redistributing
   the Work or Derivative Works thereof, You may choose to offer,
   and charge a fee for, acceptance of support, warranty, indemnity,
   or other liability obligations and/or rights consistent with this
   License. However, in accepting such obligations, You may act only
   on Your own behalf and on Your sole responsibility, not on behalf
   of any other Contributor, and only if You agree to indemnify,
   defend, and hold each Contributor harmless for any liability
   incurred by, or claims asserted against, such Contributor by reason
   of your accepting any such warranty or additional liability.

END OF TERMS AND CONDITIONS

APPENDIX: How to apply the Apache License to your work.

   To apply the Apache License to your work, attach the following
   boilerplate notice, with the fields enclosed by brackets "[]"
   replaced with your own identifying information. (Don't include
   the brackets!)  The text should be enclosed in the appropriate
   comment syntax for the file format. We also recommend that a
   file or class name and description of purpose be included on the
   same "printed page" as the copyright notice for easier
   identification within third-party archives.

Copyright [yyyy] [name of copyright owner]

Licensed under the Apache License, Version 2.0 (the "License");
you may not use this file except in compliance with the License.
You may obtain a copy of the License at

	http://www.apache.org/licenses/LICENSE-2.0

Unless required by applicable law or agreed to in writing, software
distributed under the License is distributed on an "AS IS" BASIS,
WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
See the License for the specific language governing permissions and
limitations under the License.
~~~~
````

- `chrono` (MIT OR Apache-2.0): https://github.com/chronotope/chrono
  - Copyright (c) 2014, Kang Seonghoon.

### Apache License 2.0 (`Apache-2.0`)

````text
                                 Apache License
                           Version 2.0, January 2004
                        http://www.apache.org/licenses/

   TERMS AND CONDITIONS FOR USE, REPRODUCTION, AND DISTRIBUTION

   1. Definitions.

      "License" shall mean the terms and conditions for use, reproduction,
      and distribution as defined by Sections 1 through 9 of this document.

      "Licensor" shall mean the copyright owner or entity authorized by
      the copyright owner that is granting the License.

      "Legal Entity" shall mean the union of the acting entity and all
      other entities that control, are controlled by, or are under common
      control with that entity. For the purposes of this definition,
      "control" means (i) the power, direct or indirect, to cause the
      direction or management of such entity, whether by contract or
      otherwise, or (ii) ownership of fifty percent (50%) or more of the
      outstanding shares, or (iii) beneficial ownership of such entity.

      "You" (or "Your") shall mean an individual or Legal Entity
      exercising permissions granted by this License.

      "Source" form shall mean the preferred form for making modifications,
      including but not limited to software source code, documentation
      source, and configuration files.

      "Object" form shall mean any form resulting from mechanical
      transformation or translation of a Source form, including but
      not limited to compiled object code, generated documentation,
      and conversions to other media types.

      "Work" shall mean the work of authorship, whether in Source or
      Object form, made available under the License, as indicated by a
      copyright notice that is included in or attached to the work
      (an example is provided in the Appendix below).

      "Derivative Works" shall mean any work, whether in Source or Object
      form, that is based on (or derived from) the Work and for which the
      editorial revisions, annotations, elaborations, or other modifications
      represent, as a whole, an original work of authorship. For the purposes
      of this License, Derivative Works shall not include works that remain
      separable from, or merely link (or bind by name) to the interfaces of,
      the Work and Derivative Works thereof.

      "Contribution" shall mean any work of authorship, including
      the original version of the Work and any modifications or additions
      to that Work or Derivative Works thereof, that is intentionally
      submitted to Licensor for inclusion in the Work by the copyright owner
      or by an individual or Legal Entity authorized to submit on behalf of
      the copyright owner. For the purposes of this definition, "submitted"
      means any form of electronic, verbal, or written communication sent
      to the Licensor or its representatives, including but not limited to
      communication on electronic mailing lists, source code control systems,
      and issue tracking systems that are managed by, or on behalf of, the
      Licensor for the purpose of discussing and improving the Work, but
      excluding communication that is conspicuously marked or otherwise
      designated in writing by the copyright owner as "Not a Contribution."

      "Contributor" shall mean Licensor and any individual or Legal Entity
      on behalf of whom a Contribution has been received by Licensor and
      subsequently incorporated within the Work.

   2. Grant of Copyright License. Subject to the terms and conditions of
      this License, each Contributor hereby grants to You a perpetual,
      worldwide, non-exclusive, no-charge, royalty-free, irrevocable
      copyright license to reproduce, prepare Derivative Works of,
      publicly display, publicly perform, sublicense, and distribute the
      Work and such Derivative Works in Source or Object form.

   3. Grant of Patent License. Subject to the terms and conditions of
      this License, each Contributor hereby grants to You a perpetual,
      worldwide, non-exclusive, no-charge, royalty-free, irrevocable
      (except as stated in this section) patent license to make, have made,
      use, offer to sell, sell, import, and otherwise transfer the Work,
      where such license applies only to those patent claims licensable
      by such Contributor that are necessarily infringed by their
      Contribution(s) alone or by combination of their Contribution(s)
      with the Work to which such Contribution(s) was submitted. If You
      institute patent litigation against any entity (including a
      cross-claim or counterclaim in a lawsuit) alleging that the Work
      or a Contribution incorporated within the Work constitutes direct
      or contributory patent infringement, then any patent licenses
      granted to You under this License for that Work shall terminate
      as of the date such litigation is filed.

   4. Redistribution. You may reproduce and distribute copies of the
      Work or Derivative Works thereof in any medium, with or without
      modifications, and in Source or Object form, provided that You
      meet the following conditions:

      (a) You must give any other recipients of the Work or
          Derivative Works a copy of this License; and

      (b) You must cause any modified files to carry prominent notices
          stating that You changed the files; and

      (c) You must retain, in the Source form of any Derivative Works
          that You distribute, all copyright, patent, trademark, and
          attribution notices from the Source form of the Work,
          excluding those notices that do not pertain to any part of
          the Derivative Works; and

      (d) If the Work includes a "NOTICE" text file as part of its
          distribution, then any Derivative Works that You distribute must
          include a readable copy of the attribution notices contained
          within such NOTICE file, excluding those notices that do not
          pertain to any part of the Derivative Works, in at least one
          of the following places: within a NOTICE text file distributed
          as part of the Derivative Works; within the Source form or
          documentation, if provided along with the Derivative Works; or,
          within a display generated by the Derivative Works, if and
          wherever such third-party notices normally appear. The contents
          of the NOTICE file are for informational purposes only and
          do not modify the License. You may add Your own attribution
          notices within Derivative Works that You distribute, alongside
          or as an addendum to the NOTICE text from the Work, provided
          that such additional attribution notices cannot be construed
          as modifying the License.

      You may add Your own copyright statement to Your modifications and
      may provide additional or different license terms and conditions
      for use, reproduction, or distribution of Your modifications, or
      for any such Derivative Works as a whole, provided Your use,
      reproduction, and distribution of the Work otherwise complies with
      the conditions stated in this License.

   5. Submission of Contributions. Unless You explicitly state otherwise,
      any Contribution intentionally submitted for inclusion in the Work
      by You to the Licensor shall be under the terms and conditions of
      this License, without any additional terms or conditions.
      Notwithstanding the above, nothing herein shall supersede or modify
      the terms of any separate license agreement you may have executed
      with Licensor regarding such Contributions.

   6. Trademarks. This License does not grant permission to use the trade
      names, trademarks, service marks, or product names of the Licensor,
      except as required for reasonable and customary use in describing the
      origin of the Work and reproducing the content of the NOTICE file.

   7. Disclaimer of Warranty. Unless required by applicable law or
      agreed to in writing, Licensor provides the Work (and each
      Contributor provides its Contributions) on an "AS IS" BASIS,
      WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or
      implied, including, without limitation, any warranties or conditions
      of TITLE, NON-INFRINGEMENT, MERCHANTABILITY, or FITNESS FOR A
      PARTICULAR PURPOSE. You are solely responsible for determining the
      appropriateness of using or redistributing the Work and assume any
      risks associated with Your exercise of permissions under this License.

   8. Limitation of Liability. In no event and under no legal theory,
      whether in tort (including negligence), contract, or otherwise,
      unless required by applicable law (such as deliberate and grossly
      negligent acts) or agreed to in writing, shall any Contributor be
      liable to You for damages, including any direct, indirect, special,
      incidental, or consequential damages of any character arising as a
      result of this License or out of the use or inability to use the
      Work (including but not limited to damages for loss of goodwill,
      work stoppage, computer failure or malfunction, or any and all
      other commercial damages or losses), even if such Contributor
      has been advised of the possibility of such damages.

   9. Accepting Warranty or Additional Liability. While redistributing
      the Work or Derivative Works thereof, You may choose to offer,
      and charge a fee for, acceptance of support, warranty, indemnity,
      or other liability obligations and/or rights consistent with this
      License. However, in accepting such obligations, You may act only
      on Your own behalf and on Your sole responsibility, not on behalf
      of any other Contributor, and only if You agree to indemnify,
      defend, and hold each Contributor harmless for any liability
      incurred by, or claims asserted against, such Contributor by reason
      of your accepting any such warranty or additional liability.

   END OF TERMS AND CONDITIONS

   APPENDIX: How to apply the Apache License to your work.

      To apply the Apache License to your work, attach the following
      boilerplate notice, with the fields enclosed by brackets "[]"
      replaced with your own identifying information. (Don't include
      the brackets!)  The text should be enclosed in the appropriate
      comment syntax for the file format. We also recommend that a
      file or class name and description of purpose be included on the
      same "printed page" as the copyright notice for easier
      identification within third-party archives.

   Copyright [yyyy] [name of copyright owner]

   Licensed under the Apache License, Version 2.0 (the "License");
   you may not use this file except in compliance with the License.
   You may obtain a copy of the License at

       http://www.apache.org/licenses/LICENSE-2.0

   Unless required by applicable law or agreed to in writing, software
   distributed under the License is distributed on an "AS IS" BASIS,
   WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
   See the License for the specific language governing permissions and
   limitations under the License.

--- LLVM Exceptions to the Apache 2.0 License ----

As an exception, if, as a result of your compiling your source code, portions
of this Software are embedded into an Object form of such source code, you
may redistribute such embedded portions in such Object form without complying
with the conditions of Sections 4(a), 4(b) and 4(d) of the License.

In addition, if you combine or link compiled forms of this Software with
software that is licensed under the GPLv2 ("Combined Software") and if a
court of competent jurisdiction determines that the patent provision (Section
3), the indemnity provision (Section 9) or other Section of the License
conflicts with the conditions of the GPLv2, you may retroactively and
prospectively choose to deem waived or otherwise exclude such Section(s) of
the License, but only in their entirety and only with respect to the Combined
Software.
````

- `linux-raw-sys` (Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT): https://github.com/sunfishcode/linux-raw-sys
- `rustix` (Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT): https://github.com/bytecodealliance/rustix

### Apache License 2.0 (`Apache-2.0`)

````text
                                 Apache License
                           Version 2.0, January 2004
                        http://www.apache.org/licenses/

   TERMS AND CONDITIONS FOR USE, REPRODUCTION, AND DISTRIBUTION

   1. Definitions.

      "License" shall mean the terms and conditions for use, reproduction,
      and distribution as defined by Sections 1 through 9 of this document.

      "Licensor" shall mean the copyright owner or entity authorized by
      the copyright owner that is granting the License.

      "Legal Entity" shall mean the union of the acting entity and all
      other entities that control, are controlled by, or are under common
      control with that entity. For the purposes of this definition,
      "control" means (i) the power, direct or indirect, to cause the
      direction or management of such entity, whether by contract or
      otherwise, or (ii) ownership of fifty percent (50%) or more of the
      outstanding shares, or (iii) beneficial ownership of such entity.

      "You" (or "Your") shall mean an individual or Legal Entity
      exercising permissions granted by this License.

      "Source" form shall mean the preferred form for making modifications,
      including but not limited to software source code, documentation
      source, and configuration files.

      "Object" form shall mean any form resulting from mechanical
      transformation or translation of a Source form, including but
      not limited to compiled object code, generated documentation,
      and conversions to other media types.

      "Work" shall mean the work of authorship, whether in Source or
      Object form, made available under the License, as indicated by a
      copyright notice that is included in or attached to the work
      (an example is provided in the Appendix below).

      "Derivative Works" shall mean any work, whether in Source or Object
      form, that is based on (or derived from) the Work and for which the
      editorial revisions, annotations, elaborations, or other modifications
      represent, as a whole, an original work of authorship. For the purposes
      of this License, Derivative Works shall not include works that remain
      separable from, or merely link (or bind by name) to the interfaces of,
      the Work and Derivative Works thereof.

      "Contribution" shall mean any work of authorship, including
      the original version of the Work and any modifications or additions
      to that Work or Derivative Works thereof, that is intentionally
      submitted to Licensor for inclusion in the Work by the copyright owner
      or by an individual or Legal Entity authorized to submit on behalf of
      the copyright owner. For the purposes of this definition, "submitted"
      means any form of electronic, verbal, or written communication sent
      to the Licensor or its representatives, including but not limited to
      communication on electronic mailing lists, source code control systems,
      and issue tracking systems that are managed by, or on behalf of, the
      Licensor for the purpose of discussing and improving the Work, but
      excluding communication that is conspicuously marked or otherwise
      designated in writing by the copyright owner as "Not a Contribution."

      "Contributor" shall mean Licensor and any individual or Legal Entity
      on behalf of whom a Contribution has been received by Licensor and
      subsequently incorporated within the Work.

   2. Grant of Copyright License. Subject to the terms and conditions of
      this License, each Contributor hereby grants to You a perpetual,
      worldwide, non-exclusive, no-charge, royalty-free, irrevocable
      copyright license to reproduce, prepare Derivative Works of,
      publicly display, publicly perform, sublicense, and distribute the
      Work and such Derivative Works in Source or Object form.

   3. Grant of Patent License. Subject to the terms and conditions of
      this License, each Contributor hereby grants to You a perpetual,
      worldwide, non-exclusive, no-charge, royalty-free, irrevocable
      (except as stated in this section) patent license to make, have made,
      use, offer to sell, sell, import, and otherwise transfer the Work,
      where such license applies only to those patent claims licensable
      by such Contributor that are necessarily infringed by their
      Contribution(s) alone or by combination of their Contribution(s)
      with the Work to which such Contribution(s) was submitted. If You
      institute patent litigation against any entity (including a
      cross-claim or counterclaim in a lawsuit) alleging that the Work
      or a Contribution incorporated within the Work constitutes direct
      or contributory patent infringement, then any patent licenses
      granted to You under this License for that Work shall terminate
      as of the date such litigation is filed.

   4. Redistribution. You may reproduce and distribute copies of the
      Work or Derivative Works thereof in any medium, with or without
      modifications, and in Source or Object form, provided that You
      meet the following conditions:

      (a) You must give any other recipients of the Work or
          Derivative Works a copy of this License; and

      (b) You must cause any modified files to carry prominent notices
          stating that You changed the files; and

      (c) You must retain, in the Source form of any Derivative Works
          that You distribute, all copyright, patent, trademark, and
          attribution notices from the Source form of the Work,
          excluding those notices that do not pertain to any part of
          the Derivative Works; and

      (d) If the Work includes a "NOTICE" text file as part of its
          distribution, then any Derivative Works that You distribute must
          include a readable copy of the attribution notices contained
          within such NOTICE file, excluding those notices that do not
          pertain to any part of the Derivative Works, in at least one
          of the following places: within a NOTICE text file distributed
          as part of the Derivative Works; within the Source form or
          documentation, if provided along with the Derivative Works; or,
          within a display generated by the Derivative Works, if and
          wherever such third-party notices normally appear. The contents
          of the NOTICE file are for informational purposes only and
          do not modify the License. You may add Your own attribution
          notices within Derivative Works that You distribute, alongside
          or as an addendum to the NOTICE text from the Work, provided
          that such additional attribution notices cannot be construed
          as modifying the License.

      You may add Your own copyright statement to Your modifications and
      may provide additional or different license terms and conditions
      for use, reproduction, or distribution of Your modifications, or
      for any such Derivative Works as a whole, provided Your use,
      reproduction, and distribution of the Work otherwise complies with
      the conditions stated in this License.

   5. Submission of Contributions. Unless You explicitly state otherwise,
      any Contribution intentionally submitted for inclusion in the Work
      by You to the Licensor shall be under the terms and conditions of
      this License, without any additional terms or conditions.
      Notwithstanding the above, nothing herein shall supersede or modify
      the terms of any separate license agreement you may have executed
      with Licensor regarding such Contributions.

   6. Trademarks. This License does not grant permission to use the trade
      names, trademarks, service marks, or product names of the Licensor,
      except as required for reasonable and customary use in describing the
      origin of the Work and reproducing the content of the NOTICE file.

   7. Disclaimer of Warranty. Unless required by applicable law or
      agreed to in writing, Licensor provides the Work (and each
      Contributor provides its Contributions) on an "AS IS" BASIS,
      WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or
      implied, including, without limitation, any warranties or conditions
      of TITLE, NON-INFRINGEMENT, MERCHANTABILITY, or FITNESS FOR A
      PARTICULAR PURPOSE. You are solely responsible for determining the
      appropriateness of using or redistributing the Work and assume any
      risks associated with Your exercise of permissions under this License.

   8. Limitation of Liability. In no event and under no legal theory,
      whether in tort (including negligence), contract, or otherwise,
      unless required by applicable law (such as deliberate and grossly
      negligent acts) or agreed to in writing, shall any Contributor be
      liable to You for damages, including any direct, indirect, special,
      incidental, or consequential damages of any character arising as a
      result of this License or out of the use or inability to use the
      Work (including but not limited to damages for loss of goodwill,
      work stoppage, computer failure or malfunction, or any and all
      other commercial damages or losses), even if such Contributor
      has been advised of the possibility of such damages.

   9. Accepting Warranty or Additional Liability. While redistributing
      the Work or Derivative Works thereof, You may choose to offer,
      and charge a fee for, acceptance of support, warranty, indemnity,
      or other liability obligations and/or rights consistent with this
      License. However, in accepting such obligations, You may act only
      on Your own behalf and on Your sole responsibility, not on behalf
      of any other Contributor, and only if You agree to indemnify,
      defend, and hold each Contributor harmless for any liability
      incurred by, or claims asserted against, such Contributor by reason
      of your accepting any such warranty or additional liability.

   END OF TERMS AND CONDITIONS

   APPENDIX: How to apply the Apache License to your work.

      To apply the Apache License to your work, attach the following
      boilerplate notice, with the fields enclosed by brackets "[]"
      replaced with your own identifying information. (Don't include
      the brackets!)  The text should be enclosed in the appropriate
      comment syntax for the file format. We also recommend that a
      file or class name and description of purpose be included on the
      same "printed page" as the copyright notice for easier
      identification within third-party archives.

   Copyright [yyyy] [name of copyright owner]

   Licensed under the Apache License, Version 2.0 (the "License");
   you may not use this file except in compliance with the License.
   You may obtain a copy of the License at

       http://www.apache.org/licenses/LICENSE-2.0

   Unless required by applicable law or agreed to in writing, software
   distributed under the License is distributed on an "AS IS" BASIS,
   WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
   See the License for the specific language governing permissions and
   limitations under the License.


Licenses for support code
-------------------------

Parts of the TLS test suite are under the Go license. This code is not included
in BoringSSL (i.e. libcrypto and libssl) when compiled, however, so
distributing code linked against BoringSSL does not trigger this license:

Copyright (c) 2009 The Go Authors. All rights reserved.

Redistribution and use in source and binary forms, with or without
modification, are permitted provided that the following conditions are
met:

   * Redistributions of source code must retain the above copyright
notice, this list of conditions and the following disclaimer.
   * Redistributions in binary form must reproduce the above
copyright notice, this list of conditions and the following disclaimer
in the documentation and/or other materials provided with the
distribution.
   * Neither the name of Google Inc. nor the names of its
contributors may be used to endorse or promote products derived from
this software without specific prior written permission.

THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS
"AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT
LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR
A PARTICULAR PURPOSE ARE DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT
OWNER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT
LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR SERVICES; LOSS OF USE,
DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY
THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT
(INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE
OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.


BoringSSL uses the Chromium test infrastructure to run a continuous build,
trybots etc. The scripts which manage this, and the script for generating build
metadata, are under the Chromium license. Distributing code linked against
BoringSSL does not trigger this license.

Copyright 2015 The Chromium Authors. All rights reserved.

Redistribution and use in source and binary forms, with or without
modification, are permitted provided that the following conditions are
met:

   * Redistributions of source code must retain the above copyright
notice, this list of conditions and the following disclaimer.
   * Redistributions in binary form must reproduce the above
copyright notice, this list of conditions and the following disclaimer
in the documentation and/or other materials provided with the
distribution.
   * Neither the name of Google Inc. nor the names of its
contributors may be used to endorse or promote products derived from
this software without specific prior written permission.

THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS
"AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT
LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR
A PARTICULAR PURPOSE ARE DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT
OWNER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT
LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR SERVICES; LOSS OF USE,
DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY
THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT
(INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE
OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.
````

- `ring` (Apache-2.0 AND ISC): https://github.com/briansmith/ring

### Apache License 2.0 (`Apache-2.0`)

````text
unrar-rs license

unrar-rs is licensed under the Apache License, Version 2.0, with one further
condition on its RAR engine.

The RAR engine was developed using the source code of the unRAR program. All
copyrights to the original unRAR code are owned by Alexander Roshal, and its
license restriction continues to govern that code:

UnRAR source code may be used in any software to handle
RAR archives without limitations free of charge, but cannot be
used to develop RAR (WinRAR) compatible archiver and to
re-create RAR compression algorithm, which is proprietary.
Distribution of modified UnRAR source code in separate form
or as a part of other software is permitted, provided that
full text of this paragraph, starting from "UnRAR source code"
words, is included in license, or in documentation if license
is not available, and in source code comments of resulting package.

A binary distribution must include this file.

The full Apache License, Version 2.0 text follows.

                                 Apache License
                           Version 2.0, January 2004
                        http://www.apache.org/licenses/

   TERMS AND CONDITIONS FOR USE, REPRODUCTION, AND DISTRIBUTION

   1. Definitions.

      "License" shall mean the terms and conditions for use, reproduction,
      and distribution as defined by Sections 1 through 9 of this document.

      "Licensor" shall mean the copyright owner or entity authorized by
      the copyright owner that is granting the License.

      "Legal Entity" shall mean the union of the acting entity and all
      other entities that control, are controlled by, or are under common
      control with that entity. For the purposes of this definition,
      "control" means (i) the power, direct or indirect, to cause the
      direction or management of such entity, whether by contract or
      otherwise, or (ii) ownership of fifty percent (50%) or more of the
      outstanding shares, or (iii) beneficial ownership of such entity.

      "You" (or "Your") shall mean an individual or Legal Entity
      exercising permissions granted by this License.

      "Source" form shall mean the preferred form for making modifications,
      including but not limited to software source code, documentation
      source, and configuration files.

      "Object" form shall mean any form resulting from mechanical
      transformation or translation of a Source form, including but
      not limited to compiled object code, generated documentation,
      and conversions to other media types.

      "Work" shall mean the work of authorship, whether in Source or
      Object form, made available under the License, as indicated by a
      copyright notice that is included in or attached to the work
      (an example is provided in the Appendix below).

      "Derivative Works" shall mean any work, whether in Source or Object
      form, that is based on (or derived from) the Work and for which the
      editorial revisions, annotations, elaborations, or other modifications
      represent, as a whole, an original work of authorship. For the purposes
      of this License, Derivative Works shall not include works that remain
      separable from, or merely link (or bind by name) to the interfaces of,
      the Work and Derivative Works thereof.

      "Contribution" shall mean any work of authorship, including
      the original version of the Work and any modifications or additions
      to that Work or Derivative Works thereof, that is intentionally
      submitted to Licensor for inclusion in the Work by the copyright owner
      or by an individual or Legal Entity authorized to submit on behalf of
      the copyright owner. For the purposes of this definition, "submitted"
      means any form of electronic, verbal, or written communication sent
      to the Licensor or its representatives, including but not limited to
      communication on electronic mailing lists, source code control systems,
      and issue tracking systems that are managed by, or on behalf of, the
      Licensor for the purpose of discussing and improving the Work, but
      excluding communication that is conspicuously marked or otherwise
      designated in writing by the copyright owner as "Not a Contribution."

      "Contributor" shall mean Licensor and any individual or Legal Entity
      on behalf of whom a Contribution has been received by Licensor and
      subsequently incorporated within the Work.

   2. Grant of Copyright License. Subject to the terms and conditions of
      this License, each Contributor hereby grants to You a perpetual,
      worldwide, non-exclusive, no-charge, royalty-free, irrevocable
      copyright license to reproduce, prepare Derivative Works of,
      publicly display, publicly perform, sublicense, and distribute the
      Work and such Derivative Works in Source or Object form.

   3. Grant of Patent License. Subject to the terms and conditions of
      this License, each Contributor hereby grants to You a perpetual,
      worldwide, non-exclusive, no-charge, royalty-free, irrevocable
      (except as stated in this section) patent license to make, have made,
      use, offer to sell, sell, import, and otherwise transfer the Work,
      where such license applies only to those patent claims licensable
      by such Contributor that are necessarily infringed by their
      Contribution(s) alone or by combination of their Contribution(s)
      with the Work to which such Contribution(s) was submitted. If You
      institute patent litigation against any entity (including a
      cross-claim or counterclaim in a lawsuit) alleging that the Work
      or a Contribution incorporated within the Work constitutes direct
      or contributory patent infringement, then any patent licenses
      granted to You under this License for that Work shall terminate
      as of the date such litigation is filed.

   4. Redistribution. You may reproduce and distribute copies of the
      Work or Derivative Works thereof in any medium, with or without
      modifications, and in Source or Object form, provided that You
      meet the following conditions:

      (a) You must give any other recipients of the Work or
          Derivative Works a copy of this License; and

      (b) You must cause any modified files to carry prominent notices
          stating that You changed the files; and

      (c) You must retain, in the Source form of any Derivative Works
          that You distribute, all copyright, patent, trademark, and
          attribution notices from the Source form of the Work,
          excluding those notices that do not pertain to any part of
          the Derivative Works; and

      (d) If the Work includes a "NOTICE" text file as part of its
          distribution, then any Derivative Works that You distribute must
          include a readable copy of the attribution notices contained
          within such NOTICE file, excluding those notices that do not
          pertain to any part of the Derivative Works, in at least one
          of the following places: within a NOTICE text file distributed
          as part of the Derivative Works; within the Source form or
          documentation, if provided along with the Derivative Works; or,
          within a display generated by the Derivative Works, if and
          wherever such third-party notices normally appear. The contents
          of the NOTICE file are for informational purposes only and
          do not modify the License. You may add Your own attribution
          notices within Derivative Works that You distribute, alongside
          or as an addendum to the NOTICE text from the Work, provided
          that such additional attribution notices cannot be construed
          as modifying the License.

      You may add Your own copyright statement to Your modifications and
      may provide additional or different license terms and conditions
      for use, reproduction, or distribution of Your modifications, or
      for any such Derivative Works as a whole, provided Your use,
      reproduction, and distribution of the Work otherwise complies with
      the conditions stated in this License.

   5. Submission of Contributions. Unless You explicitly state otherwise,
      any Contribution intentionally submitted for inclusion in the Work
      by You to the Licensor shall be under the terms and conditions of
      this License, without any additional terms or conditions.
      Notwithstanding the above, nothing herein shall supersede or modify
      the terms of any separate license agreement you may have executed
      with Licensor regarding such Contributions.

   6. Trademarks. This License does not grant permission to use the trade
      names, trademarks, service marks, or product names of the Licensor,
      except as required for reasonable and customary use in describing the
      origin of the Work and reproducing the content of the NOTICE file.

   7. Disclaimer of Warranty. Unless required by applicable law or
      agreed to in writing, Licensor provides the Work (and each
      Contributor provides its Contributions) on an "AS IS" BASIS,
      WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or
      implied, including, without limitation, any warranties or conditions
      of TITLE, NON-INFRINGEMENT, MERCHANTABILITY, or FITNESS FOR A
      PARTICULAR PURPOSE. You are solely responsible for determining the
      appropriateness of using or redistributing the Work and assume any
      risks associated with Your exercise of permissions under this License.

   8. Limitation of Liability. In no event and under no legal theory,
      whether in tort (including negligence), contract, or otherwise,
      unless required by applicable law (such as deliberate and grossly
      negligent acts) or agreed to in writing, shall any Contributor be
      liable to You for damages, including any direct, indirect, special,
      incidental, or consequential damages of any character arising as a
      result of this License or out of the use or inability to use the
      Work (including but not limited to damages for loss of goodwill,
      work stoppage, computer failure or malfunction, or any and all
      other commercial damages or losses), even if such Contributor
      has been advised of the possibility of such damages.

   9. Accepting Warranty or Additional Liability. While redistributing
      the Work or Derivative Works thereof, You may choose to offer,
      and charge a fee for, acceptance of support, warranty, indemnity,
      or other liability obligations and/or rights consistent with this
      License. However, in accepting such obligations, You may act only
      on Your own behalf and on Your sole responsibility, not on behalf
      of any other Contributor, and only if You agree to indemnify,
      defend, and hold each Contributor harmless for any liability
      incurred by, or claims asserted against, such Contributor by reason
      of your accepting any such warranty or additional liability.

   END OF TERMS AND CONDITIONS

   APPENDIX: How to apply the Apache License to your work.

      To apply the Apache License to your work, attach the following
      boilerplate notice, with the fields enclosed by brackets "[]"
      replaced with your own identifying information. (Don't include
      the brackets!)  The text should be enclosed in the appropriate
      comment syntax for the file format. We also recommend that a
      file or class name and description of purpose be included on the
      same "printed page" as the copyright notice for easier
      identification within third-party archives.

   Copyright [yyyy] [name of copyright owner]

   Licensed under the Apache License, Version 2.0 (the "License");
   you may not use this file except in compliance with the License.
   You may obtain a copy of the License at

       http://www.apache.org/licenses/LICENSE-2.0

   Unless required by applicable law or agreed to in writing, software
   distributed under the License is distributed on an "AS IS" BASIS,
   WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
   See the License for the specific language governing permissions and
   limitations under the License.
````

- `unrar-rs`: https://github.com/scryer-media/rarpar

### BSD 3-Clause "New" or "Revised" License (`BSD-3-Clause`)

````text
//
// Redistribution and use in source and binary forms, with or without
// modification, are permitted provided that the following conditions are met:
//
// 1. Redistributions of source code must retain the above copyright notice, this
//    list of conditions and the following disclaimer.
//
// 2. Redistributions in binary form must reproduce the above copyright notice,
//    this list of conditions and the following disclaimer in the documentation
//    and/or other materials provided with the distribution.
//
// 3. Neither the name of the copyright holder nor the names of its
//    contributors may be used to endorse or promote products derived from
//    this software without specific prior written permission.
//
// THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS"
// AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE
// IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
// DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE
// FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL
// DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
// SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER
// CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY,
// OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE
// OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

/// The PUA code points special-cased in the GB18030 encoder.
pub(crate) static GB18030_2022_OVERRIDE_PUA: [u16; 18] = [
    0xE78D, 0xE78E, 0xE78F, 0xE790, 0xE791, 0xE792, 0xE793, 0xE794, 0xE795, 0xE796, 0xE81E, 0xE826,
    0xE82B, 0xE82C, 0xE832, 0xE843, 0xE854, 0xE864,
];

/// The bytes corresponding to the PUA code points special-cased in the GB18030 encoder.
pub(crate) static GB18030_2022_OVERRIDE_BYTES: [[u8; 2]; 18] = [
    [0xA6, 0xD9],
    [0xA6, 0xDA],
    [0xA6, 0xDB],
    [0xA6, 0xDC],
    [0xA6, 0xDD],
    [0xA6, 0xDE],
    [0xA6, 0xDF],
    [0xA6, 0xEC],
    [0xA6, 0xED],
    [0xA6, 0xF3],
    [0xFE, 0x59],
    [0xFE, 0x61],
    [0xFE, 0x66],
    [0xFE, 0x67],
    [0xFE, 0x6D],
    [0xFE, 0x7E],
    [0xFE, 0x90],
    [0xFE, 0xA0],
];
````

- `encoding_rs` ((Apache-2.0 OR MIT) AND BSD-3-Clause): https://github.com/hsivonen/encoding_rs
  - // Copyright © WHATWG (Apple, Google, Mozilla, Microsoft).

### BSD 3-Clause "New" or "Revised" License (`BSD-3-Clause`)

````text
Redistribution and use in source and binary forms, with or without
modification, are permitted provided that the following conditions are
met:

1. Redistributions of source code must retain the above copyright
notice, this list of conditions and the following disclaimer.

2. Redistributions in binary form must reproduce the above copyright
notice, this list of conditions and the following disclaimer in the
documentation and/or other materials provided with the distribution.

3. Neither the name of the copyright holder nor the names of its
contributors may be used to endorse or promote products derived from
this software without specific prior written permission.

THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS
IS" AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED
TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A
PARTICULAR PURPOSE ARE DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT
HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED
TO, PROCUREMENT OF SUBSTITUTE GOODS OR SERVICES; LOSS OF USE, DATA, OR
PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF
LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING
NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE OF THIS
SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.
````

- `encoding_rs` ((Apache-2.0 OR MIT) AND BSD-3-Clause): https://github.com/hsivonen/encoding_rs
  - Copyright © WHATWG (Apple, Google, Mozilla, Microsoft).
- `subtle` (BSD-3-Clause): https://github.com/dalek-cryptography/subtle
  - Copyright (c) 2016-2017 Isis Agora Lovecruft, Henry de Valence. All rights reserved.
  - Copyright (c) 2016-2024 Isis Agora Lovecruft. All rights reserved.

### Community Data License Agreement Permissive 2.0 (`CDLA-Permissive-2.0`)

````text
# Community Data License Agreement - Permissive - Version 2.0

This is the Community Data License Agreement - Permissive, Version
2.0 (the "agreement"). Data Provider(s) and Data Recipient(s) agree
as follows:

## 1. Provision of the Data

1.1. A Data Recipient may use, modify, and share the Data made
available by Data Provider(s) under this agreement if that Data
Recipient follows the terms of this agreement.

1.2. This agreement does not impose any restriction on a Data
Recipient's use, modification, or sharing of any portions of the
Data that are in the public domain or that may be used, modified,
or shared under any other legal exception or limitation.

## 2. Conditions for Sharing Data

2.1. A Data Recipient may share Data, with or without modifications, so
long as the Data Recipient makes available the text of this agreement
with the shared Data.

## 3. No Restrictions on Results

3.1. This agreement does not impose any restriction or obligations
with respect to the use, modification, or sharing of Results.

## 4. No Warranty; Limitation of Liability

4.1. All Data Recipients receive the Data subject to the following
terms:

THE DATA IS PROVIDED ON AN "AS IS" BASIS, WITHOUT REPRESENTATIONS,
WARRANTIES OR CONDITIONS OF ANY KIND, EITHER EXPRESS OR IMPLIED
INCLUDING, WITHOUT LIMITATION, ANY WARRANTIES OR CONDITIONS OF TITLE,
NON-INFRINGEMENT, MERCHANTABILITY OR FITNESS FOR A PARTICULAR PURPOSE.

NO DATA PROVIDER SHALL HAVE ANY LIABILITY FOR ANY DIRECT, INDIRECT,
INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING
WITHOUT LIMITATION LOST PROFITS), HOWEVER CAUSED AND ON ANY THEORY OF
LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING
NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE DATA OR RESULTS,
EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGES.

## 5. Definitions

5.1. "Data" means the material received by a Data Recipient under
this agreement.

5.2. "Data Provider" means any person who is the source of Data
provided under this agreement and in reliance on a Data Recipient's
agreement to its terms.

5.3. "Data Recipient" means any person who receives Data directly
or indirectly from a Data Provider and agrees to the terms of this
agreement.

5.4. "Results" means any outcome obtained by computational analysis
of Data, including for example machine learning models and models'
insights.
````

- `webpki-root-certs` (CDLA-Permissive-2.0): https://github.com/rustls/webpki-roots
- `webpki-roots` (CDLA-Permissive-2.0): https://github.com/rustls/webpki-roots

### Creative Commons Zero v1.0 Universal (`CC0-1.0`)

````text
CC0 1.0 Universal

    CREATIVE COMMONS CORPORATION IS NOT A LAW FIRM AND DOES NOT PROVIDE
    LEGAL SERVICES. DISTRIBUTION OF THIS DOCUMENT DOES NOT CREATE AN
    ATTORNEY-CLIENT RELATIONSHIP. CREATIVE COMMONS PROVIDES THIS
    INFORMATION ON AN "AS-IS" BASIS. CREATIVE COMMONS MAKES NO WARRANTIES
    REGARDING THE USE OF THIS DOCUMENT OR THE INFORMATION OR WORKS
    PROVIDED HEREUNDER, AND DISCLAIMS LIABILITY FOR DAMAGES RESULTING FROM
    THE USE OF THIS DOCUMENT OR THE INFORMATION OR WORKS PROVIDED
    HEREUNDER.

Statement of Purpose

The laws of most jurisdictions throughout the world automatically confer
exclusive Copyright and Related Rights (defined below) upon the creator
and subsequent owner(s) (each and all, an "owner") of an original work of
authorship and/or a database (each, a "Work").

Certain owners wish to permanently relinquish those rights to a Work for
the purpose of contributing to a commons of creative, cultural and
scientific works ("Commons") that the public can reliably and without fear
of later claims of infringement build upon, modify, incorporate in other
works, reuse and redistribute as freely as possible in any form whatsoever
and for any purposes, including without limitation commercial purposes.
These owners may contribute to the Commons to promote the ideal of a free
culture and the further production of creative, cultural and scientific
works, or to gain reputation or greater distribution for their Work in
part through the use and efforts of others.

For these and/or other purposes and motivations, and without any
expectation of additional consideration or compensation, the person
associating CC0 with a Work (the "Affirmer"), to the extent that he or she
is an owner of Copyright and Related Rights in the Work, voluntarily
elects to apply CC0 to the Work and publicly distribute the Work under its
terms, with knowledge of his or her Copyright and Related Rights in the
Work and the meaning and intended legal effect of CC0 on those rights.

1. Copyright and Related Rights. A Work made available under CC0 may be
   protected by copyright and related or neighboring rights ("Copyright and
   Related Rights"). Copyright and Related Rights include, but are not
   limited to, the following:

i. the right to reproduce, adapt, distribute, perform, display,
communicate, and translate a Work;
ii. moral rights retained by the original author(s) and/or performer(s);
iii. publicity and privacy rights pertaining to a person's image or
likeness depicted in a Work;
iv. rights protecting against unfair competition in regards to a Work,
subject to the limitations in paragraph 4(a), below;
v. rights protecting the extraction, dissemination, use and reuse of data
in a Work;
vi. database rights (such as those arising under Directive 96/9/EC of the
European Parliament and of the Council of 11 March 1996 on the legal
protection of databases, and under any national implementation
thereof, including any amended or successor version of such
directive); and
vii. other similar, equivalent or corresponding rights throughout the
world based on applicable law or treaty, and any national
implementations thereof.

2. Waiver. To the greatest extent permitted by, but not in contravention
   of, applicable law, Affirmer hereby overtly, fully, permanently,
   irrevocably and unconditionally waives, abandons, and surrenders all of
   Affirmer's Copyright and Related Rights and associated claims and causes
   of action, whether now known or unknown (including existing as well as
   future claims and causes of action), in the Work (i) in all territories
   worldwide, (ii) for the maximum duration provided by applicable law or
   treaty (including future time extensions), (iii) in any current or future
   medium and for any number of copies, and (iv) for any purpose whatsoever,
   including without limitation commercial, advertising or promotional
   purposes (the "Waiver"). Affirmer makes the Waiver for the benefit of each
   member of the public at large and to the detriment of Affirmer's heirs and
   successors, fully intending that such Waiver shall not be subject to
   revocation, rescission, cancellation, termination, or any other legal or
   equitable action to disrupt the quiet enjoyment of the Work by the public
   as contemplated by Affirmer's express Statement of Purpose.

3. Public License Fallback. Should any part of the Waiver for any reason
   be judged legally invalid or ineffective under applicable law, then the
   Waiver shall be preserved to the maximum extent permitted taking into
   account Affirmer's express Statement of Purpose. In addition, to the
   extent the Waiver is so judged Affirmer hereby grants to each affected
   person a royalty-free, non transferable, non sublicensable, non exclusive,
   irrevocable and unconditional license to exercise Affirmer's Copyright and
   Related Rights in the Work (i) in all territories worldwide, (ii) for the
   maximum duration provided by applicable law or treaty (including future
   time extensions), (iii) in any current or future medium and for any number
   of copies, and (iv) for any purpose whatsoever, including without
   limitation commercial, advertising or promotional purposes (the
   "License"). The License shall be deemed effective as of the date CC0 was
   applied by Affirmer to the Work. Should any part of the License for any
   reason be judged legally invalid or ineffective under applicable law, such
   partial invalidity or ineffectiveness shall not invalidate the remainder
   of the License, and in such case Affirmer hereby affirms that he or she
   will not (i) exercise any of his or her remaining Copyright and Related
   Rights in the Work or (ii) assert any associated claims and causes of
   action with respect to the Work, in either case contrary to Affirmer's
   express Statement of Purpose.

4. Limitations and Disclaimers.

a. No trademark or patent rights held by Affirmer are waived, abandoned,
surrendered, licensed or otherwise affected by this document.
b. Affirmer offers the Work as-is and makes no representations or
warranties of any kind concerning the Work, express, implied,
statutory or otherwise, including without limitation warranties of
title, merchantability, fitness for a particular purpose, non
infringement, or the absence of latent or other defects, accuracy, or
the present or absence of errors, whether or not discoverable, all to
the greatest extent permissible under applicable law.
c. Affirmer disclaims responsibility for clearing rights of other persons
that may apply to the Work or any use thereof, including without
limitation any person's Copyright and Related Rights in the Work.
Further, Affirmer disclaims responsibility for obtaining any necessary
consents, permissions or other rights required for any use of the
Work.
d. Affirmer understands and acknowledges that Creative Commons is not a
party to this document and has no duty or obligation with respect to
this CC0 or use of the Work.
````

- `ppmd-rust` (CC0-1.0 OR MIT-0): https://github.com/hasenbanck/ppmd-rust

### ISC License (`ISC`)

````text
Copyright 2015-2025 Brian Smith.

Permission to use, copy, modify, and/or distribute this software for any
purpose with or without fee is hereby granted, provided that the above
copyright notice and this permission notice appear in all copies.

THE SOFTWARE IS PROVIDED "AS IS" AND THE AUTHOR DISCLAIMS ALL WARRANTIES
WITH REGARD TO THIS SOFTWARE INCLUDING ALL IMPLIED WARRANTIES OF
MERCHANTABILITY AND FITNESS. IN NO EVENT SHALL THE AUTHOR BE LIABLE FOR ANY
SPECIAL, DIRECT, INDIRECT, OR CONSEQUENTIAL DAMAGES OR ANY DAMAGES
WHATSOEVER RESULTING FROM LOSS OF USE, DATA OR PROFITS, WHETHER IN AN ACTION
OF CONTRACT, NEGLIGENCE OR OTHER TORTIOUS ACTION, ARISING OUT OF OR IN
CONNECTION WITH THE USE OR PERFORMANCE OF THIS SOFTWARE.
````

- `ring` (Apache-2.0 AND ISC): https://github.com/briansmith/ring

### ISC License (`ISC`)

````text
Except as otherwise noted, this project is licensed under the following
(ISC-style) terms:

Permission to use, copy, modify, and/or distribute this software for any
purpose with or without fee is hereby granted, provided that the above
copyright notice and this permission notice appear in all copies.

THE SOFTWARE IS PROVIDED "AS IS" AND THE AUTHORS DISCLAIM ALL WARRANTIES
WITH REGARD TO THIS SOFTWARE INCLUDING ALL IMPLIED WARRANTIES OF
MERCHANTABILITY AND FITNESS. IN NO EVENT SHALL THE AUTHORS BE LIABLE FOR
ANY SPECIAL, DIRECT, INDIRECT, OR CONSEQUENTIAL DAMAGES OR ANY DAMAGES
WHATSOEVER RESULTING FROM LOSS OF USE, DATA OR PROFITS, WHETHER IN AN
ACTION OF CONTRACT, NEGLIGENCE OR OTHER TORTIOUS ACTION, ARISING OUT OF
OR IN CONNECTION WITH THE USE OR PERFORMANCE OF THIS SOFTWARE.

The files under third-party/chromium are licensed as described in
third-party/chromium/LICENSE.
````

- `rustls-webpki` (ISC): https://github.com/rustls/webpki
  - Copyright 2015 Brian Smith.

### ISC License (`ISC`)

````text
//
// Permission to use, copy, modify, and/or distribute this software for any
// purpose with or without fee is hereby granted, provided that the above
// copyright notice and this permission notice appear in all copies.
//
// THE SOFTWARE IS PROVIDED "AS IS" AND THE AUTHORS DISCLAIM ALL WARRANTIES
// WITH REGARD TO THIS SOFTWARE INCLUDING ALL IMPLIED WARRANTIES OF
// MERCHANTABILITY AND FITNESS. IN NO EVENT SHALL THE AUTHORS BE LIABLE FOR
// ANY SPECIAL, DIRECT, INDIRECT, OR CONSEQUENTIAL DAMAGES OR ANY DAMAGES
// WHATSOEVER RESULTING FROM LOSS OF USE, DATA OR PROFITS, WHETHER IN AN
// ACTION OF CONTRACT, NEGLIGENCE OR OTHER TORTIOUS ACTION, ARISING OUT OF
// OR IN CONNECTION WITH THE USE OR PERFORMANCE OF THIS SOFTWARE.
````

- `untrusted` (ISC): https://github.com/briansmith/untrusted
  - // Copyright 2015-2016 Brian Smith.

### MIT License (`MIT`)

````text
MIT License

Copyright (c) <year> <copyright holders>

Permission is hereby granted, free of charge, to any person obtaining a copy of this software and
associated documentation files (the "Software"), to deal in the Software without restriction, including
without limitation the rights to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is furnished to do so, subject to the
following conditions:

The above copyright notice and this permission notice shall be included in all copies or substantial
portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT
LIMITED TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO
EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER
IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE
USE OR OTHER DEALINGS IN THE SOFTWARE.
````

- `blake2s_simd` (MIT): https://github.com/oconnor663/blake2_simd; ships no file with this licence's text; authors: Jack O'Connor
- `bytes` (MIT): https://github.com/tokio-rs/bytes
  - Copyright (c) 2018 Carl Lerche
- `cfg_aliases` (MIT): https://github.com/katharostech/cfg_aliases
  - Copyright (c) 2020 Katharos Technology
- `generic-array` (MIT): https://github.com/fizyk20/generic-array.git
  - Copyright (c) 2015 Bartłomiej Kamiński
- `memchr` (Unlicense OR MIT): https://github.com/BurntSushi/memchr
  - Copyright (c) 2015 Andrew Gallant
- `nix` (MIT): https://github.com/nix-rust/nix
  - Copyright (c) 2015 Carl Lerche + nix-rust Authors
- `rmp` (MIT): https://github.com/3Hren/msgpack-rust
  - Copyright (c) 2017 Evgeny Safronov
- `rmp-serde` (MIT): https://github.com/3Hren/msgpack-rust
  - Copyright (c) 2017 Evgeny Safronov
- `schannel` (MIT): https://github.com/steffengy/schannel-rs
  - Copyright (c) 2015 steffengy
- `simd-adler32` (MIT): https://github.com/mcountryman/simd-adler32
  - Copyright (c) [2021] [Marvin Countryman]
- `slab` (MIT): https://github.com/tokio-rs/slab
  - Copyright (c) 2019 Carl Lerche
- `spin` (MIT): https://github.com/mvdnes/spin-rs.git
  - Copyright (c) 2014 Mathijs van de Nes
- `strsim` (MIT): https://github.com/rapidfuzz/strsim-rs
  - Copyright (c) 2015 Danny Guo
  - Copyright (c) 2016 Titus Wormer <tituswormer@gmail.com>
  - Copyright (c) 2018 Akash Kurdekar
- `synstructure` (MIT): https://github.com/mystor/synstructure
  - Copyright 2016 Nika Layzell
- `tracing` (MIT): https://github.com/tokio-rs/tracing
  - Copyright (c) 2019 Tokio Contributors
- `tracing-core` (MIT): https://github.com/tokio-rs/tracing
  - Copyright (c) 2019 Tokio Contributors
  - Copyright (c) 2014 Mathijs van de Nes
- `zip` (MIT): https://github.com/zip-rs/zip2
  - Copyright (c) 2014 Mathijs van de Nes
- `zmij` (MIT): https://github.com/dtolnay/zmij

### MIT License (`MIT`)

````text
# 3rd Party Notices

The `cfg_aliases!` macro uses a lot of the code from [`tectonic_cfg_support::target_cfg!`] macro which is under the following license:

[`tectonic_cfg_support::target_cfg!`]: https://github.com/tectonic-typesetting/tectonic/blob/f2439b936470ad27bdf92882064bc4702ee01899/cfg_support/src/lib.rs#L166

    tectonic_cfg_support is licensed under the MIT License.

    Permission is hereby granted, free of charge, to any person obtaining a copy
    of this software and associated documentation files (the “Software”), to deal
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
---
````

- `cfg_aliases` (MIT): https://github.com/katharostech/cfg_aliases

### Unicode License v3 (`Unicode-3.0`)

````text
UNICODE LICENSE V3

COPYRIGHT AND PERMISSION NOTICE

Copyright © 2020-2024 Unicode, Inc.

NOTICE TO USER: Carefully read the following legal agreement. BY
DOWNLOADING, INSTALLING, COPYING OR OTHERWISE USING DATA FILES, AND/OR
SOFTWARE, YOU UNEQUIVOCALLY ACCEPT, AND AGREE TO BE BOUND BY, ALL OF THE
TERMS AND CONDITIONS OF THIS AGREEMENT. IF YOU DO NOT AGREE, DO NOT
DOWNLOAD, INSTALL, COPY, DISTRIBUTE OR USE THE DATA FILES OR SOFTWARE.

Permission is hereby granted, free of charge, to any person obtaining a
copy of data files and any associated documentation (the "Data Files") or
software and any associated documentation (the "Software") to deal in the
Data Files or Software without restriction, including without limitation
the rights to use, copy, modify, merge, publish, distribute, and/or sell
copies of the Data Files or Software, and to permit persons to whom the
Data Files or Software are furnished to do so, provided that either (a)
this copyright and permission notice appear with all copies of the Data
Files or Software, or (b) this copyright and permission notice appear in
associated Documentation.

THE DATA FILES AND SOFTWARE ARE PROVIDED "AS IS", WITHOUT WARRANTY OF ANY
KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF
MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT OF
THIRD PARTY RIGHTS.

IN NO EVENT SHALL THE COPYRIGHT HOLDER OR HOLDERS INCLUDED IN THIS NOTICE
BE LIABLE FOR ANY CLAIM, OR ANY SPECIAL INDIRECT OR CONSEQUENTIAL DAMAGES,
OR ANY DAMAGES WHATSOEVER RESULTING FROM LOSS OF USE, DATA OR PROFITS,
WHETHER IN AN ACTION OF CONTRACT, NEGLIGENCE OR OTHER TORTIOUS ACTION,
ARISING OUT OF OR IN CONNECTION WITH THE USE OR PERFORMANCE OF THE DATA
FILES OR SOFTWARE.

Except as contained in this notice, the name of a copyright holder shall
not be used in advertising or otherwise to promote the sale, use or other
dealings in these Data Files or Software without prior written
authorization of the copyright holder.

SPDX-License-Identifier: Unicode-3.0

—

Portions of ICU4X may have been adapted from ICU4C and/or ICU4J.
ICU 1.8.1 to ICU 57.1 © 1995-2016 International Business Machines Corporation and others.
````

- `icu_collections` (Unicode-3.0): https://github.com/unicode-org/icu4x
- `icu_locale_core` (Unicode-3.0): https://github.com/unicode-org/icu4x
- `icu_normalizer` (Unicode-3.0): https://github.com/unicode-org/icu4x
- `icu_normalizer_data` (Unicode-3.0): https://github.com/unicode-org/icu4x
- `icu_properties` (Unicode-3.0): https://github.com/unicode-org/icu4x
- `icu_properties_data` (Unicode-3.0): https://github.com/unicode-org/icu4x
- `icu_provider` (Unicode-3.0): https://github.com/unicode-org/icu4x
- `litemap` (Unicode-3.0): https://github.com/unicode-org/icu4x
- `potential_utf` (Unicode-3.0): https://github.com/unicode-org/icu4x
- `tinystr` (Unicode-3.0): https://github.com/unicode-org/icu4x
- `writeable` (Unicode-3.0): https://github.com/unicode-org/icu4x
- `yoke` (Unicode-3.0): https://github.com/unicode-org/icu4x
- `yoke-derive` (Unicode-3.0): https://github.com/unicode-org/icu4x
- `zerofrom` (Unicode-3.0): https://github.com/unicode-org/icu4x
- `zerofrom-derive` (Unicode-3.0): https://github.com/unicode-org/icu4x
- `zerotrie` (Unicode-3.0): https://github.com/unicode-org/icu4x
- `zerovec` (Unicode-3.0): https://github.com/unicode-org/icu4x
- `zerovec-derive` (Unicode-3.0): https://github.com/unicode-org/icu4x

### Unicode License v3 (`Unicode-3.0`)

````text
UNICODE LICENSE V3

COPYRIGHT AND PERMISSION NOTICE

Copyright © 1991-2023 Unicode, Inc.

NOTICE TO USER: Carefully read the following legal agreement. BY
DOWNLOADING, INSTALLING, COPYING OR OTHERWISE USING DATA FILES, AND/OR
SOFTWARE, YOU UNEQUIVOCALLY ACCEPT, AND AGREE TO BE BOUND BY, ALL OF THE
TERMS AND CONDITIONS OF THIS AGREEMENT. IF YOU DO NOT AGREE, DO NOT
DOWNLOAD, INSTALL, COPY, DISTRIBUTE OR USE THE DATA FILES OR SOFTWARE.

Permission is hereby granted, free of charge, to any person obtaining a
copy of data files and any associated documentation (the "Data Files") or
software and any associated documentation (the "Software") to deal in the
Data Files or Software without restriction, including without limitation
the rights to use, copy, modify, merge, publish, distribute, and/or sell
copies of the Data Files or Software, and to permit persons to whom the
Data Files or Software are furnished to do so, provided that either (a)
this copyright and permission notice appear with all copies of the Data
Files or Software, or (b) this copyright and permission notice appear in
associated Documentation.

THE DATA FILES AND SOFTWARE ARE PROVIDED "AS IS", WITHOUT WARRANTY OF ANY
KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF
MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT OF
THIRD PARTY RIGHTS.

IN NO EVENT SHALL THE COPYRIGHT HOLDER OR HOLDERS INCLUDED IN THIS NOTICE
BE LIABLE FOR ANY CLAIM, OR ANY SPECIAL INDIRECT OR CONSEQUENTIAL DAMAGES,
OR ANY DAMAGES WHATSOEVER RESULTING FROM LOSS OF USE, DATA OR PROFITS,
WHETHER IN AN ACTION OF CONTRACT, NEGLIGENCE OR OTHER TORTIOUS ACTION,
ARISING OUT OF OR IN CONNECTION WITH THE USE OR PERFORMANCE OF THE DATA
FILES OR SOFTWARE.

Except as contained in this notice, the name of a copyright holder shall
not be used in advertising or otherwise to promote the sale, use or other
dealings in these Data Files or Software without prior written
authorization of the copyright holder.
````

- `unicode-ident` ((MIT OR Apache-2.0) AND Unicode-3.0): https://github.com/dtolnay/unicode-ident
- `unicode-normalization` ((MIT OR Apache-2.0) AND Unicode-3.0): https://github.com/unicode-rs/unicode-normalization

### bzip2 and libbzip2 License v1.0.6 (`bzip2-1.0.6`)

````text
--------------------------------------------------------------------------

The original program, "bzip2", the associated library "libbzip2", and all
documentation, are

Copyright (C) 1996-2021 Julian R Seward.
Copyright (C) 2019-2020 Federico Mena Quintero
Copyright (C) 2021 Micah Snyder

This Rust translation, "libbzip2-rs" is a derived work based on "bzip2" and
"libbzip2", and is Copyright (C) 2024-2025 Trifecta Tech Foundation and contributors

Redistribution and use in source and binary forms, with or without
modification, are permitted provided that the following conditions
are met:

1. Redistributions of source code must retain the above copyright
   notice, this list of conditions and the following disclaimer.

2. The origin of this software must not be misrepresented; you must
   not claim that you wrote the original software.  If you use this
   software in a product, an acknowledgment in the product
   documentation would be appreciated but is not required.

3. Altered source versions must be plainly marked as such, and must
   not be misrepresented as being the original software.

4. The name of the author may not be used to endorse or promote
   products derived from this software without specific prior written
   permission.

THIS SOFTWARE IS PROVIDED BY THE AUTHOR ``AS IS'' AND ANY EXPRESS
OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE IMPLIED
WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE
ARE DISCLAIMED.  IN NO EVENT SHALL THE AUTHOR BE LIABLE FOR ANY
DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL
DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE
GOODS OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS
INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING
NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE OF THIS
SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

Julian Seward, jseward@acm.org
bzip2/libbzip2 version 1.1.0 of 6 September 2010

--------------------------------------------------------------------------
````

- `libbz2-rs-sys` (bzip2-1.0.6): https://github.com/trifectatechfoundation/libbzip2-rs

### zlib License (`Zlib`)

````text
zlib License

This software is provided 'as-is', without any express or implied warranty.  In no event will the authors be held liable for any damages arising from the use of this software.

Permission is granted to anyone to use this software for any purpose, including commercial applications, and to alter it and redistribute it freely, subject to the following restrictions:

     1. The origin of this software must not be misrepresented; you must not claim that you wrote the original software. If you use this software in a product, an acknowledgment in the product documentation would be appreciated but is not required.

     2. Altered source versions must be plainly marked as such, and must not be misrepresented as being the original software.

     3. This notice may not be removed or altered from any source distribution.
````

- `crc-turbo` ((MIT OR Apache-2.0) AND Zlib): https://github.com/scryer-media/crc-turbo; ships no file with this licence's text
- `foldhash` (Zlib): https://github.com/orlp/foldhash
  - Copyright (c) 2024 Orson Peters

## Go modules (the window)

The window (`bmseeker-gui`, and the web page's) is written in Go: these are the modules it links on every system it is built for, and Go's own standard library and runtime.

7 modules (Go itself one of them), 5 licence texts (each once, then the modules under it):

- Apache License 2.0 (`Apache-2.0`): 1 module, 1 text
- BSD 3-Clause "New" or "Revised" License (`BSD-3-Clause`): 4 modules, 1 text
- Go's additional IP rights grant (patents): 4 modules, 1 text
- MIT License (`MIT`): 1 module, 1 text
- The Unlicense or BSD 3-Clause License (`Unlicense OR BSD-3-Clause`): 1 module, 1 text

### Apache License 2.0 (`Apache-2.0`)

````text
                                 Apache License
                           Version 2.0, January 2004
                        http://www.apache.org/licenses/

   TERMS AND CONDITIONS FOR USE, REPRODUCTION, AND DISTRIBUTION

   1. Definitions.

      "License" shall mean the terms and conditions for use, reproduction,
      and distribution as defined by Sections 1 through 9 of this document.

      "Licensor" shall mean the copyright owner or entity authorized by
      the copyright owner that is granting the License.

      "Legal Entity" shall mean the union of the acting entity and all
      other entities that control, are controlled by, or are under common
      control with that entity. For the purposes of this definition,
      "control" means (i) the power, direct or indirect, to cause the
      direction or management of such entity, whether by contract or
      otherwise, or (ii) ownership of fifty percent (50%) or more of the
      outstanding shares, or (iii) beneficial ownership of such entity.

      "You" (or "Your") shall mean an individual or Legal Entity
      exercising permissions granted by this License.

      "Source" form shall mean the preferred form for making modifications,
      including but not limited to software source code, documentation
      source, and configuration files.

      "Object" form shall mean any form resulting from mechanical
      transformation or translation of a Source form, including but
      not limited to compiled object code, generated documentation,
      and conversions to other media types.

      "Work" shall mean the work of authorship, whether in Source or
      Object form, made available under the License, as indicated by a
      copyright notice that is included in or attached to the work
      (an example is provided in the Appendix below).

      "Derivative Works" shall mean any work, whether in Source or Object
      form, that is based on (or derived from) the Work and for which the
      editorial revisions, annotations, elaborations, or other modifications
      represent, as a whole, an original work of authorship. For the purposes
      of this License, Derivative Works shall not include works that remain
      separable from, or merely link (or bind by name) to the interfaces of,
      the Work and Derivative Works thereof.

      "Contribution" shall mean any work of authorship, including
      the original version of the Work and any modifications or additions
      to that Work or Derivative Works thereof, that is intentionally
      submitted to Licensor for inclusion in the Work by the copyright owner
      or by an individual or Legal Entity authorized to submit on behalf of
      the copyright owner. For the purposes of this definition, "submitted"
      means any form of electronic, verbal, or written communication sent
      to the Licensor or its representatives, including but not limited to
      communication on electronic mailing lists, source code control systems,
      and issue tracking systems that are managed by, or on behalf of, the
      Licensor for the purpose of discussing and improving the Work, but
      excluding communication that is conspicuously marked or otherwise
      designated in writing by the copyright owner as "Not a Contribution."

      "Contributor" shall mean Licensor and any individual or Legal Entity
      on behalf of whom a Contribution has been received by Licensor and
      subsequently incorporated within the Work.

   2. Grant of Copyright License. Subject to the terms and conditions of
      this License, each Contributor hereby grants to You a perpetual,
      worldwide, non-exclusive, no-charge, royalty-free, irrevocable
      copyright license to reproduce, prepare Derivative Works of,
      publicly display, publicly perform, sublicense, and distribute the
      Work and such Derivative Works in Source or Object form.

   3. Grant of Patent License. Subject to the terms and conditions of
      this License, each Contributor hereby grants to You a perpetual,
      worldwide, non-exclusive, no-charge, royalty-free, irrevocable
      (except as stated in this section) patent license to make, have made,
      use, offer to sell, sell, import, and otherwise transfer the Work,
      where such license applies only to those patent claims licensable
      by such Contributor that are necessarily infringed by their
      Contribution(s) alone or by combination of their Contribution(s)
      with the Work to which such Contribution(s) was submitted. If You
      institute patent litigation against any entity (including a
      cross-claim or counterclaim in a lawsuit) alleging that the Work
      or a Contribution incorporated within the Work constitutes direct
      or contributory patent infringement, then any patent licenses
      granted to You under this License for that Work shall terminate
      as of the date such litigation is filed.

   4. Redistribution. You may reproduce and distribute copies of the
      Work or Derivative Works thereof in any medium, with or without
      modifications, and in Source or Object form, provided that You
      meet the following conditions:

      (a) You must give any other recipients of the Work or
          Derivative Works a copy of this License; and

      (b) You must cause any modified files to carry prominent notices
          stating that You changed the files; and

      (c) You must retain, in the Source form of any Derivative Works
          that You distribute, all copyright, patent, trademark, and
          attribution notices from the Source form of the Work,
          excluding those notices that do not pertain to any part of
          the Derivative Works; and

      (d) If the Work includes a "NOTICE" text file as part of its
          distribution, then any Derivative Works that You distribute must
          include a readable copy of the attribution notices contained
          within such NOTICE file, excluding those notices that do not
          pertain to any part of the Derivative Works, in at least one
          of the following places: within a NOTICE text file distributed
          as part of the Derivative Works; within the Source form or
          documentation, if provided along with the Derivative Works; or,
          within a display generated by the Derivative Works, if and
          wherever such third-party notices normally appear. The contents
          of the NOTICE file are for informational purposes only and
          do not modify the License. You may add Your own attribution
          notices within Derivative Works that You distribute, alongside
          or as an addendum to the NOTICE text from the Work, provided
          that such additional attribution notices cannot be construed
          as modifying the License.

      You may add Your own copyright statement to Your modifications and
      may provide additional or different license terms and conditions
      for use, reproduction, or distribution of Your modifications, or
      for any such Derivative Works as a whole, provided Your use,
      reproduction, and distribution of the Work otherwise complies with
      the conditions stated in this License.

   5. Submission of Contributions. Unless You explicitly state otherwise,
      any Contribution intentionally submitted for inclusion in the Work
      by You to the Licensor shall be under the terms and conditions of
      this License, without any additional terms or conditions.
      Notwithstanding the above, nothing herein shall supersede or modify
      the terms of any separate license agreement you may have executed
      with Licensor regarding such Contributions.

   6. Trademarks. This License does not grant permission to use the trade
      names, trademarks, service marks, or product names of the Licensor,
      except as required for reasonable and customary use in describing the
      origin of the Work and reproducing the content of the NOTICE file.

   7. Disclaimer of Warranty. Unless required by applicable law or
      agreed to in writing, Licensor provides the Work (and each
      Contributor provides its Contributions) on an "AS IS" BASIS,
      WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or
      implied, including, without limitation, any warranties or conditions
      of TITLE, NON-INFRINGEMENT, MERCHANTABILITY, or FITNESS FOR A
      PARTICULAR PURPOSE. You are solely responsible for determining the
      appropriateness of using or redistributing the Work and assume any
      risks associated with Your exercise of permissions under this License.

   8. Limitation of Liability. In no event and under no legal theory,
      whether in tort (including negligence), contract, or otherwise,
      unless required by applicable law (such as deliberate and grossly
      negligent acts) or agreed to in writing, shall any Contributor be
      liable to You for damages, including any direct, indirect, special,
      incidental, or consequential damages of any character arising as a
      result of this License or out of the use or inability to use the
      Work (including but not limited to damages for loss of goodwill,
      work stoppage, computer failure or malfunction, or any and all
      other commercial damages or losses), even if such Contributor
      has been advised of the possibility of such damages.

   9. Accepting Warranty or Additional Liability. While redistributing
      the Work or Derivative Works thereof, You may choose to offer,
      and charge a fee for, acceptance of support, warranty, indemnity,
      or other liability obligations and/or rights consistent with this
      License. However, in accepting such obligations, You may act only
      on Your own behalf and on Your sole responsibility, not on behalf
      of any other Contributor, and only if You agree to indemnify,
      defend, and hold each Contributor harmless for any liability
      incurred by, or claims asserted against, such Contributor by reason
      of your accepting any such warranty or additional liability.

   END OF TERMS AND CONDITIONS

   APPENDIX: How to apply the Apache License to your work.

      To apply the Apache License to your work, attach the following
      boilerplate notice, with the fields enclosed by brackets "{}"
      replaced with your own identifying information. (Don't include
      the brackets!)  The text should be enclosed in the appropriate
      comment syntax for the file format. We also recommend that a
      file or class name and description of purpose be included on the
      same "printed page" as the copyright notice for easier
      identification within third-party archives.

   Copyright {yyyy} {name of copyright owner}

   Licensed under the Apache License, Version 2.0 (the "License");
   you may not use this file except in compliance with the License.
   You may obtain a copy of the License at

       http://www.apache.org/licenses/LICENSE-2.0

   Unless required by applicable law or agreed to in writing, software
   distributed under the License is distributed on an "AS IS" BASIS,
   WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
   See the License for the specific language governing permissions and
   limitations under the License.
````

- `github.com/ebitengine/purego`: https://pkg.go.dev/github.com/ebitengine/purego

### BSD 3-Clause "New" or "Revised" License (`BSD-3-Clause`)

````text
Redistribution and use in source and binary forms, with or without
modification, are permitted provided that the following conditions are
met:

   * Redistributions of source code must retain the above copyright
notice, this list of conditions and the following disclaimer.
   * Redistributions in binary form must reproduce the above
copyright notice, this list of conditions and the following disclaimer
in the documentation and/or other materials provided with the
distribution.
   * Neither the name of Google LLC nor the names of its
contributors may be used to endorse or promote products derived from
this software without specific prior written permission.

THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS
"AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT
LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR
A PARTICULAR PURPOSE ARE DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT
OWNER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT
LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR SERVICES; LOSS OF USE,
DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY
THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT
(INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE
OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.
````

- Go (its standard library and runtime, in every Go program): https://go.dev
  - Copyright 2009 The Go Authors.
- `golang.org/x/image`: https://pkg.go.dev/golang.org/x/image
  - Copyright 2009 The Go Authors.
- `golang.org/x/net`: https://pkg.go.dev/golang.org/x/net
  - Copyright 2009 The Go Authors.
- `golang.org/x/text`: https://pkg.go.dev/golang.org/x/text
  - Copyright 2009 The Go Authors.

### Go's additional IP rights grant (patents)

````text
Additional IP Rights Grant (Patents)

"This implementation" means the copyrightable works distributed by
Google as part of the Go project.

Google hereby grants to You a perpetual, worldwide, non-exclusive,
no-charge, royalty-free, irrevocable (except as stated in this section)
patent license to make, have made, use, offer to sell, sell, import,
transfer and otherwise run, modify and propagate the contents of this
implementation of Go, where such license applies only to those patent
claims, both currently owned or controlled by Google and acquired in
the future, licensable by Google that are necessarily infringed by this
implementation of Go.  This grant does not include claims that would be
infringed only as a consequence of further modification of this
implementation.  If you or your agent or exclusive licensee institute or
order or agree to the institution of patent litigation against any
entity (including a cross-claim or counterclaim in a lawsuit) alleging
that this implementation of Go or any code incorporated within this
implementation of Go constitutes direct or contributory patent
infringement, or inducement of patent infringement, then any patent
rights granted to you under this License for this implementation of Go
shall terminate as of the date such litigation is filed.
````

- Go (its standard library and runtime, in every Go program): https://go.dev
- `golang.org/x/image`: https://pkg.go.dev/golang.org/x/image
- `golang.org/x/net`: https://pkg.go.dev/golang.org/x/net
- `golang.org/x/text`: https://pkg.go.dev/golang.org/x/text

### MIT License (`MIT`)

````text
MIT License

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
````

- `github.com/egoist/mygo`: https://pkg.go.dev/github.com/egoist/mygo
  - Copyright (c) 2026 MyGo contributors

### The Unlicense or BSD 3-Clause License (`Unlicense OR BSD-3-Clause`)

````text
This project is provided under the terms of the UNLICENSE or
the BSD license denoted by the following SPDX identifier:

SPDX-License-Identifier: Unlicense OR BSD-3-Clause

You may use the project under the terms of either license.

Both licenses are reproduced below.

----
The BSD 3 Clause License

Copyright 2021 The go-text authors

Redistribution and use in source and binary forms, with or without modification, are permitted provided that the following conditions are met:

1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following disclaimer.

2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the following disclaimer in the documentation and/or other materials provided with the distribution.

3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote products derived from this software without specific prior written permission.

THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.
---



---
The UNLICENSE

This is free and unencumbered software released into the public domain.

Anyone is free to copy, modify, publish, use, compile, sell, or
distribute this software, either in source code form or as a compiled
binary, for any purpose, commercial or non-commercial, and by any
means.

In jurisdictions that recognize copyright laws, the author or authors
of this software dedicate any and all copyright interest in the
software to the public domain. We make this dedication for the benefit
of the public at large and to the detriment of our heirs and
successors. We intend this dedication to be an overt act of
relinquishment in perpetuity of all present and future rights to this
software under copyright law.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND,
EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF
MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT.
IN NO EVENT SHALL THE AUTHORS BE LIABLE FOR ANY CLAIM, DAMAGES OR
OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE,
ARISING FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR
OTHER DEALINGS IN THE SOFTWARE.

For more information, please refer to <https://unlicense.org/>
---
````

- `github.com/go-text/typesetting`: https://pkg.go.dev/github.com/go-text/typesetting
