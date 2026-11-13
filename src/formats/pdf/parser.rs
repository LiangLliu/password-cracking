use super::crypto::{PADDING, build_md5_tail_blocks};
use anyhow::{Context, Result, bail};
use md5::{Digest as _, Md5};
use memchr::memmem;

/// Parsed fields from the PDF `/Encrypt` dictionary.
#[derive(Debug, Clone)]
pub(super) struct PdfEncryption {
    pub(super) version: u32,
    pub(super) revision: u32,
    pub(super) key_len: usize,
    pub(super) owner_hash: Vec<u8>,
    pub(super) user_hash: Vec<u8>,
    pub(super) block0_o_words: [u32; 8],
    pub(super) derive_tail_blocks: Vec<[u32; 16]>,
    pub(super) r3_target: [u8; 16],
    pub(super) u_validation_salt: Option<[u8; 8]>,
    pub(super) o_validation_salt: Option<[u8; 8]>,
}

#[inline]
fn is_pdf_delim(b: u8) -> bool {
    b.is_ascii_whitespace()
        || matches!(b, b'(' | b')' | b'<' | b'>' | b'[' | b']' | b'{' | b'}' | b'/' | b'%')
}

#[inline]
fn skip_literal_string(bytes: &[u8], start: usize) -> usize {
    let mut i = start + 1;
    let mut depth = 1usize;
    while i < bytes.len() {
        match bytes[i] {
            b'\\' => i += 2,
            b'(' => {
                depth += 1;
                i += 1;
            }
            b')' => {
                depth -= 1;
                i += 1;
                if depth == 0 {
                    return i;
                }
            }
            _ => i += 1,
        }
    }
    bytes.len()
}

#[inline]
fn skip_comment(bytes: &[u8], start: usize) -> usize {
    let mut i = start + 1;
    while i < bytes.len() && bytes[i] != b'\n' && bytes[i] != b'\r' {
        i += 1;
    }
    i
}

#[inline]
fn skip_hex_string(bytes: &[u8], start: usize) -> usize {
    memchr::memchr(b'>', &bytes[start + 1..]).map_or(bytes.len(), |end| start + 1 + end + 1)
}

#[inline]
fn skip_ws_and_comments(bytes: &[u8], mut i: usize) -> usize {
    while i < bytes.len() {
        if bytes[i].is_ascii_whitespace() {
            i += 1;
        } else if bytes[i] == b'%' {
            i = skip_comment(bytes, i);
        } else {
            break;
        }
    }
    i
}

/// Extract a balanced `<< ... >>` dictionary starting at `start`.
fn extract_balanced_dict(data: &[u8], start: usize) -> Option<&[u8]> {
    if !data[start..].starts_with(b"<<") {
        return None;
    }
    let mut depth = 0i32;
    let mut i = start;
    while i < data.len() {
        match data[i] {
            b'%' => i = skip_comment(data, i),
            b'(' => i = skip_literal_string(data, i),
            b'<' if data[i..].starts_with(b"<<") => {
                depth += 1;
                i += 2;
            }
            b'<' => i = skip_hex_string(data, i),
            b'>' if data[i..].starts_with(b">>") => {
                depth -= 1;
                i += 2;
                if depth == 0 {
                    return Some(&data[start..i]);
                }
            }
            _ => i += 1,
        }
    }
    None
}

