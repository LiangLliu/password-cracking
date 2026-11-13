use super::{BatchCheckResult, PasswordVerifier, PrefixBuf};
use crate::generators::CandidateBatch;
use anyhow::{Context, Result, bail};
use base64::Engine;
use cipher::{BlockCipherDecrypt, KeyInit};
use sha1::Digest as _;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

/// Office document password verifier supporting Agile Encryption (Office 2010+).
///
/// Office encrypted documents are OLE2 compound files containing an
/// `EncryptionInfo` stream with XML-based encryption parameters.
pub struct OfficeVerifier {
    enc: AgileEncryption,
}

/// Block key constants from MS-OFFCRYPTO used to derive per-purpose keys.
const BLK_KEY_VERIFIER_HASH_INPUT: [u8; 8] = [0xFE, 0xA7, 0xD2, 0x76, 0x3B, 0x4B, 0x9E, 0x79];
const BLK_KEY_ENCRYPTED_VERIFIER_HASH_VALUE: [u8; 8] =
    [0xD7, 0xAA, 0x0F, 0x6D, 0x30, 0x61, 0x34, 0x4E];

const SHA1_IV: [u32; 5] = [
    0x6745_2301,
    0xEFCD_AB89,
    0x98BA_DCFE,
    0x1032_5476,
    0xC3D2_E1F0,
];
const SHA256_IV: [u32; 8] = [
    0x6A09_E667,
    0xBB67_AE85,
    0x3C6E_F372,
    0xA54F_F53A,
    0x510E_527F,
    0x9B05_688C,
    0x1F83_D9AB,
    0x5BE0_CD19,
];
const SHA384_IV: [u64; 8] = [
    0xCBBB_9D5D_C105_9ED8,
    0x629A_292A_367C_D507,
    0x9159_015A_3070_DD17,
    0x152F_ECD8_F70E_5939,
    0x6733_2667_FFC0_0B31,
    0x8EB4_4A87_6858_1511,
    0xDB0C_2E0D_64F9_8FA7,
    0x47B5_481D_BEFA_4FA4,
];
const SHA512_IV: [u64; 8] = [
    0x6A09_E667_F3BC_C908,
    0xBB67_AE85_84CA_A73B,
    0x3C6E_F372_FE94_F82B,
    0xA54F_F53A_5F1D_36F1,
    0x510E_527F_ADE6_82D1,
    0x9B05_688C_2B3E_6C1F,
    0x1F83_D9AB_FB41_BD6B,
    0x5BE0_CD19_137E_2179,
];

/// Parsed Agile Encryption parameters.
struct AgileEncryption {
    spin_count: u32,
    key_bits: u32,
    hash_size: usize,
    hash_algorithm: HashAlgorithm,
    salt: Vec<u8>,
    encrypted_verifier_hash_input: Vec<u8>,
    encrypted_verifier_hash_value: Vec<u8>,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum HashAlgorithm {
    Sha1,
    Sha256,
    Sha384,
    Sha512,
}

impl HashAlgorithm {
    fn from_name(name: &str) -> Option<Self> {
        if name.eq_ignore_ascii_case("SHA1") {
            Some(Self::Sha1)
        } else if name.eq_ignore_ascii_case("SHA256") {
            Some(Self::Sha256)
        } else if name.eq_ignore_ascii_case("SHA384") {
            Some(Self::Sha384)
        } else if name.eq_ignore_ascii_case("SHA512") {
            Some(Self::Sha512)
        } else {
            None
        }
    }

    #[inline]
    const fn digest_len(self) -> usize {
        match self {
            Self::Sha1 => 20,
            Self::Sha256 => 32,
            Self::Sha384 => 48,
            Self::Sha512 => 64,
        }
    }

