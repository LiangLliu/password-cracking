use super::PasswordVerifier;
use anyhow::{Context, Result};
use base64::Engine;
use std::path::Path;

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

/// Parsed Agile Encryption parameters.
struct AgileEncryption {
    spin_count: u32,
    key_bits: u32,
    hash_size: usize,
    #[allow(dead_code)]
    block_size: usize,
    hash_algorithm: HashAlgorithm,
    salt: Vec<u8>,
    encrypted_verifier_hash_input: Vec<u8>,
    encrypted_verifier_hash_value: Vec<u8>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum HashAlgorithm {
    Sha1,
    Sha256,
    Sha384,
    Sha512,
}

impl HashAlgorithm {
    fn from_name(name: &str) -> Option<Self> {
        match name.to_uppercase().as_str() {
            "SHA1" => Some(Self::Sha1),
            "SHA256" => Some(Self::Sha256),
            "SHA384" => Some(Self::Sha384),
            "SHA512" => Some(Self::Sha512),
            _ => None,
        }
    }

    fn digest(&self, data: &[u8]) -> Vec<u8> {
        use sha1::Digest;
        match self {
            Self::Sha1 => sha1::Sha1::digest(data).to_vec(),
            Self::Sha256 => sha2::Sha256::digest(data).to_vec(),
            Self::Sha384 => sha2::Sha384::digest(data).to_vec(),
            Self::Sha512 => sha2::Sha512::digest(data).to_vec(),
        }
    }
}

impl OfficeVerifier {
    pub fn new(path: &Path) -> Result<Self> {
        let enc = parse_encryption_info(path)?;
        Ok(Self { enc })
    }

    /// Derive the iterated hash from a password (MS-OFFCRYPTO 2.3.7.1).
    ///
    /// 1. H = hash(salt + password_utf16le)
    /// 2. For i in 0..spin_count: H = hash(i_as_4_bytes_le + H)
    fn derive_hash(&self, password: &[u8]) -> Vec<u8> {
        let mut pwd_utf16 = Vec::with_capacity(password.len() * 2);
        for &byte in password {
            pwd_utf16.push(byte);
            pwd_utf16.push(0);
        }

        // Step 1: H = hash(salt + password)
        let mut input = Vec::with_capacity(self.enc.salt.len() + pwd_utf16.len());
        input.extend_from_slice(&self.enc.salt);
        input.extend_from_slice(&pwd_utf16);
        let mut h = self.enc.hash_algorithm.digest(&input);

        // Step 2: H = hash(i_le + H) for each iteration
        for i in 0..self.enc.spin_count {
            let mut buf = Vec::with_capacity(4 + h.len());
            buf.extend_from_slice(&i.to_le_bytes());
            buf.extend_from_slice(&h);
            h = self.enc.hash_algorithm.digest(&buf);
        }
        h
    }

    /// Derive a per-purpose encryption key from the hash and a block key.
    ///
    /// key = hash(H + blockKey)[:keyBits/8]
    fn derive_key(&self, h: &[u8], block_key: &[u8]) -> Vec<u8> {
        let mut buf = Vec::with_capacity(h.len() + block_key.len());
        buf.extend_from_slice(h);
        buf.extend_from_slice(block_key);
        let derived = self.enc.hash_algorithm.digest(&buf);
        let key_len = (self.enc.key_bits / 8) as usize;
        derived[..key_len.min(derived.len())].to_vec()
    }

    /// Verify the password by decrypting and comparing the verifier hash.
    fn verify_password(&self, password: &[u8]) -> bool {
        let h = self.derive_hash(password);

        // Derive two keys: one for verifier input, one for verifier hash value
        let key1 = self.derive_key(&h, &BLK_KEY_VERIFIER_HASH_INPUT);
        let key2 = self.derive_key(&h, &BLK_KEY_ENCRYPTED_VERIFIER_HASH_VALUE);

        // IV is the salt value (not zeros!)
        let iv = &self.enc.salt;

        // Decrypt encryptedVerifierHashInput with key1 and salt as IV
        let verifier_input =
            match aes_cbc_decrypt_raw(&key1, iv, &self.enc.encrypted_verifier_hash_input) {
                Some(v) => v,
                None => return false,
            };

        // Hash the decrypted verifier input
        let verifier_hash = self.enc.hash_algorithm.digest(&verifier_input);

        // Decrypt encryptedVerifierHashValue with key2 and salt as IV
        let stored_hash =
            match aes_cbc_decrypt_raw(&key2, iv, &self.enc.encrypted_verifier_hash_value) {
                Some(v) => v,
                None => return false,
            };

        // Compare the computed hash with the decrypted stored hash
        let hash_size = self.enc.hash_size;
        let computed = &verifier_hash[..hash_size.min(verifier_hash.len())];
        let stored = &stored_hash[..hash_size.min(stored_hash.len())];
        computed == stored
    }
}

impl PasswordVerifier for OfficeVerifier {
    fn quick_check(&self, password: &[u8]) -> bool {
        self.verify_password(password)
    }