/// Parse the `/Encrypt` dictionary from a PDF file.
pub(super) fn parse_encryption(data: &[u8]) -> Result<PdfEncryption> {
    if memmem::rfind(data, b"/Encrypt").is_none() {
        bail!("PDF is not encrypted");
    }

    // Check both classic `trailer <<...>>` and PDF 1.5+ cross-reference stream
    // dicts, preferring whichever `/Encrypt`-bearing dictionary appears nearest EOF.
    let trailer_cand = find_trailer(data);
    let trailing_cand = find_trailing_dict(data);

    let (first, second) = match (trailer_cand, trailing_cand) {
        (Some((pos_t, t)), Some((pos_d, d))) => {
            if pos_d > pos_t {
                (Some(d), Some(t))
            } else {
                (Some(t), Some(d))
            }
        }
        (Some((_, t)), None) => (Some(t), None),
        (None, Some((_, d))) => (Some(d), None),
        (None, None) => bail!("No trailer or /Encrypt dictionary found in PDF"),
    };

    let (trailer, encrypt_ref) = first
        .and_then(|t| parse_dict_value(t, b"/Encrypt").map(|v| (t, v)))
        .or_else(|| second.and_then(|t| parse_dict_value(t, b"/Encrypt").map(|v| (t, v))))
        .context("Missing /Encrypt in trailer")?;

    let id0 = parse_id_array(trailer)?;

    // Support both indirect `/Encrypt 4 0 R` and inline `/Encrypt << ... >>`.
    let owned_dict;
    let enc_dict: &[u8] = if encrypt_ref.trim_ascii_start().starts_with(b"<<") {
        encrypt_ref
    } else {
        owned_dict = resolve_object(data, encrypt_ref)?;
        &owned_dict
    };

    let version = parse_int(enc_dict, b"/V").unwrap_or(0);
    let revision = parse_int(enc_dict, b"/R").unwrap_or(0);
    let length = parse_int(enc_dict, b"/Length").unwrap_or(40);
    let permissions = parse_i32(enc_dict, b"/P").unwrap_or(0);
    let owner_hash = parse_pdf_string(enc_dict, b"/O").context("Missing /O")?;
    let user_hash = parse_pdf_string(enc_dict, b"/U").context("Missing /U")?;
    // R>=4 mixes this flag into the file key. The default is true.
    let encrypt_metadata = parse_dict_value(enc_dict, b"/EncryptMetadata")
        .map(|v| !v.trim_ascii().eq_ignore_ascii_case(b"false"))
        .unwrap_or(true);

    if owner_hash.len() < 32 {
        bail!("Invalid /O entry length in PDF");
    }
    if user_hash.len() < 32 {
        bail!("Invalid /U entry length in PDF");
    }

    let key_len = key_length(length);
    if version < 5 && (key_len == 0 || key_len > 16) {
        bail!("Unsupported PDF key length: {length} bits");
    }

    // For V>=5, the validation salts live at bytes 32..40 of U and O.
    // U/O = [32 bytes hash][8 bytes validation salt][8 bytes key salt]
    let (u_validation_salt, o_validation_salt) = if version >= 5 {
        if user_hash.len() < 40 {
            bail!("Invalid /U entry for AES-256 PDF");
        }
        (
            user_hash.get(32..40).and_then(|s| s.try_into().ok()),
            owner_hash.get(32..40).and_then(|s| s.try_into().ok()),
        )
    } else {
        (None, None)
    };

    // Precompute Algorithm 2 suffix: `O[..32] || P || ID[0] || optional 0xFFFFFFFF`.
    let mut derive_suffix = Vec::with_capacity(32 + 4 + id0.len() + 4);
    derive_suffix.extend_from_slice(&owner_hash[..32]);
    derive_suffix.extend_from_slice(&permissions.to_le_bytes());
    derive_suffix.extend_from_slice(&id0);
    if revision >= 4 && !encrypt_metadata {
        derive_suffix.extend_from_slice(&[0xFF, 0xFF, 0xFF, 0xFF]);
    }

    let mut block0_o_words = [0u32; 8];
    for i in 0..8 {
        block0_o_words[i] = u32::from_le_bytes(derive_suffix[i * 4..i * 4 + 4].try_into().unwrap());
    }
    let derive_tail_blocks = build_md5_tail_blocks(&derive_suffix[32..], 32 + derive_suffix.len());

    let r3_target = if revision >= 3 && version < 5 {
        let mut hasher = Md5::new();
        hasher.update(PADDING);
        hasher.update(&id0);
        hasher.finalize().into()
    } else {
        [0u8; 16]
    };

    Ok(PdfEncryption {
        version,
        revision,
        key_len,
        owner_hash,
        user_hash,
        block0_o_words,
        derive_tail_blocks,
        r3_target,
        u_validation_salt,
        o_validation_salt,
    })
}