    /// Hash `data` into a fixed stack buffer, returning the valid prefix length.
    #[inline]
    fn digest_into(self, data: &[u8], out: &mut [u8; 64]) -> usize {
        match self {
            Self::Sha1 => {
                let d: [u8; 20] = sha1::Sha1::digest(data).into();
                out[..20].copy_from_slice(&d);
                20
            }
            Self::Sha256 => {
                let d: [u8; 32] = sha2::Sha256::digest(data).into();
                out[..32].copy_from_slice(&d);
                32
            }
            Self::Sha384 => {
                let d: [u8; 48] = sha2::Sha384::digest(data).into();
                out[..48].copy_from_slice(&d);
                48
            }
            Self::Sha512 => {
                let d: [u8; 64] = sha2::Sha512::digest(data).into();
                out[..64].copy_from_slice(&d);
                64
            }
        }
    }

    /// Compute `H_0 = hash(salt || utf16le(password))` directly into a stack buffer
    /// without allocating.
    fn hash_salt_and_utf16le(self, salt: &[u8], password: &[u8], out: &mut [u8; 64]) -> usize {
        fn feed_utf16<D: sha1::Digest>(hasher: &mut D, salt: &[u8], password: &[u8]) {
            hasher.update(salt);
            let mut chunk = [0u8; 64];
            let mut pos = 0usize;
            if let Ok(text) = std::str::from_utf8(password) {
                for unit in text.encode_utf16() {
                    let bytes = unit.to_le_bytes();
                    chunk[pos] = bytes[0];
                    chunk[pos + 1] = bytes[1];
                    pos += 2;
                    if pos == chunk.len() {
                        hasher.update(chunk);
                        pos = 0;
                    }
                }
            } else {
                for &b in password {
                    chunk[pos] = b;
                    chunk[pos + 1] = 0;
                    pos += 2;
                    if pos == chunk.len() {
                        hasher.update(chunk);
                        pos = 0;
                    }
                }
            }
            if pos > 0 {
                hasher.update(&chunk[..pos]);
            }
        }

        match self {
            Self::Sha1 => {
                let mut h = sha1::Sha1::new();
                feed_utf16(&mut h, salt, password);
                let d: [u8; 20] = h.finalize().into();
                out[..20].copy_from_slice(&d);
                20
            }
            Self::Sha256 => {
                let mut h = sha2::Sha256::new();
                feed_utf16(&mut h, salt, password);
                let d: [u8; 32] = h.finalize().into();
                out[..32].copy_from_slice(&d);
                32
            }
            Self::Sha384 => {
                let mut h = sha2::Sha384::new();
                feed_utf16(&mut h, salt, password);
                let d: [u8; 48] = h.finalize().into();
                out[..48].copy_from_slice(&d);
                48
            }
            Self::Sha512 => {
                let mut h = sha2::Sha512::new();
                feed_utf16(&mut h, salt, password);
                let d: [u8; 64] = h.finalize().into();
                out[..64].copy_from_slice(&d);
                64
            }
        }
    }
}

impl OfficeVerifier {
    pub fn new(path: &Path) -> Result<Self> {
        let enc = parse_encryption_info(path)?;
        Ok(Self { enc })
    }

