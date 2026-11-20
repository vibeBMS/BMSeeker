//! Text from BMS files and pages (charts, file names, venue pages).
//!
//! BMS files are traditionally Shift_JIS (MS932); Korean charts are often
//! EUC-KR (CP949) and newer ones UTF-8. bmseeker picks UTF-8 when the bytes are valid
//! UTF-8, otherwise try Shift_JIS and fall back to EUC-KR when Shift_JIS
//! decoding produces errors but EUC-KR does not.

mod entities;
pub(crate) mod pattern;

use encoding_rs::{EUC_KR, SHIFT_JIS, UTF_8};
use entities::ENTITIES;

pub(crate) fn decode_text(bytes: &[u8]) -> String {
    let bytes = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(bytes);
    if let Ok(s) = std::str::from_utf8(bytes) {
        return s.to_owned();
    }
    let (sjis, _, sjis_err) = SHIFT_JIS.decode(bytes);
    if !sjis_err {
        // Valid Shift_JIS, but EUC-KR text is frequently *also* valid Shift_JIS
        // (as half-width katakana soup). Prefer EUC-KR when it decodes cleanly
        // and yields Hangul.
        let (euc, _, euc_err) = EUC_KR.decode(bytes);
        if !euc_err && hangul_ratio(&euc) > 0.3 && kana_ratio(&sjis) > 0.3 {
            return euc.into_owned();
        }
        return sjis.into_owned();
    }
    let (euc, _, euc_err) = EUC_KR.decode(bytes);
    if !euc_err {
        return euc.into_owned();
    }
    let (s, _, _) = UTF_8.decode(bytes);
    if s.chars().filter(|c| *c == '\u{FFFD}').count()
        <= sjis.chars().filter(|c| *c == '\u{FFFD}').count()
    {
        s.into_owned()
    } else {
        sjis.into_owned()
    }
}

fn non_ascii(s: &str) -> usize {
    s.chars().filter(|c| !c.is_ascii()).count().max(1)
}

fn hangul_ratio(s: &str) -> f64 {
    let h = s
        .chars()
        .filter(|c| ('\u{AC00}'..='\u{D7A3}').contains(c))
        .count();
    h as f64 / non_ascii(s) as f64
}

fn kana_ratio(s: &str) -> f64 {
    // half-width katakana is the tell-tale sign of mis-decoded EUC-KR
    let k = s
        .chars()
        .filter(|c| ('\u{FF61}'..='\u{FF9F}').contains(c))
        .count();
    k as f64 / non_ascii(s) as f64
}

/// `bytes` in lower case hex.
pub(crate) fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write;
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        let _ = write!(out, "{b:02x}");
    }
    out
}

/// HTML's character references in text (`&amp;`, `&#12354;`,
/// `&#x3042;`...).
pub(crate) fn unescape(s: &str) -> String {
    unescape_with(s, true)
}

/// [`unescape`] for an attribute's value (a link): only references that
/// end in `;` (`&copy=1` in a URL stays).
pub(crate) fn unescape_attr(s: &str) -> String {
    unescape_with(s, false)
}

fn unescape_with(s: &str, legacy: bool) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find('&') {
        out.push_str(&rest[..i]);
        rest = &rest[i..];
        // (a reference is ten bytes at most: no further is looked)
        let end = rest.bytes().take(11).position(|b| b == b';');
        let decoded = end.and_then(|e| {
            let name = &rest[1..e];
            let c = if let Some(n) = name.strip_prefix('#') {
                let v = match n.strip_prefix(['x', 'X']) {
                    Some(h) => u32::from_str_radix(h, 16).ok()?,
                    None => n.parse().ok()?,
                };
                char::from_u32(v)?
            } else {
                if name == "apos" {
                    '\''
                } else {
                    let i = ENTITIES.binary_search_by(|(k, _)| (*k).cmp(name)).ok()?;
                    char::from_u32(ENTITIES[i].1)?
                }
            };
            Some((c, e + 1))
        });
        // as HTML5 reads them: a few old names need no ';' ("&lt", "&eacute")
        let decoded = decoded.or_else(|| {
            (2..=6).filter(|_| legacy).rev().find_map(|n| {
                let name = rest.get(1..1 + n)?;
                let i = ENTITIES.binary_search_by(|(k, _)| (*k).cmp(name)).ok()?;
                let cp = ENTITIES[i].1;
                if cp <= 0xff {
                    Some((char::from_u32(cp)?, 1 + n))
                } else {
                    None
                }
            })
        });
        match decoded {
            Some((c, len)) => {
                out.push(c);
                rest = &rest[len..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

/// `n` as one file name: no path, no dot before it, nothing a file
/// system refuses, Windows' too ([`safe_part`](crate::fs::safe_part): no
/// dot or space at its end, no device's name); None when nothing is left.
pub(crate) fn safe_file_name(n: &str) -> Option<String> {
    crate::fs::safe_part(n.trim().trim_start_matches('.'))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn references_in_a_link() {
        // in text, old names need no ';'; in a link they do
        assert_eq!(unescape("a&copy=1"), "a\u{a9}=1");
        assert_eq!(unescape_attr("?a=1&copy=2&amp;b=&#x2F;"), "?a=1&copy=2&b=/");
        assert_eq!(safe_file_name(" ..a/b?. "), Some("a_b_".into()));
        assert_eq!(safe_file_name("..."), None);
        // Windows' names too: a device's, a dot or a space at the end
        assert_eq!(safe_file_name("NUL"), Some("_NUL".into()));
        assert_eq!(safe_file_name("com1.zip"), Some("_com1.zip".into()));
        assert_eq!(safe_file_name("a.zip. ."), Some("a.zip".into()));
        assert_eq!(
            crate::net::disposition_name("attachment; filename=\"aux.bms.\"").as_deref(),
            Some("_aux.bms")
        );
        assert_eq!(
            crate::hosts::url_file_name("https://h.example/d/CON").as_deref(),
            Some("_CON")
        );
    }

    /// A text of nothing but `&` costs its length, not its square.
    #[test]
    fn many_references() {
        let n = 2_000_000;
        let t = std::time::Instant::now();
        assert_eq!(unescape(&"&".repeat(n)).len(), n);
        assert_eq!(unescape(&"&amp".repeat(n / 4)).len(), n / 4);
        assert!(t.elapsed() < std::time::Duration::from_secs(5));
        // a reference is looked for no further than it can be long
        assert_eq!(unescape("&#x10FFFF;"), "\u{10ffff}");
        assert_eq!(unescape("&#65;&#00000000065;x"), "A&#00000000065;x");
        assert_eq!(unescape("&thetasym;&nbsp;"), "\u{3d1}\u{a0}");
    }

    #[test]
    fn decodes_common_encodings() {
        assert_eq!(decode_text("#TITLE テスト".as_bytes()), "#TITLE テスト");
        let (sjis, _, _) = SHIFT_JIS.encode("#TITLE テスト曲");
        assert_eq!(decode_text(&sjis), "#TITLE テスト曲");
        let (euc, _, _) = EUC_KR.encode("#TITLE 테스트 곡 제목입니다");
        assert_eq!(decode_text(&euc), "#TITLE 테스트 곡 제목입니다");
    }
}
