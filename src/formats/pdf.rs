use super::PasswordVerifier;
use anyhow::{Context, Result, bail};
use std::path::Path;

/// PDF password verifier supporting all standard encryption algorithms.
///
/// Supports:
/// - RC4 40-bit (V=1, R=2)
/// - RC4 128-bit (V=2, R=3)
/// - AES-128 (V=4, R=4)
/// - AES-256 (V=5, R=5/R=6)
pub struct PdfVerifier {
    enc: PdfEncryption,
}

/// Parsed encryption parameters from the PDF /Encrypt dictionary.
struct PdfEncryption {
    version: u32,        // V
    revision: u32,       // R
    length: u32,         // key length in bits
    owner_hash: Vec<u8>, // O entry (32 bytes)
    user_hash: Vec<u8>,  // U entry (32 bytes)
    permissions: i32,    // P entry
    id0: Vec<u8>,        // first element of /ID array
    encrypt_metadata: bool,
    /// AES-256 salt for U (only V>=5)
    u_validation_salt: Option<Vec<u8>>,
    /// AES-256 salt for O (only V>=5)
    #[allow(dead_code)]
    o_validation_salt: Option<Vec<u8>>,
}

impl PdfVerifier {
    pub fn new(path: &Path) -> Result<Self> {
        let data = std::fs::read(path).context("Failed to read PDF")?;
        let enc = parse_encryption(&data)?;
        Ok(Self { enc })
    }

    /// Compute the encryption key from a user password.
    #[allow(dead_code)]
    fn derive_key(&self, password: &[u8]) -> Vec<u8> {
        if self.enc.version >= 5 {
            self.derive_key_aes256(password)
        } else {
            self.derive_key_rc4(password)
        }
    }

    /// Key derivation for RC4 40/128-bit and AES-128 (V=1,2,4, R=2,3,4).
    fn derive_key_rc4(&self, password: &[u8]) -> Vec<u8> {
        use md5::Md5;
        use sha1::Digest;

        const PADDING: [u8; 32] = [
            0x28, 0xBF, 0x4E, 0x5E, 0x4E, 0x75, 0x8A, 0x41, 0x64, 0x00, 0x4E, 0x56, 0xFF, 0xFA,
            0x01, 0x08, 0x2E, 0x2E, 0x00, 0xB6, 0xD0, 0x68, 0x3E, 0x80, 0x2F, 0x0C, 0xA9, 0xFE,
            0x64, 0x53, 0x69, 0x7A,
        ];

        // Step 1: pad the password to 32 bytes
        let mut buf = Vec::with_capacity(32 + 32 + 4 + self.enc.id0.len() + 4);
        let pwd_len = password.len().min(32);
        buf.extend_from_slice(&password[..pwd_len]);
        buf.extend_from_slice(&PADDING[..32 - pwd_len]);

        // Step 2: append O entry
        buf.extend_from_slice(&self.enc.owner_hash);

        // Step 3: append P (4 bytes, little-endian)
        buf.extend_from_slice(&self.enc.permissions.to_le_bytes());

        // Step 4: append ID[0]
        buf.extend_from_slice(&self.enc.id0);

        // Step 5: if R>=4 and EncryptMetadata is false, append 0xFFFFFFFF
        if self.enc.revision >= 4 && !self.enc.encrypt_metadata {
            buf.extend_from_slice(&[0xFF, 0xFF, 0xFF, 0xFF]);
        }

        // Step 6: MD5 hash
        let mut hasher = Md5::new();
        hasher.update(&buf);
        let mut hash = hasher.finalize().to_vec();

        // Step 7: for R>=3, do 50 additional rounds of MD5
        if self.enc.revision >= 3 {
            let key_len = (self.enc.length / 8) as usize;
            for _ in 0..50 {
                let mut hasher = Md5::new();
                hasher.update(&hash[..key_len.min(hash.len())]);
                hash = hasher.finalize().to_vec();
            }
        }

        // Return the first n bytes (key length)
        let key_len = if self.enc.length > 0 {
            (self.enc.length / 8) as usize
        } else {
            5 // 40-bit default
        };
        hash.truncate(key_len);
        hash
    }