/// Find the last dictionary containing `/Encrypt` along with its byte offset.
fn find_trailing_dict(data: &[u8]) -> Option<(usize, &[u8])> {
    for pos in memmem::rfind_iter(data, b"/Encrypt") {
        let before_ok = pos == 0 || is_pdf_delim(data[pos - 1]);
        let after_ok = data.get(pos + 8).is_some_and(|&b| is_pdf_delim(b));
        if !before_ok || !after_ok {
            continue;
        }

        // Walk backwards to find the enclosing `<<` (skipping nested dicts).
        let mut depth = 0i32;
        let mut start = None;
        let mut i = pos;
        while i >= 2 {
            let pair = &data[i - 2..i];
            if pair == b">>" {
                depth += 1;
                i -= 2;
                continue;
            } else if pair == b"<<" {
                if depth == 0 {
                    start = Some(i - 2);
                    break;
                }
                depth -= 1;
                i -= 2;
                continue;
            }
            i -= 1;
        }
        let Some(start) = start else {
            continue;
        };
        if let Some(dict) = extract_balanced_dict(data, start) {
            return Some((start, dict));
        }
    }
    None
}

/// Find the last `trailer << ... >>` dictionary in a PDF along with its offset.
fn find_trailer(data: &[u8]) -> Option<(usize, &[u8])> {
    for trailer_idx in memmem::rfind_iter(data, b"trailer") {
        let before_ok = trailer_idx == 0 || is_pdf_delim(data[trailer_idx - 1]);
        let after_ok = data.get(trailer_idx + 7).is_some_and(|&b| is_pdf_delim(b));
        if !before_ok || !after_ok {
            continue;
        }

        let after_trailer = &data[trailer_idx + 7..];
        let dict_rel = skip_ws_and_comments(after_trailer, 0);
        let dict_start = trailer_idx + 7 + dict_rel;
        if let Some(dict) = extract_balanced_dict(data, dict_start) {
            return Some((trailer_idx, dict));
        }

        // Fallback on malformed dict delimiters up to `startxref`.
        let end = memmem::find(after_trailer, b"startxref").unwrap_or(after_trailer.len());
        return Some((trailer_idx, &after_trailer[..end]));
    }
    None
}

/// Parse a value like `8 0 R` or `(raw)` from a dictionary byte slice for a top-level key.
///
/// Only keys at the top level of the dictionary (`depth == 1`) are considered: nested
/// dictionaries (e.g. `/CF << /Length 16 >>`), literal strings `(...)`, hex strings `<...>`,
/// and `%` comments are skipped.
fn parse_dict_value<'a>(dict: &'a [u8], key: &[u8]) -> Option<&'a [u8]> {
    let mut i = 0usize;
    let mut depth = 0i32;

    while i < dict.len() {
        if dict[i] == b'%' {
            i = skip_comment(dict, i);
            continue;
        }
        if dict[i] == b'(' {
            i = skip_literal_string(dict, i);
            continue;
        }
        if dict[i..].starts_with(b"<<") {
            depth += 1;
            i += 2;
            continue;
        }
        if dict[i..].starts_with(b">>") {
            depth -= 1;
            i += 2;
            continue;
        }
        // Skip hex strings so their contents cannot be mistaken for keys.
        if dict[i] == b'<' {
            i = skip_hex_string(dict, i);
            continue;
        }
        if depth == 1
            && dict[i..].starts_with(key)
            && (i == 0 || is_pdf_delim(dict[i - 1]))
            && dict.get(i + key.len()).is_some_and(|&c| is_pdf_delim(c))
        {
            let j = skip_ws_and_comments(dict, i + key.len());
            if j >= dict.len() {
                return None;
            }
            if dict[j..].starts_with(b"<<") {
                return extract_balanced_dict(dict, j);
            }
            if dict[j] == b'<' {
                let end = skip_hex_string(dict, j);
                return Some(&dict[j..end]);
            }
            if dict[j] == b'(' {
                let end = skip_literal_string(dict, j);
                return Some(&dict[j..end]);
            }
            if dict[j] == b'[' {
                let mut arr_depth = 0i32;
                let mut k = j;
                while k < dict.len() {
                    match dict[k] {
                        b'%' => k = skip_comment(dict, k),
                        b'(' => k = skip_literal_string(dict, k),
                        b'<' => k = skip_hex_string(dict, k),
                        b'[' => {
                            arr_depth += 1;
                            k += 1;
                        }
                        b']' => {
                            arr_depth -= 1;
                            k += 1;
                            if arr_depth == 0 {
                                return Some(&dict[j..k]);
                            }
                        }
                        _ => k += 1,
                    }
                }
                return Some(&dict[j..]);
            }
            if dict[j] == b'/' {
                let mut k = j + 1;
                while k < dict.len() && !is_pdf_delim(dict[k]) {
                    k += 1;
                }
                return Some(&dict[j..k]);
            }
            // Bare value: read until the next top-level key or closing delimiter.
            // Keeps multi-token indirect references intact (e.g. "/Encrypt 4 0 R").
            let mut k = j;
            while k < dict.len() {
                let c = dict[k];
                if matches!(c, b'>' | b'/' | b'[' | b']' | b'<' | b'(' | b'%') {
                    break;
                }
                k += 1;
            }
            return Some(dict[j..k].trim_ascii());
        }
        i += 1;
    }
    None
}

