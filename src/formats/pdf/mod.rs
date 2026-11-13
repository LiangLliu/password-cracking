mod crypto;
mod parser;

use super::{BatchCheckResult, PasswordVerifier, PrefixBuf};
use crate::generators::CandidateBatch;
use anyhow::{Context, Result, bail};
use crypto::{
    MD5_IV, PADDING, RC4_INIT, U32x4, algorithm_2b, algorithm_2b_with_work, bytes_to_words,
    md5_compress_1x, md5_compress_4x, pad_password_stack, pad_short_md5_block, rc4_crypt_in_place,
    rc4_matches_prefix, words_to_bytes,
};
use md5::{Digest as _, Md5};
use memmap2::Mmap;
use parser::{PdfEncryption, parse_encryption};
use sha2::Sha256;
use std::fs::File;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

/// PDF password verifier supporting Standard Security Handler revisions 2–6.
pub struct PdfVerifier {
    enc: PdfEncryption,
    try_owner: bool,
}

impl PdfVerifier {
    pub fn new(path: &Path, try_owner: bool) -> Result<Self> {
        let file = File::open(path).context("Failed to read PDF")?;
        let file_len = file.metadata().map(|m| m.len()).unwrap_or(0);
        if file_len == 0 {
            bail!("PDF file is empty");
        }
        let enc = match unsafe { Mmap::map(&file) } {
            Ok(mmap) => parse_encryption(&mmap)?,
            Err(_) => {
                drop(file);
                let data = std::fs::read(path).context("Failed to read PDF")?;
                parse_encryption(&data)?
            }
        };
        Ok(Self { enc, try_owner })
    }

    /// Key derivation for RC4 40/128-bit and AES-128 (V=1,2,4, R=2,3,4).
    /// Returns `(key_buffer, key_len)`.
    #[inline]
    fn derive_key_rc4_stack(&self, password: &[u8]) -> ([u8; 16], usize) {
        let padded = pad_password_stack(password);
        let key_len = self.enc.key_len;

        // First MD5 over `padded (32) || O (32) || P || ID[0] || optional FFFFFFFF`.
        let mut state = MD5_IV;
        let mut block0 = [0u32; 16];
        for i in 0..8 {
            block0[i] = u32::from_le_bytes(padded[i * 4..i * 4 + 4].try_into().unwrap());
        }
        block0[8..16].copy_from_slice(&self.enc.block0_o_words);
        md5_compress_1x(&mut state, &block0);
        for tb in &self.enc.derive_tail_blocks {
            md5_compress_1x(&mut state, tb);
        }

        // For R>=3, 50 additional MD5 rounds over the first `key_len` bytes of the digest.
        if self.enc.revision >= 3 {
            let mut next = [0u32; 16];
            let bit_len = (key_len as u32) * 8;
            if key_len == 16 {
                next[4] = 0x0000_0080;
                next[14] = bit_len;
                for _ in 0..50 {
                    next[0] = state[0];
                    next[1] = state[1];
                    next[2] = state[2];
                    next[3] = state[3];
                    state = MD5_IV;
                    md5_compress_1x(&mut state, &next);
                }
            } else {
                for _ in 0..50 {
                    let digest = words_to_bytes(&state);
                    next = pad_short_md5_block(&digest[..key_len]);
                    state = MD5_IV;
                    md5_compress_1x(&mut state, &next);
                }
            }
        }

        (words_to_bytes(&state), key_len)
    }