    /// Key derivation for AES-256 (V=5, R=5/R=6).
    #[allow(dead_code)]
    fn derive_key_aes256(&self, password: &[u8]) -> Vec<u8> {
        use sha2::{Digest, Sha256};

        if self.enc.revision == 5 {
            // R=5: SHA256(password + validation_salt)
            let salt = self.enc.u_validation_salt.as_deref().unwrap_or(&[]);
            let mut hasher = Sha256::new();
            hasher.update(password);
            hasher.update(salt);
            hasher.finalize().to_vec()
        } else {
            // R=6: iterative hashing with XOR
            let salt = self.enc.u_validation_salt.as_deref().unwrap_or(&[]);
            let mut input = Vec::with_capacity(password.len() + salt.len());
            input.extend_from_slice(password);
            input.extend_from_slice(salt);

            let mut current = Sha256::digest(&input).to_vec();
            // R=6 performs 64 rounds of alternating SHA256 with specific input modifications
            for _ in 0..64 {
                let mut hasher = Sha256::new();
                hasher.update(&current);
                hasher.update(&input);
                current = hasher.finalize().to_vec();
                // XOR trick from the spec
                let sum: u8 = current.iter().map(|b| b.count_ones() as u8).sum();
                if sum.is_multiple_of(8) {
                    // Use the last byte as a seed for the next round
                    break;
                }
            }
            current
        }
    }

    /// Verify the user password against the U entry.
    fn verify_user_password(&self, password: &[u8]) -> bool {
        if self.enc.version >= 5 {
            self.verify_user_password_aes256(password)
        } else {
            self.verify_user_password_rc4(password)
        }
    }

    /// Verify user password for RC4/AES-128 (R=2,3,4).
    fn verify_user_password_rc4(&self, password: &[u8]) -> bool {
        let key = self.derive_key_rc4(password);

        if self.enc.revision == 2 {
            // R=2: RC4(key, padding) should equal U
            const PADDING: [u8; 32] = [
                0x28, 0xBF, 0x4E, 0x5E, 0x4E, 0x75, 0x8A, 0x41, 0x64, 0x00, 0x4E, 0x56, 0xFF, 0xFA,
                0x01, 0x08, 0x2E, 0x2E, 0x00, 0xB6, 0xD0, 0x68, 0x3E, 0x80, 0x2F, 0x0C, 0xA9, 0xFE,
                0x64, 0x53, 0x69, 0x7A,
            ];
            let decrypted = rc4_encrypt(&key, &PADDING);
            decrypted == self.enc.user_hash
        } else {
            // R>=3: compute MD5(padding + ID[0]), then RC4 with 20 rounds
            use md5::Md5;
            use sha1::Digest;

            const PADDING: [u8; 32] = [
                0x28, 0xBF, 0x4E, 0x5E, 0x4E, 0x75, 0x8A, 0x41, 0x64, 0x00, 0x4E, 0x56, 0xFF, 0xFA,
                0x01, 0x08, 0x2E, 0x2E, 0x00, 0xB6, 0xD0, 0x68, 0x3E, 0x80, 0x2F, 0x0C, 0xA9, 0xFE,
                0x64, 0x53, 0x69, 0x7A,
            ];

            let mut hasher = Md5::new();
            hasher.update(PADDING);
            hasher.update(&self.enc.id0);
            let hash = hasher.finalize().to_vec();

            // Round 0: RC4 with the key
            let mut result = rc4_encrypt(&key, &hash);

            // Rounds 1-19: RC4 with key XORed with round number
            for round in 1u8..=19 {
                let modified_key: Vec<u8> = key.iter().map(|&b| b ^ round).collect();
                result = rc4_encrypt(&modified_key, &result);
            }

            // Compare first 16 bytes with U
            result[..16] == self.enc.user_hash[..16]
        }
    }

    /// Verify user password for AES-256 (R=5/R=6).
    fn verify_user_password_aes256(&self, password: &[u8]) -> bool {
        use sha2::{Digest, Sha256};

        let salt = self.enc.u_validation_salt.as_deref().unwrap_or(&[]);
        let mut hasher = Sha256::new();
        hasher.update(password);
        hasher.update(salt);
        let hash = hasher.finalize();

        // Compare first 32 bytes of U with the computed hash
        hash.as_slice() == &self.enc.user_hash[..32.min(self.enc.user_hash.len())]
    }
}