    /// Derive the iterated hash from a password (MS-OFFCRYPTO 2.3.7.1) using zero
    /// heap allocations and direct single-block compression in the spin loop.
    ///
    /// Returns `None` if `stop` became `true` mid-spin.
    fn derive_hash_into(
        &self,
        password: &[u8],
        out: &mut [u8; 64],
        stop: Option<&AtomicBool>,
    ) -> Option<usize> {
        let h_len = self
            .enc
            .hash_algorithm
            .hash_salt_and_utf16le(&self.enc.salt, password, out);

        if self.enc.spin_count == 0 {
            return Some(h_len);
        }

        let spin_count = self.enc.spin_count;
        macro_rules! spin_blocks {
            ($block_len:literal, $digest_len:literal, $len_pos:literal, $bit_len:expr, $iv:ident, $compress:path, $words:literal) => {{
                let mut block = [0u8; $block_len];
                block[4..4 + $digest_len].copy_from_slice(&out[..$digest_len]);
                block[4 + $digest_len] = 0x80;
                block[$len_pos..$block_len].copy_from_slice(&$bit_len.to_be_bytes());
                for i in 0..spin_count {
                    if let Some(flag) = stop
                        && (i & 4095) == 0
                        && flag.load(Ordering::Relaxed)
                    {
                        return None;
                    }
                    block[0..4].copy_from_slice(&i.to_le_bytes());
                    let mut state = $iv;
                    $compress(&mut state, core::slice::from_ref(&block));
                    let word_size = $digest_len / $words;
                    for (idx, w) in state[..$words].iter().enumerate() {
                        let start = 4 + idx * word_size;
                        block[start..start + word_size].copy_from_slice(&w.to_be_bytes());
                    }
                }
                out[..$digest_len].copy_from_slice(&block[4..4 + $digest_len]);
            }};
        }

        match self.enc.hash_algorithm {
            HashAlgorithm::Sha1 => {
                spin_blocks!(64, 20, 56, 24u64 * 8, SHA1_IV, sha1::block_api::compress, 5)
            }
            HashAlgorithm::Sha256 => {
                spin_blocks!(64, 32, 56, 36u64 * 8, SHA256_IV, sha2::block_api::compress256, 8)
            }
            HashAlgorithm::Sha384 => {
                spin_blocks!(128, 48, 112, 52u128 * 8, SHA384_IV, sha2::block_api::compress512, 6)
            }
            HashAlgorithm::Sha512 => {
                spin_blocks!(128, 64, 112, 68u128 * 8, SHA512_IV, sha2::block_api::compress512, 8)
            }
        }
        Some(h_len)
    }

    /// Derive a per-purpose encryption key from the hash and a block key into a stack buffer.
    ///
    /// `key = hash(H + blockKey)[:keyBits/8]`
    #[inline]
    fn derive_key_into(&self, h: &[u8], block_key: &[u8; 8], out: &mut [u8; 32]) -> usize {
        let mut input = [0u8; 72];
        input[..h.len()].copy_from_slice(h);
        input[h.len()..h.len() + 8].copy_from_slice(block_key);

        let mut derived = [0u8; 64];
        let d_len = self
            .enc
            .hash_algorithm
            .digest_into(&input[..h.len() + 8], &mut derived);
        let key_len = ((self.enc.key_bits / 8) as usize).min(d_len).min(out.len());
        out[..key_len].copy_from_slice(&derived[..key_len]);
        key_len
    }

    /// Verify the password; returns `None` if aborted early via `stop`.
    fn verify_password(&self, password: &[u8], stop: Option<&AtomicBool>) -> Option<bool> {
        let mut h_buf = [0u8; 64];
        let h_len = self.derive_hash_into(password, &mut h_buf, stop)?;
        let h = &h_buf[..h_len];

        let mut key1 = [0u8; 32];
        let mut key2 = [0u8; 32];
        let k1_len = self.derive_key_into(h, &BLK_KEY_VERIFIER_HASH_INPUT, &mut key1);
        let k2_len = self.derive_key_into(h, &BLK_KEY_ENCRYPTED_VERIFIER_HASH_VALUE, &mut key2);

        // IV is the salt value (first 16 bytes)
        let iv = &self.enc.salt;

        // Decrypt encryptedVerifierHashInput with key1 and salt as IV
        let mut stack_in = [0u8; 128];
        let mut heap_in = Vec::new();
        let verifier_input = decrypt_into_buf(
            &key1[..k1_len],
            iv,
            &self.enc.encrypted_verifier_hash_input,
            &mut stack_in,
            &mut heap_in,
        )?;

        // Hash the decrypted verifier input
        let mut verifier_hash = [0u8; 64];
        let vh_len = self
            .enc
            .hash_algorithm
            .digest_into(verifier_input, &mut verifier_hash);

        // Decrypt encryptedVerifierHashValue with key2 and salt as IV
        let mut stack_stored = [0u8; 128];
        let mut heap_stored = Vec::new();
        let stored_hash = decrypt_into_buf(
            &key2[..k2_len],
            iv,
            &self.enc.encrypted_verifier_hash_value,
            &mut stack_stored,
            &mut heap_stored,
        )?;

        let hash_size = self.enc.hash_size.min(vh_len).min(stored_hash.len());
        Some(verifier_hash[..hash_size] == stored_hash[..hash_size])
    }
}

impl PasswordVerifier for OfficeVerifier {
    fn verify(&self, password: &[u8]) -> bool {
        self.verify_password(password, None).unwrap_or(false)
    }