    /// Key derivation for 4 candidates simultaneously in 128-bit SIMD lanes.
    #[inline]
    fn derive_key_rc4_4x(&self, passwords: [&[u8]; 4]) -> ([[u8; 16]; 4], usize) {
        let mut state = [
            U32x4::splat(MD5_IV[0]),
            U32x4::splat(MD5_IV[1]),
            U32x4::splat(MD5_IV[2]),
            U32x4::splat(MD5_IV[3]),
        ];

        let p0 = pad_password_stack(passwords[0]);
        let p1 = pad_password_stack(passwords[1]);
        let p2 = pad_password_stack(passwords[2]);
        let p3 = pad_password_stack(passwords[3]);

        let mut block0 = [U32x4::splat(0); 16];
        for (i, slot) in block0[..8].iter_mut().enumerate() {
            let s = i * 4;
            *slot = U32x4([
                u32::from_le_bytes(p0[s..s + 4].try_into().unwrap()),
                u32::from_le_bytes(p1[s..s + 4].try_into().unwrap()),
                u32::from_le_bytes(p2[s..s + 4].try_into().unwrap()),
                u32::from_le_bytes(p3[s..s + 4].try_into().unwrap()),
            ]);
        }
        for (slot, &word) in block0[8..16].iter_mut().zip(self.enc.block0_o_words.iter()) {
            *slot = U32x4::splat(word);
        }
        md5_compress_4x(&mut state, &block0);

        for tb in &self.enc.derive_tail_blocks {
            let mut block = [U32x4::splat(0); 16];
            for i in 0..16 {
                block[i] = U32x4::splat(tb[i]);
            }
            md5_compress_4x(&mut state, &block);
        }

        let key_len = self.enc.key_len;
        if self.enc.revision >= 3 {
            let mut next = [U32x4::splat(0); 16];
            next[14] = U32x4::splat((key_len as u32) * 8);
            if key_len == 16 {
                next[4] = U32x4::splat(0x0000_0080);
                for _ in 0..50 {
                    next[0] = state[0];
                    next[1] = state[1];
                    next[2] = state[2];
                    next[3] = state[3];
                    state = [
                        U32x4::splat(MD5_IV[0]),
                        U32x4::splat(MD5_IV[1]),
                        U32x4::splat(MD5_IV[2]),
                        U32x4::splat(MD5_IV[3]),
                    ];
                    md5_compress_4x(&mut state, &next);
                }
            } else {
                let full_words = key_len / 4;
                let rem = key_len % 4;
                let keep_mask = if rem == 0 {
                    0u32
                } else {
                    (1u32 << (rem * 8)) - 1
                };
                let pad_bit = 0x80u32 << (rem * 8);
                for _ in 0..50 {
                    next[0] = state[0];
                    next[1] = state[1];
                    next[2] = state[2];
                    next[3] = state[3];
                    if full_words < 4 {
                        next[full_words] = U32x4([
                            (next[full_words].0[0] & keep_mask) | pad_bit,
                            (next[full_words].0[1] & keep_mask) | pad_bit,
                            (next[full_words].0[2] & keep_mask) | pad_bit,
                            (next[full_words].0[3] & keep_mask) | pad_bit,
                        ]);
                        next[full_words + 1..4].fill(U32x4::splat(0));
                    }
                    state = [
                        U32x4::splat(MD5_IV[0]),
                        U32x4::splat(MD5_IV[1]),
                        U32x4::splat(MD5_IV[2]),
                        U32x4::splat(MD5_IV[3]),
                    ];
                    md5_compress_4x(&mut state, &next);
                }
            }
        }

        let mut keys = [[0u8; 16]; 4];
        for (lane, key) in keys.iter_mut().enumerate() {
            for (word, dst) in state[..4].iter().zip(key.as_chunks_mut::<4>().0) {
                *dst = word.0[lane].to_le_bytes();
            }
        }
        (keys, key_len)
    }

    /// Verify a password, accepting either the user or (if enabled) owner password.
    fn verify_password(&self, password: &[u8]) -> bool {
        if self.verify_user_password(password) {
            return true;
        }
        self.try_owner && self.verify_owner_password(password)
    }

    /// Verify the user password against the U entry.
    fn verify_user_password(&self, password: &[u8]) -> bool {
        if self.enc.version >= 5 {
            self.verify_user_password_aes256(password)
        } else {
            self.verify_user_password_rc4(password)
        }
    }