/// Parse the `/ID` array from the trailer to get the first element (`ID[0]`).
fn parse_id_array(trailer: &[u8]) -> Result<Vec<u8>> {
    let after = if let Some(val) = parse_dict_value(trailer, b"/ID") {
        val
    } else {
        let id_idx = memmem::find(trailer, b"/ID").context("Missing /ID in trailer")?;
        &trailer[id_idx + 3..]
    };

    let mut i = 0;
    while i < after.len() {
        if after[i] == b'<' {
            let end = skip_hex_string(after, i);
            let inner = after[i + 1..end - 1].trim_ascii();
            return hex_decode_bytes(inner);
        }
        if after[i] == b'(' {
            let end = skip_literal_string(after, i);
            return decode_literal_string(&after[i..end]);
        }
        i += 1;
    }
    bail!("No string in /ID")
}

/// Parse `num gen R` and resolve the last matching `num gen obj ... endobj` in `data`.
fn resolve_object(data: &[u8], ref_bytes: &[u8]) -> Result<Vec<u8>> {
    let ref_str = std::str::from_utf8(ref_bytes).context("Non-UTF8 object reference")?;
    let mut parts = ref_str.split_whitespace();
    let obj_num: u32 = parts
        .next()
        .context("Invalid object reference")?
        .parse()
        .with_context(|| format!("Invalid object number in reference: {ref_str}"))?;
    let gen_num: u32 = parts
        .next()
        .unwrap_or("0")
        .parse()
        .with_context(|| format!("Invalid generation number in reference: {ref_str}"))?;

    for obj_pos in memmem::rfind_iter(data, b"obj") {
        if obj_pos == 0 || !data[obj_pos - 1].is_ascii_whitespace() {
            continue;
        }
        if data.get(obj_pos + 3).is_some_and(|&b| !is_pdf_delim(b)) {
            continue;
        }

        // Parse `<obj_num> <ws> <gen_num> <ws> obj` immediately before `obj_pos`.
        let mut p = obj_pos;
        while p > 0 && data[p - 1].is_ascii_whitespace() {
            p -= 1;
        }
        let gen_end = p;
        while p > 0 && data[p - 1].is_ascii_digit() {
            p -= 1;
        }
        if p == gen_end || p == 0 || !data[p - 1].is_ascii_whitespace() {
            continue;
        }
        let parsed_gen = parse_u32_ascii(&data[p..gen_end]);

        while p > 0 && data[p - 1].is_ascii_whitespace() {
            p -= 1;
        }
        let num_end = p;
        while p > 0 && data[p - 1].is_ascii_digit() {
            p -= 1;
        }
        if p == num_end {
            continue;
        }
        if p > 0 && !is_pdf_delim(data[p - 1]) {
            continue;
        }
        let parsed_obj = parse_u32_ascii(&data[p..num_end]);

        if parsed_obj == Some(obj_num) && parsed_gen == Some(gen_num) {
            let after_obj = &data[obj_pos + 3..];
            let obj_end = find_endobj(after_obj).context("Missing endobj")?;
            return Ok(after_obj[..obj_end].to_vec());
        }
    }

    bail!("Object {obj_num} {gen_num} not found")
}