    fn verify(&self, password: &[u8]) -> bool {
        self.verify_password(password)
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

/// Parse the EncryptionInfo stream from an OLE2 compound file.
fn parse_encryption_info(path: &Path) -> Result<AgileEncryption> {
    let file = std::fs::File::open(path).context("Failed to open Office file")?;
    let mut comp = cfb::CompoundFile::open(file).context("Failed to parse OLE2 compound file")?;

    let mut enc_info = Vec::new();
    let mut stream = comp
        .open_stream("EncryptionInfo")
        .context("No EncryptionInfo stream found")?;
    std::io::Read::read_to_end(&mut stream, &mut enc_info)?;

    if enc_info.len() < 8 {
        anyhow::bail!("EncryptionInfo too short");
    }

    let version = u16::from_le_bytes([enc_info[0], enc_info[1]]);
    if version != 4 {
        anyhow::bail!(
            "Unsupported Office encryption version: {version} (only Agile/v4 supported)"
        );
    }

    let xml = std::str::from_utf8(&enc_info[8..]).context("Invalid XML in EncryptionInfo")?;

    // Parse from the p:encryptedKey element, which contains the password verifier.
    // We find it by looking for encryptedVerifierHashInput which is unique to that element.
    let key_element_start = xml
        .find("encryptedKey")
        .context("No encryptedKey element in XML")?;
    let key_xml = &xml[key_element_start..];

    parse_agile_xml(key_xml)
}

/// Parse Agile Encryption parameters from the p:encryptedKey XML element.
fn parse_agile_xml(xml: &str) -> Result<AgileEncryption> {
    let spin_count = extract_attr(xml, "spinCount")?
        .parse()
        .context("Invalid spinCount")?;
    let key_bits = extract_attr(xml, "keyBits")?
        .parse()
        .context("Invalid keyBits")?;
    let hash_size: usize = extract_attr(xml, "hashSize")?
        .parse()
        .context("Invalid hashSize")?;
    let block_size: usize = extract_attr(xml, "blockSize")?
        .parse()
        .context("Invalid blockSize")?;
    let hash_name = extract_attr(xml, "hashAlgorithm")?;
    let hash_algorithm = HashAlgorithm::from_name(&hash_name)
        .context(format!("Unsupported hash algorithm: {hash_name}"))?;

    let b64 = base64::engine::general_purpose::STANDARD;
    let salt = b64
        .decode(extract_attr(xml, "saltValue")?)
        .context("Invalid saltValue base64")?;
    let encrypted_verifier_hash_input = b64
        .decode(extract_attr(xml, "encryptedVerifierHashInput")?)
        .context("Invalid encryptedVerifierHashInput base64")?;
    let encrypted_verifier_hash_value = b64
        .decode(extract_attr(xml, "encryptedVerifierHashValue")?)
        .context("Invalid encryptedVerifierHashValue base64")?;

    Ok(AgileEncryption {
        spin_count,
        key_bits,
        hash_size,
        block_size,
        hash_algorithm,
        salt,
        encrypted_verifier_hash_input,
        encrypted_verifier_hash_value,
    })
}

/// Extract an XML attribute value by name from a string.
fn extract_attr(xml: &str, attr: &str) -> Result<String> {
    let pattern = format!("{attr}=\"");
    let start = xml
        .find(&pattern)
        .context(format!("Attribute {attr} not found in XML"))?;
    let value_start = start + pattern.len();
    let value_end = xml[value_start..]
        .find('"')
        .context(format!("Unterminated attribute {attr}"))?;
    Ok(xml[value_start..value_start + value_end].to_string())
}

/// Raw AES-CBC decryption WITHOUT padding removal.
fn aes_cbc_decrypt_raw(key: &[u8], iv: &[u8], ciphertext: &[u8]) -> Option<Vec<u8>> {
    use cipher::{BlockCipherDecrypt, KeyInit};

    if ciphertext.is_empty() || !ciphertext.len().is_multiple_of(16) {
        return None;
    }
    let iv_arr: [u8; 16] = iv.get(..16)?.try_into().ok()?;

    type Aes128 = aes::Aes128;
    type Aes192 = aes::Aes192;
    type Aes256 = aes::Aes256;

    macro_rules! cbc_decrypt {
        ($aes_ty:ty, $key:expr) => {{
            let cipher = <$aes_ty>::new_from_slice($key).ok()?;
            let mut result = Vec::with_capacity(ciphertext.len());
            let mut prev = iv_arr;
            for chunk in ciphertext.chunks(16) {
                let mut block = cipher::Block::<$aes_ty>::default();
                block.copy_from_slice(chunk);
                let mut saved = [0u8; 16];
                saved.copy_from_slice(&block[..]);
                cipher.decrypt_block(&mut block);
                for i in 0..16 {
                    block[i] ^= prev[i];
                }
                result.extend_from_slice(&block[..]);
                prev = saved;
            }
            result
        }};
    }

    match key.len() {
        16 => {
            let key_arr: [u8; 16] = key.try_into().ok()?;
            Some(cbc_decrypt!(Aes128, &key_arr))
        }
        24 => {
            let key_arr: [u8; 24] = key.try_into().ok()?;
            Some(cbc_decrypt!(Aes192, &key_arr))
        }
        32 => {
            let key_arr: [u8; 32] = key.try_into().ok()?;
            Some(cbc_decrypt!(Aes256, &key_arr))
        }
        _ => None,
    }
}