    /// Verify the owner password against the O entry.
    ///
    /// For R<=4 the owner key decrypts O to recover the padded user password,
    /// which is then verified normally. For R>=5 the owner hash is computed
    /// directly from the password, the O validation salt, and the full U entry.
    fn verify_owner_password(&self, password: &[u8]) -> bool {
        if self.enc.version >= 5 {
            self.verify_owner_password_aes256(password)
        } else {
            let recovered = self.recover_user_password(password);
            self.verify_user_password_rc4(&recovered)
        }
    }

    /// Algorithm 3: recover the padded user password from the O entry.
    fn recover_user_password(&self, owner_password: &[u8]) -> [u8; 32] {
        // Steps (a)-(d): derive the owner key (MD5 always over 16 bytes for R>=3).
        let padded = pad_password_stack(owner_password);
        let mut digest: [u8; 16] = Md5::digest(padded).into();
        if self.enc.revision >= 3 {
            let mut next = [0u32; 16];
            next[4] = 0x0000_0080;
            next[14] = 16 * 8;
            let mut state = bytes_to_words(&digest);
            for _ in 0..50 {
                next[0..4].copy_from_slice(&state);
                state = MD5_IV;
                md5_compress_1x(&mut state, &next);
            }
            digest = words_to_bytes(&state);
        }
        let key = &digest[..self.enc.key_len];

        // Steps (e)-(h): decrypt O in-place on stack.
        let mut out = [0u8; 32];
        out.copy_from_slice(&self.enc.owner_hash[..32]);
        if self.enc.revision == 2 {
            rc4_crypt_in_place(key, &mut out);
        } else {
            let mut mod_key = [0u8; 16];
            for i in (0..=19u8).rev() {
                for k in 0..key.len() {
                    mod_key[k] = key[k] ^ i;
                }
                rc4_crypt_in_place(&mod_key[..key.len()], &mut out);
            }
        }
        out
    }

    /// Verify the owner password for AES-256 (R=5/R=6).
    fn verify_owner_password_aes256(&self, password: &[u8]) -> bool {
        let password = &password[..password.len().min(127)];
        let salt = self.enc.o_validation_salt.as_ref().map_or(&[][..], |s| s);
        // For owner authentication the first 48 bytes of the U entry are the extra data.
        let udata = &self.enc.user_hash[..self.enc.user_hash.len().min(48)];
        let computed = if self.enc.revision >= 6 {
            algorithm_2b(password, salt, udata)
        } else {
            let mut hasher = Sha256::new();
            hasher.update(password);
            hasher.update(salt);
            hasher.update(udata);
            let out: [u8; 32] = hasher.finalize().into();
            out
        };
        computed[..] == self.enc.owner_hash[..32]
    }

    /// Verify user password for RC4/AES-128 (R=2,3,4).
    #[inline]
    fn verify_user_password_rc4(&self, password: &[u8]) -> bool {
        let (key, key_len) = self.derive_key_rc4_stack(password);
        self.verify_user_rc4_with_key(&key[..key_len])
    }

    /// Checks whether a derived key matches the stored `/U` entry via RC4.
    #[inline]
    fn verify_user_rc4_with_key(&self, key: &[u8]) -> bool {
        if self.enc.revision == 2 {
            rc4_matches_prefix(key, &PADDING, &self.enc.user_hash[..32])
        } else {
            let mut result = self.enc.r3_target;
            let mut mod_key = [0u8; 16];
            rc4_crypt_in_place(key, &mut result);
            for round in 1u8..=18 {
                for k in 0..key.len() {
                    mod_key[k] = key[k] ^ round;
                }
                rc4_crypt_in_place(&mod_key[..key.len()], &mut result);
            }
            for k in 0..key.len() {
                mod_key[k] = key[k] ^ 19;
            }
            rc4_matches_prefix(&mod_key[..key.len()], &result, &self.enc.user_hash[..16])
        }
    }