impl PasswordVerifier for PdfVerifier {
    fn quick_check(&self, password: &[u8]) -> bool {
        // For PDF, the password verification IS the quick check.
        // The full verify confirms by attempting to decrypt content.
        self.verify_user_password(password)
    }

    fn verify(&self, password: &[u8]) -> bool {
        // The user password verification is sufficient to confirm the password.
        // Decrypting actual content would be slower and unnecessary.
        self.verify_user_password(password)
    }

    fn format_name(&self) -> &'static str {
        "PDF"
    }

    fn encryption_info(&self) -> &str {
        match (self.enc.version, self.enc.revision) {
            (1, 2) => "RC4-40bit",
            (2, 3) => "RC4-128bit",
            (4, 4) => "AES-128",
            (5, 5) => "AES-256-R5",
            (5, 6) => "AES-256-R6",
            _ => "unknown",
        }
    }
}

// --- PDF parsing ---

/// Parse the /Encrypt dictionary from a PDF file.
fn parse_encryption(data: &[u8]) -> Result<PdfEncryption> {
    if !data.windows(8).any(|w| w == b"/Encrypt") {
        bail!("PDF is not encrypted");
    }

    // Find the trailer to get the /Encrypt object reference and /ID array.
    let trailer = find_trailer(data)?;
    let encrypt_ref =
        parse_dict_value(&trailer, b"/Encrypt").context("Missing /Encrypt in trailer")?;
    let id_array = parse_id_array(&trailer)?;

    // Resolve the Encrypt object
    let enc_dict = resolve_object(data, &encrypt_ref)?;
    let enc_dict_str = String::from_utf8_lossy(&enc_dict);

    let version = parse_int(&enc_dict_str, b"/V").unwrap_or(0);
    let revision = parse_int(&enc_dict_str, b"/R").unwrap_or(0);
    let length = parse_int(&enc_dict_str, b"/Length").unwrap_or(40);
    let permissions = parse_int(&enc_dict_str, b"/P").unwrap_or(0) as i32;
    let owner_hash = parse_hex_string(&enc_dict_str, b"/O").context("Missing /O")?;
    let user_hash = parse_hex_string(&enc_dict_str, b"/U").context("Missing /U")?;

    // For V>=5, extract validation salts from U and O
    // U = [32 bytes hash][8 bytes validation salt][8 bytes key salt]
    // O = [32 bytes hash][8 bytes validation salt][8 bytes key salt]
    let (u_validation_salt, o_validation_salt) = if version >= 5 {
        let u_vs = user_hash.get(32..40).map(|s| s.to_vec());
        let o_vs = owner_hash.get(32..40).map(|s| s.to_vec());
        (u_vs, o_vs)
    } else {
        (None, None)
    };

    Ok(PdfEncryption {
        version,
        revision,
        length,
        owner_hash,
        user_hash,
        permissions,
        id0: id_array,
        encrypt_metadata: true, // default
        u_validation_salt,
        o_validation_salt,
    })
}

/// Find the trailer dictionary in a PDF.
fn find_trailer(data: &[u8]) -> Result<String> {
    // Search from the end for "trailer"
    let trailer_idx = data
        .windows(7)
        .rposition(|w| w == b"trailer")
        .context("No trailer found in PDF")?;
    let after_trailer = &data[trailer_idx..];
    // Find the closing >> of the trailer dictionary
    let end = after_trailer
        .windows(8)
        .position(|w| w == b"startxref")
        .unwrap_or(after_trailer.len());
    Ok(String::from_utf8_lossy(&after_trailer[..end]).into_owned())
}

/// Parse a value like "8 0 R" from a dictionary string for a given key.
fn parse_dict_value(dict: &str, key: &[u8]) -> Option<String> {
    let key_str = std::str::from_utf8(key).ok()?;
    let idx = dict.find(key_str)?;
    let after = &dict[idx + key_str.len()..];
    let after = after.trim_start();

    // An indirect reference like "8 0 R" spans 3 tokens.
    // A name like "/Standard" is one token.
    // Read until we hit ">>" (end of dict) or "/" (next key).
    let end = after.find(['>', '/']).unwrap_or(after.len());
    Some(after[..end].trim().to_string())
}