fn find_endobj(after_obj: &[u8]) -> Option<usize> {
    let mut i = 0;
    while i < after_obj.len() {
        match after_obj[i] {
            b'%' => i = skip_comment(after_obj, i),
            b'(' => i = skip_literal_string(after_obj, i),
            b'<' if !after_obj[i..].starts_with(b"<<") => i = skip_hex_string(after_obj, i),
            _ if after_obj[i..].starts_with(b"endobj")
                && (i == 0 || is_pdf_delim(after_obj[i - 1]))
                && after_obj.get(i + 6).is_none_or(|&b| is_pdf_delim(b)) =>
            {
                return Some(i);
            }
            _ => i += 1,
        }
    }
    None
}

fn parse_u32_ascii(bytes: &[u8]) -> Option<u32> {
    std::str::from_utf8(bytes).ok()?.parse().ok()
}

/// Parse an unsigned integer value for a key from a dictionary byte slice.
fn parse_int(dict: &[u8], key: &[u8]) -> Option<u32> {
    let val = parse_dict_value(dict, key)?;
    std::str::from_utf8(val).ok()?.parse().ok()
}

/// Parse a signed 32-bit integer value for a key from a dictionary byte slice.
///
/// The `/P` permissions entry is written as a signed integer by many PDF
/// producers (e.g. `-1028`), so it must not be parsed as `u32`.
fn parse_i32(dict: &[u8], key: &[u8]) -> Option<i32> {
    let val = parse_dict_value(dict, key)?;
    std::str::from_utf8(val).ok()?.parse().ok()
}

/// Parse either a `<hex>` string or `(literal)` string for a top-level dictionary key.
fn parse_pdf_string(dict: &[u8], key: &[u8]) -> Option<Vec<u8>> {
    let value = parse_dict_value(dict, key)?;
    if let Some(rest) = value.strip_prefix(b"<") {
        let end = memchr::memchr(b'>', rest)?;
        hex_decode_bytes(rest[..end].trim_ascii()).ok()
    } else if value.starts_with(b"(") {
        decode_literal_string(value).ok()
    } else {
        None
    }
}

/// Decode a PDF literal string `(...)` preserving arbitrary binary bytes.
fn decode_literal_string(bytes: &[u8]) -> Result<Vec<u8>> {
    let inner = bytes
        .strip_prefix(b"(")
        .and_then(|s| s.strip_suffix(b")"))
        .context("Invalid PDF literal string")?;
    let mut out = Vec::with_capacity(inner.len());
    let mut i = 0;
    while i < inner.len() {
        match inner[i] {
            b'\\' => {
                i += 1;
                if i >= inner.len() {
                    break;
                }
                match inner[i] {
                    b'n' => {
                        out.push(b'\n');
                        i += 1;
                    }
                    b'r' => {
                        out.push(b'\r');
                        i += 1;
                    }
                    b't' => {
                        out.push(b'\t');
                        i += 1;
                    }
                    b'b' => {
                        out.push(0x08);
                        i += 1;
                    }
                    b'f' => {
                        out.push(0x0C);
                        i += 1;
                    }
                    b'(' | b')' | b'\\' => {
                        out.push(inner[i]);
                        i += 1;
                    }
                    b'\r' => {
                        // Line continuation `\r` or `\r\n` is ignored.
                        i += 1;
                        if inner.get(i) == Some(&b'\n') {
                            i += 1;
                        }
                    }
                    b'\n' => {
                        i += 1;
                    }
                    b'0'..=b'7' => {
                        let mut val = (inner[i] - b'0') as u16;
                        i += 1;
                        for _ in 0..2 {
                            if let Some(&d @ b'0'..=b'7') = inner.get(i) {
                                val = val * 8 + (d - b'0') as u16;
                                i += 1;
                            } else {
                                break;
                            }
                        }
                        out.push((val & 0xFF) as u8);
                    }
                    other => {
                        out.push(other);
                        i += 1;
                    }
                }
            }
            b'\r' => {
                // ISO 32000-1 §7.3.4.2: unescaped CR or CRLF normalizes to LF.
                out.push(b'\n');
                i += 1;
                if inner.get(i) == Some(&b'\n') {
                    i += 1;
                }
            }
            other => {
                out.push(other);
                i += 1;
            }
        }
    }
    Ok(out)
}

#[inline]
fn hex_nibble(b: u8) -> Result<u8> {
    match b {
        b'0'..=b'9' => Ok(b - b'0'),
        b'a'..=b'f' => Ok(b - b'a' + 10),
        b'A'..=b'F' => Ok(b - b'A' + 10),
        _ => bail!("Invalid hex byte: 0x{b:02x}"),
    }
}