    /// 4-way interleaved RC4 check hiding S-box load/swap latency across 4 independent keys.
    #[inline]
    fn verify_user_rc4_4x(&self, keys: &[[u8; 16]; 4], key_len: usize) -> Option<usize> {
        if self.enc.revision == 2 {
            for (lane, key) in keys.iter().enumerate() {
                if rc4_matches_prefix(&key[..key_len], &PADDING, &self.enc.user_hash[..32]) {
                    return Some(lane);
                }
            }
            return None;
        }

        let mut bufs = [self.enc.r3_target; 4];
        let mut s0;
        let mut s1;
        let mut s2;
        let mut s3;
        let mut mk0 = [0u8; 16];
        let mut mk1 = [0u8; 16];
        let mut mk2 = [0u8; 16];
        let mut mk3 = [0u8; 16];

        for round in 0u8..=18 {
            for k in 0..key_len {
                mk0[k] = keys[0][k] ^ round;
                mk1[k] = keys[1][k] ^ round;
                mk2[k] = keys[2][k] ^ round;
                mk3[k] = keys[3][k] ^ round;
            }
            s0 = RC4_INIT;
            s1 = RC4_INIT;
            s2 = RC4_INIT;
            s3 = RC4_INIT;
            let (mut j0, mut j1, mut j2, mut j3) = (0u8, 0u8, 0u8, 0u8);
            if key_len == 16 {
                for i in 0..256 {
                    let m = i & 15;
                    j0 = j0.wrapping_add(s0[i]).wrapping_add(mk0[m]);
                    j1 = j1.wrapping_add(s1[i]).wrapping_add(mk1[m]);
                    j2 = j2.wrapping_add(s2[i]).wrapping_add(mk2[m]);
                    j3 = j3.wrapping_add(s3[i]).wrapping_add(mk3[m]);
                    s0.swap(i, j0 as usize);
                    s1.swap(i, j1 as usize);
                    s2.swap(i, j2 as usize);
                    s3.swap(i, j3 as usize);
                }
            } else {
                let mut k_idx = 0usize;
                for i in 0..256 {
                    j0 = j0.wrapping_add(s0[i]).wrapping_add(mk0[k_idx]);
                    j1 = j1.wrapping_add(s1[i]).wrapping_add(mk1[k_idx]);
                    j2 = j2.wrapping_add(s2[i]).wrapping_add(mk2[k_idx]);
                    j3 = j3.wrapping_add(s3[i]).wrapping_add(mk3[k_idx]);
                    s0.swap(i, j0 as usize);
                    s1.swap(i, j1 as usize);
                    s2.swap(i, j2 as usize);
                    s3.swap(i, j3 as usize);
                    k_idx += 1;
                    if k_idx == key_len {
                        k_idx = 0;
                    }
                }
            }

            let mut pi: u8 = 0;
            let (mut pj0, mut pj1, mut pj2, mut pj3) = (0u8, 0u8, 0u8, 0u8);
            let [b0, b1, b2, b3] = &mut bufs;
            for (((x0, x1), x2), x3) in b0.iter_mut().zip(b1).zip(b2).zip(b3) {
                pi = pi.wrapping_add(1);
                let pi_u = pi as usize;
                pj0 = pj0.wrapping_add(s0[pi_u]);
                pj1 = pj1.wrapping_add(s1[pi_u]);
                pj2 = pj2.wrapping_add(s2[pi_u]);
                pj3 = pj3.wrapping_add(s3[pi_u]);
                s0.swap(pi_u, pj0 as usize);
                s1.swap(pi_u, pj1 as usize);
                s2.swap(pi_u, pj2 as usize);
                s3.swap(pi_u, pj3 as usize);
                *x0 ^= s0[(s0[pi_u].wrapping_add(s0[pj0 as usize])) as usize];
                *x1 ^= s1[(s1[pi_u].wrapping_add(s1[pj1 as usize])) as usize];
                *x2 ^= s2[(s2[pi_u].wrapping_add(s2[pj2 as usize])) as usize];
                *x3 ^= s3[(s3[pi_u].wrapping_add(s3[pj3 as usize])) as usize];
            }
        }

        for lane in 0..4 {
            for k in 0..key_len {
                mk0[k] = keys[lane][k] ^ 19;
            }
            if rc4_matches_prefix(&mk0[..key_len], &bufs[lane], &self.enc.user_hash[..16]) {
                return Some(lane);
            }
        }
        None
    }