    fn verify_batch(&self, batch: &CandidateBatch, stop: &AtomicBool) -> BatchCheckResult {
        let mut attempts = 0u64;
        for (idx, pw) in batch.iter().enumerate() {
            if stop.load(Ordering::Relaxed) {
                break;
            }
            match self.verify_password(pw, Some(stop)) {
                Some(true) => {
                    return BatchCheckResult {
                        matched: Some(idx),
                        attempts: attempts + 1,
                    };
                }
                Some(false) => attempts += 1,
                None => break,
            }
        }
        BatchCheckResult {
            matched: None,
            attempts,
        }
    }

    fn verify_prefix_group(
        &self,
        prefix: &[u8],
        last_bytes: &[u8],
        stop: &AtomicBool,
    ) -> BatchCheckResult {
        let mut pbuf = PrefixBuf::new(prefix);
        let mut attempts = 0u64;
        for (idx, &b) in last_bytes.iter().enumerate() {
            if stop.load(Ordering::Relaxed) {
                break;
            }
            let cand = pbuf.with_last(b);
            match self.verify_password(cand, Some(stop)) {
                Some(true) => {
                    return BatchCheckResult {
                        matched: Some(idx),
                        attempts: attempts + 1,
                    };
                }
                Some(false) => attempts += 1,
                None => break,
            }
        }
        BatchCheckResult {
            matched: None,
            attempts,
        }
    }

    fn format_name(&self) -> &'static str {
        "Office"
    }

    fn encryption_info(&self) -> &str {
        match self.enc.key_bits {
            128 => "Agile-AES128",
            192 => "Agile-AES192",
            256 => "Agile-AES256",
            _ => "Agile",
        }
    }
}

/// Decrypt `ciphertext` using AES-CBC without padding into `stack_buf` (or `heap_buf` if >128B).
#[inline]
fn decrypt_into_buf<'a>(
    key: &[u8],
    iv: &[u8],
    ciphertext: &[u8],
    stack_buf: &'a mut [u8; 128],
    heap_buf: &'a mut Vec<u8>,
) -> Option<&'a [u8]> {
    if ciphertext.len() <= stack_buf.len() {
        let out = &mut stack_buf[..ciphertext.len()];
        aes_cbc_decrypt_into(key, iv, ciphertext, out)?;
        Some(&stack_buf[..ciphertext.len()])
    } else {
        heap_buf.resize(ciphertext.len(), 0);
        aes_cbc_decrypt_into(key, iv, ciphertext, heap_buf)?;
        Some(heap_buf.as_slice())
    }
}