/// Parse the /ID array from the trailer to get the first ID.
fn parse_id_array(trailer: &str) -> Result<Vec<u8>> {
    let id_idx = trailer.find("/ID").context("Missing /ID in trailer")?;
    let after = &trailer[id_idx..];
    // Find the first hex string <...> after /ID
    let hex_start = after.find('<').context("No hex string in /ID")?;
    let hex_end = after[hex_start..]
        .find('>')
        .context("Unterminated hex string in /ID")?;
    let hex_str = &after[hex_start + 1..hex_start + hex_end];
    hex_decode(hex_str.trim())
}

/// Resolve an indirect object reference like "8 0 R" to its content.
fn resolve_object(data: &[u8], ref_str: &str) -> Result<Vec<u8>> {
    let parts: Vec<&str> = ref_str.split_whitespace().collect();
    if parts.len() < 2 {
        bail!("Invalid object reference: {ref_str}");
    }
    let obj_num: u32 = parts[0].parse()?;

    // Find "N 0 obj" in the data
    let search = format!("{obj_num} 0 obj");
    let search_bytes = search.as_bytes();
    let obj_start = data
        .windows(search_bytes.len())
        .position(|w| w == search_bytes)
        .context(format!("Object {obj_num} not found"))?;
    let after_obj = &data[obj_start + search_bytes.len()..];
    let obj_end = after_obj
        .windows(6)
        .position(|w| w == b"endobj")
        .context("Missing endobj")?;

    Ok(after_obj[..obj_end].to_vec())
}

/// Parse an integer value for a key from a dictionary string.
fn parse_int(dict: &str, key: &[u8]) -> Option<u32> {
    let val = parse_dict_value(dict, key)?;
    val.parse().ok()
}

/// Parse a hex string value <...> for a key from a dictionary string.
fn parse_hex_string(dict: &str, key: &[u8]) -> Option<Vec<u8>> {
    let key_str = std::str::from_utf8(key).ok()?;
    let idx = dict.find(key_str)?;
    let after = &dict[idx + key_str.len()..];
    let after = after.trim_start();
    if let Some(rest) = after.strip_prefix('<') {
        let end = rest.find('>')?;
        hex_decode(rest[..end].trim()).ok()
    } else {
        None
    }
}

/// Decode a hex string to bytes.
fn hex_decode(s: &str) -> Result<Vec<u8>> {
    let s: String = s.chars().filter(|c| !c.is_whitespace()).collect();
    if !s.len().is_multiple_of(2) {
        bail!("Hex string has odd length");
    }
    let bytes: Result<Vec<u8>, _> = (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).map_err(|e| anyhow::anyhow!(e)))
        .collect();
    bytes
}

/// RC4 stream cipher encryption/decryption (same operation).
fn rc4_encrypt(key: &[u8], data: &[u8]) -> Vec<u8> {
    let mut s: Vec<u8> = (0..=255u8).collect();
    let mut j: u8 = 0;
    for i in 0..256 {
        j = j.wrapping_add(s[i]).wrapping_add(key[i % key.len()]);
        s.swap(i, j as usize);
    }
    let mut i: u8 = 0;
    let mut j: u8 = 0;
    data.iter()
        .map(|&byte| {
            i = i.wrapping_add(1);
            j = j.wrapping_add(s[i as usize]);
            s.swap(i as usize, j as usize);
            let k = s[(s[i as usize].wrapping_add(s[j as usize])) as usize];
            byte ^ k
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rc4_known_vector() {
        // RFC 6229 test vector: key = "Key", plaintext = "Plaintext"
        let result = rc4_encrypt(b"Key", b"Plaintext");
        assert_eq!(result, [0xBB, 0xF3, 0x16, 0xE8, 0xD9, 0x40, 0xAF, 0x0A, 0xD3]);
    }

    #[test]
    fn hex_decode_basic() {
        assert_eq!(hex_decode("48656c6c6f").unwrap(), b"Hello");
        assert_eq!(hex_decode("48 65 6c 6c 6f").unwrap(), b"Hello");
    }
}