    /// Verify user password for AES-256 (R=5/R=6).
    fn verify_user_password_aes256_with_work(
        &self,
        password: &[u8],
        work: &mut [u8; 16384],
    ) -> bool {
        let password = &password[..password.len().min(127)];
        let salt = self.enc.u_validation_salt.as_ref().map_or(&[][..], |s| s);
        let computed = if self.enc.revision >= 6 {
            algorithm_2b_with_work(password, salt, &[], work)
        } else {
            let mut hasher = Sha256::new();
            hasher.update(password);
            hasher.update(salt);
            let out: [u8; 32] = hasher.finalize().into();
            out
        };
        computed[..] == self.enc.user_hash[..32]
    }

    /// Verify user password for AES-256 (R=5/R=6).
    fn verify_user_password_aes256(&self, password: &[u8]) -> bool {
        let mut work = [0u8; 16384];
        self.verify_user_password_aes256_with_work(password, &mut work)
    }
}

impl PasswordVerifier for PdfVerifier {
    fn verify(&self, password: &[u8]) -> bool {
        self.verify_password(password)
    }

    fn verify_batch(&self, batch: &CandidateBatch, stop: &AtomicBool) -> BatchCheckResult {
        if self.enc.version < 5 && !self.try_owner {
            let total = batch.len();
            let mut attempts = 0u64;
            let mut i = 0usize;
            while i + 4 <= total {
                if stop.load(Ordering::Relaxed) {
                    return BatchCheckResult {
                        matched: None,
                        attempts,
                    };
                }
                let p0 = batch.get(i).unwrap();
                let p1 = batch.get(i + 1).unwrap();
                let p2 = batch.get(i + 2).unwrap();
                let p3 = batch.get(i + 3).unwrap();
                let (keys, key_len) = self.derive_key_rc4_4x([p0, p1, p2, p3]);
                if let Some(lane) = self.verify_user_rc4_4x(&keys, key_len) {
                    return BatchCheckResult {
                        matched: Some(i + lane),
                        attempts: attempts + lane as u64 + 1,
                    };
                }
                attempts += 4;
                i += 4;
            }
            while i < total {
                if stop.load(Ordering::Relaxed) {
                    break;
                }
                attempts += 1;
                if self.verify_user_password_rc4(batch.get(i).unwrap()) {
                    return BatchCheckResult {
                        matched: Some(i),
                        attempts,
                    };
                }
                i += 1;
            }
            return BatchCheckResult {
                matched: None,
                attempts,
            };
        }

        if self.enc.version >= 5 && !self.try_owner {
            let mut work = [0u8; 16384];
            let mut attempts = 0u64;
            for (idx, pw) in batch.iter().enumerate() {
                if stop.load(Ordering::Relaxed) {
                    break;
                }
                attempts += 1;
                if self.verify_user_password_aes256_with_work(pw, &mut work) {
                    return BatchCheckResult {
                        matched: Some(idx),
                        attempts,
                    };
                }
            }
            return BatchCheckResult {
                matched: None,
                attempts,
            };
        }

        let mut attempts = 0u64;
        for (idx, pw) in batch.iter().enumerate() {
            if stop.load(Ordering::Relaxed) {
                break;
            }
            attempts += 1;
            if self.verify_password(pw) {
                return BatchCheckResult {
                    matched: Some(idx),
                    attempts,
                };
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

        if self.enc.version < 5 && !self.try_owner {
            let mut base = [0u8; 32];
            let p_len = prefix.len().min(32);
            base[..p_len].copy_from_slice(&prefix[..p_len]);
            if p_len < 32 {
                // Candidate is `prefix || last_byte` (length `p_len + 1`), so PADDING begins at `p_len + 1`.
                let pad_count = 32 - (p_len + 1);
                base[p_len + 1..].copy_from_slice(&PADDING[..pad_count]);
            }
            let mut bufs = [base; 4];

            let mut attempts = 0u64;
            let mut i = 0usize;
            let n = last_bytes.len();
            while i + 4 <= n {
                if stop.load(Ordering::Relaxed) {
                    return BatchCheckResult {
                        matched: None,
                        attempts,
                    };
                }
                if prefix.len() < 32 {
                    bufs[0][prefix.len()] = last_bytes[i];
                    bufs[1][prefix.len()] = last_bytes[i + 1];
                    bufs[2][prefix.len()] = last_bytes[i + 2];
                    bufs[3][prefix.len()] = last_bytes[i + 3];
                }
                let (keys, key_len) =
                    self.derive_key_rc4_4x([&bufs[0], &bufs[1], &bufs[2], &bufs[3]]);
                if let Some(lane) = self.verify_user_rc4_4x(&keys, key_len) {
                    return BatchCheckResult {
                        matched: Some(i + lane),
                        attempts: attempts + lane as u64 + 1,
                    };
                }
                attempts += 4;
                i += 4;
            }
            while i < n {
                if stop.load(Ordering::Relaxed) {
                    break;
                }
                let cand = pbuf.with_last(last_bytes[i]);
                attempts += 1;
                if self.verify_user_password_rc4(cand) {
                    return BatchCheckResult {
                        matched: Some(i),
                        attempts,
                    };
                }
                i += 1;
            }
            return BatchCheckResult {
                matched: None,
                attempts,
            };
        }

        if self.enc.version >= 5 && !self.try_owner {
            let mut work = [0u8; 16384];
            let mut attempts = 0u64;
            for (idx, &b) in last_bytes.iter().enumerate() {
                if stop.load(Ordering::Relaxed) {
                    break;
                }
                let cand = pbuf.with_last(b);
                attempts += 1;
                if self.verify_user_password_aes256_with_work(cand, &mut work) {
                    return BatchCheckResult {
                        matched: Some(idx),
                        attempts,
                    };
                }
            }
            return BatchCheckResult {
                matched: None,
                attempts,
            };
        }

        let mut attempts = 0u64;
        for (idx, &b) in last_bytes.iter().enumerate() {
            if stop.load(Ordering::Relaxed) {
                break;
            }
            let cand = pbuf.with_last(b);
            attempts += 1;
            if self.verify_password(cand) {
                return BatchCheckResult {
                    matched: Some(idx),
                    attempts,
                };
            }
        }
        BatchCheckResult {
            matched: None,
            attempts,
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verify_prefix_group_and_batch_match_scalar() {
        let verifier = PdfVerifier::new(Path::new("examples/test.pdf"), false).unwrap();
        let stop = AtomicBool::new(false);

        let res = verifier.verify_prefix_group(b"92e", b"abcdoef", &stop);
        assert_eq!(res.matched, Some(4));
        assert_eq!(res.attempts, 5);

        let mut batch = CandidateBatch::new();
        for w in [b"wrong1".as_slice(), b"wrong2", b"wrong3", b"92eo", b"tail"] {
            batch.push(w);
        }
        let batch_res = verifier.verify_batch(&batch, &stop);
        assert_eq!(batch_res.matched, Some(3));
        assert_eq!(batch_res.attempts, 4);
    }
}