/// Parse the EncryptionInfo stream from an OLE2 compound file.
fn parse_encryption_info(path: &Path) -> Result<AgileEncryption> {
    let file = std::fs::File::open(path).context("Failed to open Office file")?;
    let mut comp = cfb::CompoundFile::open(file).context("Failed to parse OLE2 compound file")?;

    let mut enc_info = Vec::new();
    let mut stream = match comp.open_stream("EncryptionInfo") {
        Ok(s) => s,
        Err(e) => {
            if [
                "Workbook",
                "Book",
                "WordDocument",
                "PowerPoint Document",
                "Current User",
            ]
            .iter()
            .any(|s| comp.exists(s))
            {
                bail!(
                    "Unsupported Office encryption version: legacy Office 97-2003 format (only Office 2010+ Agile supported)"
                );
            }
            return Err(e).context("No EncryptionInfo stream found");
        }
    };
    std::io::Read::read_to_end(&mut stream, &mut enc_info)?;

    if enc_info.len() < 8 {
        bail!("EncryptionInfo too short");
    }

    let version_major = u16::from_le_bytes([enc_info[0], enc_info[1]]);
    let version_minor = u16::from_le_bytes([enc_info[2], enc_info[3]]);
    if version_major != 4 || version_minor != 4 {
        bail!(
            "Unsupported Office encryption version {version_major}.{version_minor} (only Agile 4.4 supported)"
        );
    }

    let raw_xml = std::str::from_utf8(&enc_info[8..]).context("Invalid XML in EncryptionInfo")?;
    let xml = raw_xml.trim_matches('\0').trim_start_matches('\u{FEFF}');

    // Locate `<p:encryptedKey ...>` or `<encryptedKey ...>` element tag cleanly.
    let tag_start = find_encrypted_key_tag(xml).context("No encryptedKey element in XML")?;
    let rest = &xml[tag_start..];
    let tag_end = find_tag_end(rest).context("Unterminated encryptedKey element in XML")?;
    let key_xml = &rest[..tag_end];

    parse_agile_xml(key_xml)
}

fn find_encrypted_key_tag(xml: &str) -> Option<usize> {
    for tag in ["<p:encryptedKey", "<encryptedKey"] {
        let mut offset = 0usize;
        while let Some(rel) = xml[offset..].find(tag) {
            let start = offset + rel;
            let after = start + tag.len();
            if xml.as_bytes()[after..]
                .first()
                .is_some_and(|&b| b.is_ascii_whitespace() || b == b'>' || b == b'/')
            {
                return Some(start);
            }
            offset = after;
        }
    }
    None
}

fn find_tag_end(xml: &str) -> Option<usize> {
    let bytes = xml.as_bytes();
    let mut quote: Option<u8> = None;
    let mut i = 0usize;
    while i < bytes.len() {
        let b = bytes[i];
        match quote {
            Some(q) => {
                if b == q {
                    quote = None;
                }
            }
            None => match b {
                b'"' | b'\'' => quote = Some(b),
                b'>' => return Some(i + 1),
                _ => {}
            },
        }
        i += 1;
    }
    None
}

/// Parse Agile Encryption parameters from the `<p:encryptedKey ...>` XML element.
fn parse_agile_xml(xml: &str) -> Result<AgileEncryption> {
    let spin_count: u32 = extract_attr(xml, "spinCount")?
        .parse()
        .context("Invalid spinCount")?;
    // A malicious or corrupt file could request billions of hash iterations,
    // hanging the cracker. Real Office files use 100,000.
    if spin_count > 5_000_000 {
        bail!("Unreasonable spinCount {spin_count} (limit: 5,000,000)");
    }
    let key_bits: u32 = extract_attr(xml, "keyBits")?
        .parse()
        .context("Invalid keyBits")?;
    if !matches!(key_bits, 128 | 192 | 256) {
        bail!("Unsupported AES keyBits: {key_bits} (expected 128, 192, or 256)");
    }

    let hash_size: usize = extract_attr(xml, "hashSize")?
        .parse()
        .context("Invalid hashSize")?;
    let block_size: usize = extract_attr(xml, "blockSize")?
        .parse()
        .context("Invalid blockSize")?;
    if block_size != 16 {
        bail!("Unsupported AES block size: {block_size} (expected 16)");
    }
    let hash_name = extract_attr(xml, "hashAlgorithm")?;
    let hash_algorithm = HashAlgorithm::from_name(hash_name)
        .context(format!("Unsupported hash algorithm: {hash_name}"))?;
    if hash_size == 0 || hash_size > hash_algorithm.digest_len() {
        bail!("Invalid hashSize {hash_size} for {hash_name}");
    }

    let b64 = base64::engine::general_purpose::STANDARD;
    let salt = b64
        .decode(extract_attr(xml, "saltValue")?)
        .context("Invalid saltValue base64")?;
    if salt.len() < 16 {
        bail!("Invalid Office saltValue length: {} (expected >= 16 bytes)", salt.len());
    }
    let encrypted_verifier_hash_input = b64
        .decode(extract_attr(xml, "encryptedVerifierHashInput")?)
        .context("Invalid encryptedVerifierHashInput base64")?;
    let encrypted_verifier_hash_value = b64
        .decode(extract_attr(xml, "encryptedVerifierHashValue")?)
        .context("Invalid encryptedVerifierHashValue base64")?;

    if encrypted_verifier_hash_input.is_empty()
        || !encrypted_verifier_hash_input.len().is_multiple_of(16)
    {
        bail!("Invalid encryptedVerifierHashInput length");
    }
    if encrypted_verifier_hash_value.len() < hash_size
        || !encrypted_verifier_hash_value.len().is_multiple_of(16)
    {
        bail!("Invalid encryptedVerifierHashValue length");
    }

    Ok(AgileEncryption {
        spin_count,
        key_bits,
        hash_size,
        hash_algorithm,
        salt,
        encrypted_verifier_hash_input,
        encrypted_verifier_hash_value,
    })
}

