//! Local file clipboard formats interoperable with Trek and GNOME.
use std::fmt::Write;
/// Malformed, remote or non-UTF-8 clipboard URI.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
#[error("invalid local file clipboard payload")]
pub struct ClipError;
/// Percent-encode a local absolute UTF-8 path, retaining slash and URI-safe bytes.
pub fn uri(path: &str) -> Result<String, ClipError> {
    if !path.starts_with('/') || path.contains('\0') {
        return Err(ClipError);
    }
    let mut out = "file://".to_owned();
    for b in path.bytes() {
        if b.is_ascii_alphanumeric() || b"/-._~".contains(&b) {
            out.push(char::from(b));
        } else {
            let _ = write!(out, "%{b:02X}");
        }
    }
    Ok(out)
}
fn path(uri: &str) -> Result<String, ClipError> {
    let encoded = uri.strip_prefix("file://").ok_or(ClipError)?;
    if !encoded.starts_with('/') {
        return Err(ClipError);
    }
    let mut out = vec![];
    let mut iter = encoded.bytes();
    while let Some(b) = iter.next() {
        if b == b'%' {
            let a = char::from(iter.next().ok_or(ClipError)?)
                .to_digit(16)
                .ok_or(ClipError)?;
            let b = char::from(iter.next().ok_or(ClipError)?)
                .to_digit(16)
                .ok_or(ClipError)?;
            out.push(u8::try_from(a * 16 + b).map_err(|_| ClipError)?);
        } else {
            out.push(b);
        }
    }
    let s = String::from_utf8(out).map_err(|_| ClipError)?;
    if s.contains('\0') {
        Err(ClipError)
    } else {
        Ok(s)
    }
}
/// Encode RFC URI-list using CRLF and a trailing separator.
pub fn encode_uri_list(paths: &[String]) -> Result<String, ClipError> {
    let mut out = String::new();
    for p in paths {
        out.push_str(&uri(p)?);
        out.push_str("\r\n");
    }
    Ok(out)
}
/// Decode URI-list, skipping empty and comment lines.
pub fn decode_uri_list(text: &str) -> Result<Vec<String>, ClipError> {
    text.lines()
        .filter(|s| !s.is_empty() && !s.starts_with('#'))
        .map(path)
        .collect()
}
/// Encode GNOME's cut/copy format with LF and no trailing separator.
pub fn encode_gnome_copied(paths: &[String], cut: bool) -> Result<String, ClipError> {
    Ok(format!(
        "{}\n{}",
        if cut { "cut" } else { "copy" },
        paths
            .iter()
            .map(|p| uri(p))
            .collect::<Result<Vec<_>, _>>()?
            .join("\n")
    ))
}
/// Decode the GNOME header and URI payload.
pub fn decode_gnome_copied(text: &str) -> Result<(Vec<String>, bool), ClipError> {
    let (header, body) = text.split_once('\n').ok_or(ClipError)?;
    let cut = match header.trim_end_matches('\r') {
        "cut" => true,
        "copy" => false,
        _ => return Err(ClipError),
    };
    Ok((decode_uri_list(body)?, cut))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn interop() {
        let paths = vec!["/tmp/line\nbreak".into(), "/tmp/café #1.md".into()];
        assert_eq!(
            decode_uri_list(&encode_uri_list(&paths).unwrap()).unwrap(),
            paths
        );
        for cut in [true, false] {
            assert_eq!(
                decode_gnome_copied(&encode_gnome_copied(&paths, cut).unwrap()).unwrap(),
                (paths.clone(), cut)
            );
        }
        assert_eq!(
            encode_gnome_copied(&["/tmp/line\nbreak".into()], true).unwrap(),
            include_str!("../tests/fixtures/trek-clipboard.txt").trim_end_matches('\n')
        );
        for s in ["file://remote/tmp/a", "https://x", "file:///tmp/%xy"] {
            assert!(decode_uri_list(s).is_err());
        }
    }
}
