//! Lossless UTF-8 decoding and source-format preservation.
use serde::{Deserialize, Serialize};
/// Predominant newline convention, or verbatim mixed endings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Eol {
    /// Unix.
    Lf,
    /// Windows.
    Crlf,
    /// Classic Mac.
    Cr,
    /// Multiple conventions; buffer stays verbatim.
    Mixed,
}
/// Format remembered separately from editable text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextFormat {
    /// Line ending convention.
    pub eol: Eol,
    /// UTF-8 byte-order mark.
    pub bom: bool,
    /// Original final newline.
    pub trailing_newline: bool,
}
impl Default for TextFormat {
    fn default() -> Self {
        Self {
            eol: Eol::Lf,
            bom: false,
            trailing_newline: true,
        }
    }
}
/// Decode error; the caller must open the bytes read-only.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum TextError {
    /// Invalid UTF-8.
    #[error("not UTF-8; opened read-only")]
    NotUtf8,
}
/// Decode uniform endings to LF; mixed endings remain verbatim.
pub fn decode(bytes: &[u8]) -> Result<(String, TextFormat), TextError> {
    let bom = bytes.starts_with(&[0xef, 0xbb, 0xbf]);
    let text = std::str::from_utf8(if bom { &bytes[3..] } else { bytes })
        .map_err(|_| TextError::NotUtf8)?;
    let mut counts = [0; 3];
    let mut iter = text.bytes().peekable();
    while let Some(b) = iter.next() {
        match b {
            b'\r' if iter.peek() == Some(&b'\n') => {
                iter.next();
                counts[1] += 1;
            }
            b'\r' => counts[2] += 1,
            b'\n' => counts[0] += 1,
            _ => {}
        }
    }
    let eol = if counts.iter().filter(|n| **n > 0).count() > 1 {
        Eol::Mixed
    } else if counts[1] > 0 {
        Eol::Crlf
    } else if counts[2] > 0 {
        Eol::Cr
    } else {
        Eol::Lf
    };
    let trailing_newline = text.ends_with(['\r', '\n']);
    let text = match eol {
        Eol::Crlf => text.replace("\r\n", "\n"),
        Eol::Cr => text.replace('\r', "\n"),
        _ => text.to_owned(),
    };
    Ok((
        text,
        TextFormat {
            eol,
            bom,
            trailing_newline,
        },
    ))
}
/// Encode the edited buffer. A mixed buffer is never normalized.
/// Newline edits are explicit: the remembered flag is descriptive, not destructive.
pub fn encode(text: &str, fmt: &TextFormat) -> Vec<u8> {
    let mut out = if fmt.bom {
        vec![0xef, 0xbb, 0xbf]
    } else {
        Vec::new()
    };
    let text = match fmt.eol {
        Eol::Crlf => text.replace('\n', "\r\n"),
        Eol::Cr => text.replace('\n', "\r"),
        _ => text.to_owned(),
    };
    out.extend_from_slice(text.as_bytes());
    out
}
/// Fingerprint a decoded buffer for rejecting edits planned against stale text.
/// This is an in-process change detector, not a cryptographic signature.
pub fn fingerprint(text: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    text.as_bytes().hash(&mut h);
    h.finish()
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn all_formats() {
        for e in ["\n", "\r\n", "\r", "\n\r\n\r", ""] {
            for bom in [false, true] {
                for trailing in [false, true] {
                    let s = format!(
                        "{}hello{e}world{}",
                        if bom { "\u{feff}" } else { "" },
                        if trailing { e } else { "" }
                    );
                    let (t, f) = decode(s.as_bytes()).unwrap();
                    assert_eq!(encode(&t, &f), s.as_bytes());
                }
            }
        }
        for s in [b"".as_slice(), b"\xef\xbb\xbf", b"\n"] {
            let (t, f) = decode(s).unwrap();
            assert_eq!(encode(&t, &f), s);
        }
        assert_eq!(decode(b"\xff"), Err(TextError::NotUtf8));
    }
}