/// Extract an XML attribute value by exact name (requiring leading ASCII whitespace
/// and supporting both `"` and `'` quotes as well as optional whitespace around `=`).
fn extract_attr<'a>(xml: &'a str, attr: &str) -> Result<&'a str> {
    let bytes = xml.as_bytes();
    let mut offset = 0usize;
    while let Some(rel) = xml[offset..].find(attr) {
        let start = offset + rel;
        let after_name = start + attr.len();
        let before_ok = start > 0 && bytes[start - 1].is_ascii_whitespace();
        if before_ok {
            let mut j = after_name;
            while j < bytes.len() && bytes[j].is_ascii_whitespace() {
                j += 1;
            }
            if j < bytes.len() && bytes[j] == b'=' {
                j += 1;
                while j < bytes.len() && bytes[j].is_ascii_whitespace() {
                    j += 1;
                }
                if j < bytes.len() && (bytes[j] == b'"' || bytes[j] == b'\'') {
                    let quote = bytes[j];
                    let val_start = j + 1;
                    let val_end = bytes[val_start..]
                        .iter()
                        .position(|&b| b == quote)
                        .with_context(|| format!("Unterminated attribute {attr}"))?;
                    return Ok(&xml[val_start..val_start + val_end]);
                }
            }
        }
        offset = after_name;
    }
    bail!("Attribute {attr} not found in XML")
}