/// Decode a PDF hex string (ignoring ASCII whitespace; ISO allows an odd trailing digit padded with 0).
fn hex_decode_bytes(bytes: &[u8]) -> Result<Vec<u8>> {
    let mut out = Vec::with_capacity(bytes.len() / 2);
    let mut hi: Option<u8> = None;
    for &b in bytes {
        if b.is_ascii_whitespace() {
            continue;
        }
        let nib = hex_nibble(b)?;
        if let Some(h) = hi.take() {
            out.push((h << 4) | nib);
        } else {
            hi = Some(nib);
        }
    }
    if hi.is_some() {
        bail!("Hex string has odd length");
    }
    Ok(out)
}

// --- Zero-alloc RC4 & padding ---

fn key_length(bits: u32) -> usize {
    if bits > 0 { (bits / 8) as usize } else { 5 }
}

// --- Zero-alloc R6 Algorithm 2.B ---

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_decode_basic() {
        assert_eq!(hex_decode_bytes(b"48656c6c6f").unwrap(), b"Hello");
        assert_eq!(hex_decode_bytes(b"48 65 6c 6c 6f").unwrap(), b"Hello");
    }

    #[test]
    fn top_level_dict_skips_nested_length() {
        // Real-world shape: /CF has a nested /Length (bytes), while the
        // top-level /Length is in bits. The parser must return the latter.
        let dict = b"<< /CF << /StdCF << /CFM /AESV2 /Length 16 >> >> \
                    /Filter /Standard /Length 128 /O <aabb> /P -1028 /R 4 /V 4 >>";
        assert_eq!(parse_dict_value(dict, b"/Length"), Some(&b"128"[..]));
        assert_eq!(parse_dict_value(dict, b"/P"), Some(&b"-1028"[..]));
        assert_eq!(parse_dict_value(dict, b"/R"), Some(&b"4"[..]));
        assert_eq!(parse_dict_value(dict, b"/O"), Some(&b"<aabb>"[..]));
    }

    #[test]
    fn top_level_dict_keeps_indirect_reference() {
        let dict = b"<< /Size 9 /ID [<aa><bb>] /Encrypt 4 0 R >>";
        assert_eq!(parse_dict_value(dict, b"/Encrypt"), Some(&b"4 0 R"[..]));
    }

    #[test]
    fn dict_parser_handles_literal_strings_and_comments() {
        let dict =
            b"<< /Title (<< /Length 999 >>) % /Length 888\n /Length 128 /O (ab\\(c\\)\\377) >>";
        assert_eq!(parse_dict_value(dict, b"/Length"), Some(&b"128"[..]));
        assert_eq!(
            parse_pdf_string(dict, b"/O").as_deref(),
            Some(&[b'a', b'b', b'(', b'c', b')', 0xFF][..])
        );
    }

    #[test]
    fn resolve_object_respects_boundary_gen_and_incremental_updates() {
        let pdf = b"%PDF-1.4\n\
            14 0 obj\n<< /Fake true >>\nendobj\n\
            4 0 obj\n<< /V 1 /Old true >>\nendobj\n\
            4 0 obj\n<< /V 2 /New true >>\nendobj\n\
            4 1 obj\n<< /V 3 /Gen1 true >>\nendobj\n";
        let resolved_gen0 = resolve_object(pdf, b"4 0 R").unwrap();
        assert!(resolved_gen0.windows(9).any(|w| w == b"/New true"));
        let resolved_gen1 = resolve_object(pdf, b"4 1 R").unwrap();
        assert!(resolved_gen1.windows(10).any(|w| w == b"/Gen1 true"));
    }

    #[test]
    fn find_trailer_past_startxref() {
        let pdf = b"%PDF-1.4\ntrailer\n<< /Size 5 /Encrypt 2 0 R /ID [<dead><beef>] >>\nstartxref\n10\n%%EOF\n";
        let (_, trailer) = find_trailer(pdf).unwrap();
        assert_eq!(parse_dict_value(trailer, b"/Encrypt"), Some(&b"2 0 R"[..]));
        assert_eq!(parse_id_array(trailer).unwrap(), vec![0xde, 0xad]);
    }
}
