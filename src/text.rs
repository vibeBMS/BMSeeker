//! Text from BMS files and pages (charts, file names, venue pages).
//!
//! BMS files are traditionally Shift_JIS (MS932); Korean charts are often
//! EUC-KR (CP949) and newer ones UTF-8. bmseeker picks UTF-8 when the bytes are valid
//! UTF-8, otherwise try Shift_JIS and fall back to EUC-KR when Shift_JIS
//! decoding produces errors but EUC-KR does not.

use encoding_rs::{EUC_KR, SHIFT_JIS, UTF_8};

pub fn decode_text(bytes: &[u8]) -> String {
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

/// HTML 4's named character references (sorted by name).
const ENTITIES: &[(&str, u32)] = &[
    ("AElig", 198),
    ("Aacute", 193),
    ("Acirc", 194),
    ("Agrave", 192),
    ("Alpha", 913),
    ("Aring", 197),
    ("Atilde", 195),
    ("Auml", 196),
    ("Beta", 914),
    ("Ccedil", 199),
    ("Chi", 935),
    ("Dagger", 8225),
    ("Delta", 916),
    ("ETH", 208),
    ("Eacute", 201),
    ("Ecirc", 202),
    ("Egrave", 200),
    ("Epsilon", 917),
    ("Eta", 919),
    ("Euml", 203),
    ("Gamma", 915),
    ("Iacute", 205),
    ("Icirc", 206),
    ("Igrave", 204),
    ("Iota", 921),
    ("Iuml", 207),
    ("Kappa", 922),
    ("Lambda", 923),
    ("Mu", 924),
    ("Ntilde", 209),
    ("Nu", 925),
    ("OElig", 338),
    ("Oacute", 211),
    ("Ocirc", 212),
    ("Ograve", 210),
    ("Omega", 937),
    ("Omicron", 927),
    ("Oslash", 216),
    ("Otilde", 213),
    ("Ouml", 214),
    ("Phi", 934),
    ("Pi", 928),
    ("Prime", 8243),
    ("Psi", 936),
    ("Rho", 929),
    ("Scaron", 352),
    ("Sigma", 931),
    ("THORN", 222),
    ("Tau", 932),
    ("Theta", 920),
    ("Uacute", 218),
    ("Ucirc", 219),
    ("Ugrave", 217),
    ("Upsilon", 933),
    ("Uuml", 220),
    ("Xi", 926),
    ("Yacute", 221),
    ("Yuml", 376),
    ("Zeta", 918),
    ("aacute", 225),
    ("acirc", 226),
    ("acute", 180),
    ("aelig", 230),
    ("agrave", 224),
    ("alefsym", 8501),
    ("alpha", 945),
    ("amp", 38),
    ("and", 8743),
    ("ang", 8736),
    ("aring", 229),
    ("asymp", 8776),
    ("atilde", 227),
    ("auml", 228),
    ("bdquo", 8222),
    ("beta", 946),
    ("brvbar", 166),
    ("bull", 8226),
    ("cap", 8745),
    ("ccedil", 231),
    ("cedil", 184),
    ("cent", 162),
    ("chi", 967),
    ("circ", 710),
    ("clubs", 9827),
    ("cong", 8773),
    ("copy", 169),
    ("crarr", 8629),
    ("cup", 8746),
    ("curren", 164),
    ("dArr", 8659),
    ("dagger", 8224),
    ("darr", 8595),
    ("deg", 176),
    ("delta", 948),
    ("diams", 9830),
    ("divide", 247),
    ("eacute", 233),
    ("ecirc", 234),
    ("egrave", 232),
    ("empty", 8709),
    ("emsp", 8195),
    ("ensp", 8194),
    ("epsilon", 949),
    ("equiv", 8801),
    ("eta", 951),
    ("eth", 240),
    ("euml", 235),
    ("euro", 8364),
    ("exist", 8707),
    ("fnof", 402),
    ("forall", 8704),
    ("frac12", 189),
    ("frac14", 188),
    ("frac34", 190),
    ("frasl", 8260),
    ("gamma", 947),
    ("ge", 8805),
    ("gt", 62),
    ("hArr", 8660),
    ("harr", 8596),
    ("hearts", 9829),
    ("hellip", 8230),
    ("iacute", 237),
    ("icirc", 238),
    ("iexcl", 161),
    ("igrave", 236),
    ("image", 8465),
    ("infin", 8734),
    ("int", 8747),
    ("iota", 953),
    ("iquest", 191),
    ("isin", 8712),
    ("iuml", 239),
    ("kappa", 954),
    ("lArr", 8656),
    ("lambda", 955),
    ("lang", 9001),
    ("laquo", 171),
    ("larr", 8592),
    ("lceil", 8968),
    ("ldquo", 8220),
    ("le", 8804),
    ("lfloor", 8970),
    ("lowast", 8727),
    ("loz", 9674),
    ("lrm", 8206),
    ("lsaquo", 8249),
    ("lsquo", 8216),
    ("lt", 60),
    ("macr", 175),
    ("mdash", 8212),
    ("micro", 181),
    ("middot", 183),
    ("minus", 8722),
    ("mu", 956),
    ("nabla", 8711),
    ("nbsp", 160),
    ("ndash", 8211),
    ("ne", 8800),
    ("ni", 8715),
    ("not", 172),
    ("notin", 8713),
    ("nsub", 8836),
    ("ntilde", 241),
    ("nu", 957),
    ("oacute", 243),
    ("ocirc", 244),
    ("oelig", 339),
    ("ograve", 242),
    ("oline", 8254),
    ("omega", 969),
    ("omicron", 959),
    ("oplus", 8853),
    ("or", 8744),
    ("ordf", 170),
    ("ordm", 186),
    ("oslash", 248),
    ("otilde", 245),
    ("otimes", 8855),
    ("ouml", 246),
    ("para", 182),
    ("part", 8706),
    ("permil", 8240),
    ("perp", 8869),
    ("phi", 966),
    ("pi", 960),
    ("piv", 982),
    ("plusmn", 177),
    ("pound", 163),
    ("prime", 8242),
    ("prod", 8719),
    ("prop", 8733),
    ("psi", 968),
    ("quot", 34),
    ("rArr", 8658),
    ("radic", 8730),
    ("rang", 9002),
    ("raquo", 187),
    ("rarr", 8594),
    ("rceil", 8969),
    ("rdquo", 8221),
    ("real", 8476),
    ("reg", 174),
    ("rfloor", 8971),
    ("rho", 961),
    ("rlm", 8207),
    ("rsaquo", 8250),
    ("rsquo", 8217),
    ("sbquo", 8218),
    ("scaron", 353),
    ("sdot", 8901),
    ("sect", 167),
    ("shy", 173),
    ("sigma", 963),
    ("sigmaf", 962),
    ("sim", 8764),
    ("spades", 9824),
    ("sub", 8834),
    ("sube", 8838),
    ("sum", 8721),
    ("sup", 8835),
    ("sup1", 185),
    ("sup2", 178),
    ("sup3", 179),
    ("supe", 8839),
    ("szlig", 223),
    ("tau", 964),
    ("there4", 8756),
    ("theta", 952),
    ("thetasym", 977),
    ("thinsp", 8201),
    ("thorn", 254),
    ("tilde", 732),
    ("times", 215),
    ("trade", 8482),
    ("uArr", 8657),
    ("uacute", 250),
    ("uarr", 8593),
    ("ucirc", 251),
    ("ugrave", 249),
    ("uml", 168),
    ("upsih", 978),
    ("upsilon", 965),
    ("uuml", 252),
    ("weierp", 8472),
    ("xi", 958),
    ("yacute", 253),
    ("yen", 165),
    ("yuml", 255),
    ("zeta", 950),
    ("zwj", 8205),
    ("zwnj", 8204),
];

/// HTML's character references in text (`&amp;`, `&#12354;`,
/// `&#x3042;`...).
pub fn unescape(s: &str) -> String {
    unescape_with(s, true)
}

/// [`unescape`] for an attribute's value (a link): only references that
/// end in `;` (`&copy=1` in a URL stays).
pub fn unescape_attr(s: &str) -> String {
    unescape_with(s, false)
}

fn unescape_with(s: &str, legacy: bool) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find('&') {
        out.push_str(&rest[..i]);
        rest = &rest[i..];
        let end = rest.find(';').filter(|e| *e <= 10);
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

/// `n` as one file name: no path, nothing a file system refuses; None
/// when nothing is left.
pub fn safe_file_name(n: &str) -> Option<String> {
    let n: String = n
        .chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            c if c.is_control() => '_',
            c => c,
        })
        .collect();
    let n = n.trim().trim_matches('.').to_string();
    (!n.is_empty()).then_some(n)
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