/// Raw AES-CBC decryption WITHOUT padding removal into `out` (`out.len() == ciphertext.len()`).
#[inline]
fn aes_cbc_decrypt_into(key: &[u8], iv: &[u8], ciphertext: &[u8], out: &mut [u8]) -> Option<()> {
    if ciphertext.is_empty()
        || !ciphertext.len().is_multiple_of(16)
        || out.len() != ciphertext.len()
    {
        return None;
    }
    let iv_arr: [u8; 16] = iv.get(..16)?.try_into().ok()?;

    macro_rules! cbc_decrypt_into {
        ($aes_ty:ty, $key:expr) => {{
            let cipher = <$aes_ty>::new_from_slice($key).ok()?;
            let mut prev = iv_arr;
            for (in_chunk, out_chunk) in ciphertext.chunks_exact(16).zip(out.chunks_exact_mut(16)) {
                let mut block = cipher::Block::<$aes_ty>::default();
                block.copy_from_slice(in_chunk);
                cipher.decrypt_block(&mut block);
                for i in 0..16 {
                    out_chunk[i] = block[i] ^ prev[i];
                }
                prev.copy_from_slice(in_chunk);
            }
        }};
    }

    match key.len() {
        16 => {
            let key_arr: [u8; 16] = key.try_into().ok()?;
            cbc_decrypt_into!(aes::Aes128, &key_arr);
            Some(())
        }
        24 => {
            let key_arr: [u8; 24] = key.try_into().ok()?;
            cbc_decrypt_into!(aes::Aes192, &key_arr);
            Some(())
        }
        32 => {
            let key_arr: [u8; 32] = key.try_into().ok()?;
            cbc_decrypt_into!(aes::Aes256, &key_arr);
            Some(())
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reference_derive_hash(
        alg: HashAlgorithm,
        salt: &[u8],
        password: &[u8],
        spin: u32,
    ) -> Vec<u8> {
        let pwd_utf16: Vec<u8> = match std::str::from_utf8(password) {
            Ok(text) => text.encode_utf16().flat_map(u16::to_le_bytes).collect(),
            Err(_) => password.iter().flat_map(|&b| [b, 0]).collect(),
        };
        let mut input = Vec::with_capacity(salt.len() + pwd_utf16.len());
        input.extend_from_slice(salt);
        input.extend_from_slice(&pwd_utf16);

        let digest = |buf: &[u8]| -> Vec<u8> {
            match alg {
                HashAlgorithm::Sha1 => sha1::Sha1::digest(buf).to_vec(),
                HashAlgorithm::Sha256 => sha2::Sha256::digest(buf).to_vec(),
                HashAlgorithm::Sha384 => sha2::Sha384::digest(buf).to_vec(),
                HashAlgorithm::Sha512 => sha2::Sha512::digest(buf).to_vec(),
            }
        };

        let mut h = digest(&input);
        for i in 0..spin {
            let mut buf = Vec::with_capacity(4 + h.len());
            buf.extend_from_slice(&i.to_le_bytes());
            buf.extend_from_slice(&h);
            h = digest(&buf);
        }
        h
    }

    #[test]
    fn derive_hash_block_api_matches_reference_for_all_algorithms() {
        let salt = [0x11u8; 16];
        for (alg, pw) in [
            (HashAlgorithm::Sha1, "hello".as_bytes()),
            (HashAlgorithm::Sha256, "密码92eo".as_bytes()),
            (HashAlgorithm::Sha384, &[0xFF, 0x00, b'a', b'z']),
            (
                HashAlgorithm::Sha512,
                "longer-password-exceeding-64-bytes-in-utf16le-representation!".as_bytes(),
            ),
        ] {
            let verifier = OfficeVerifier {
                enc: AgileEncryption {
                    spin_count: 17,
                    key_bits: 256,
                    hash_size: alg.digest_len(),
                    hash_algorithm: alg,
                    salt: salt.to_vec(),
                    encrypted_verifier_hash_input: vec![0; 16],
                    encrypted_verifier_hash_value: vec![0; 64],
                },
            };
            let mut got = [0u8; 64];
            let len = verifier.derive_hash_into(pw, &mut got, None).unwrap();
            let expected = reference_derive_hash(alg, &salt, pw, 17);
            assert_eq!(&got[..len], expected.as_slice(), "mismatch for {alg:?}");
        }
    }

    #[test]
    fn extract_attr_respects_boundaries_and_quotes() {
        let xml = r#"<p:encryptedKey other_spinCount="999" spinCount = '100000' keyBits="256"/>"#;
        assert_eq!(extract_attr(xml, "spinCount").unwrap(), "100000");
        assert_eq!(extract_attr(xml, "keyBits").unwrap(), "256");
        assert!(extract_attr(xml, "Bits").is_err());
    }

    #[test]
    fn legacy_ole2_stream_reports_clear_error() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        {
            let file = std::fs::OpenOptions::new()
                .read(true)
                .write(true)
                .open(tmp.path())
                .unwrap();
            let mut comp = cfb::CompoundFile::create(file).unwrap();
            comp.create_stream("WordDocument").unwrap();
        }
        let err = parse_encryption_info(tmp.path()).err().unwrap().to_string();
        assert!(err.contains("legacy Office 97-2003 format"), "unexpected error: {err}");
    }
}
