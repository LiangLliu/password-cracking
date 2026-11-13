use sha1::Digest as _;

const SHA1_IV: [u32; 5] = [
    0x6745_2301,
    0xEFCD_AB89,
    0x98BA_DCFE,
    0x1032_5476,
    0xC3D2_E1F0,
];

/// Compile-time CRC32 lookup table (polynomial 0xEDB88320).
const CRC32_TABLE: [u32; 256] = {
    let mut tbl = [0u32; 256];
    let mut i = 0u32;
    while i < 256 {
        let mut crc = i;
        let mut b = 0;
        while b < 8 {
            crc = if crc & 1 != 0 {
                (crc >> 1) ^ 0xEDB8_8320
            } else {
                crc >> 1
            };
            b += 1;
        }
        tbl[i as usize] = crc;
        i += 1;
    }
    tbl
};

/// Lightweight parameters extracted from a ZipCrypto encrypted entry.
#[derive(Clone)]
pub(super) struct ZipCryptoCheck {
    pub(super) header: [u8; 12],
    pub(super) expected_byte: u8,
}

/// Parameters extracted from a WinZip AES encrypted entry for quick verification.
pub(super) struct AesCheckParams {
    pub(super) salt: [u8; 16],
    pub(super) salt_len: usize,
    pub(super) block_index: u32,
    pub(super) verify_offset: usize,
    pub(super) expected_verify: [u8; 2],
}

/// Traditional PKWARE ZipCrypto stream cipher state (3 x 32-bit keys).
#[derive(Clone, Copy)]
pub(super) struct ZipCryptoKeys {
    k0: u32,
    k1: u32,
    k2: u32,
}
impl ZipCryptoKeys {
    const INIT_K0: u32 = 0x1234_5678;
    const INIT_K1: u32 = 0x2345_6789;
    const INIT_K2: u32 = 0x3456_7890;

    #[inline]
    pub(super) fn from_password(password: &[u8]) -> Self {
        let mut keys = Self {
            k0: Self::INIT_K0,
            k1: Self::INIT_K1,
            k2: Self::INIT_K2,
        };
        for &byte in password {
            keys.update(byte);
        }
        keys
    }

    #[inline]
    pub(super) fn update(&mut self, byte: u8) {
        self.k0 = crc32_byte(self.k0, byte);
        self.k1 = self
            .k1
            .wrapping_add(self.k0 & 0xff)
            .wrapping_mul(134_775_813)
            .wrapping_add(1);
        self.k2 = crc32_byte(self.k2, (self.k1 >> 24) as u8);
    }

    #[inline]
    pub(super) fn stream_byte(&self) -> u8 {
        // The multiplication must be done in u32 to match the reference
        // implementation (C uses `unsigned` = 32-bit for the product).
        let temp = (self.k2 | 2) & 0xffff;
        ((temp.wrapping_mul(temp ^ 1)) >> 8) as u8
    }

    #[inline]
    pub(super) fn decrypt_byte(&mut self, cipher: u8) -> u8 {
        let plain = cipher ^ self.stream_byte();
        self.update(plain);
        plain
    }

    #[inline]
    pub(super) fn check_header(&self, check: &ZipCryptoCheck) -> bool {
        let mut k = *self;
        for &cipher in &check.header[..11] {
            k.decrypt_byte(cipher);
        }
        // The 12th byte check does not need to update key state afterwards.
        (check.header[11] ^ k.stream_byte()) == check.expected_byte
    }
}

#[inline]
fn crc32_byte(crc: u32, byte: u8) -> u32 {
    let idx = ((crc ^ u32::from(byte)) & 0xff) as usize;
    (crc >> 8) ^ CRC32_TABLE[idx]
}

#[inline]
fn hmac_sha1_states(password: &[u8]) -> ([u32; 5], [u32; 5]) {
    let mut key_block = [0u8; 64];
    if password.len() <= 64 {
        key_block[..password.len()].copy_from_slice(password);
    } else {
        let digest = sha1::Sha1::digest(password);
        key_block[..20].copy_from_slice(&digest);
    }

    let mut ipad = [0u8; 64];
    let mut opad = [0u8; 64];
    for i in 0..64 {
        ipad[i] = key_block[i] ^ 0x36;
        opad[i] = key_block[i] ^ 0x5c;
    }

    let mut ipad_state = SHA1_IV;
    sha1::block_api::compress(&mut ipad_state, &[ipad]);
    let mut opad_state = SHA1_IV;
    sha1::block_api::compress(&mut opad_state, &[opad]);
    (ipad_state, opad_state)
}

#[inline]
fn write_u32x5_be(words: &[u32; 5], out: &mut [u8]) {
    for (chunk, &w) in out[..20]
        .as_chunks_mut::<4>()
        .0
        .iter_mut()
        .zip(words.iter())
    {
        chunk.copy_from_slice(&w.to_be_bytes());
    }
}

/// Compute a single 20-byte PBKDF2-HMAC-SHA1 block (1,000 iterations) and
/// check the 2-byte password verification value without heap allocations.
#[inline]
pub(super) fn aes_quick_check_single(password: &[u8], params: &AesCheckParams) -> bool {
    let (ipad_state, opad_state) = hmac_sha1_states(password);

    // U_1 = HMAC(password, salt || block_index_be)
    let mut inner_1 = [0u8; 64];
    inner_1[..params.salt_len].copy_from_slice(&params.salt[..params.salt_len]);
    inner_1[params.salt_len..params.salt_len + 4]
        .copy_from_slice(&params.block_index.to_be_bytes());
    let msg_len = params.salt_len + 4;
    inner_1[msg_len] = 0x80;
    let bit_len_1 = ((64 + msg_len) as u64) * 8;
    inner_1[56..64].copy_from_slice(&bit_len_1.to_be_bytes());

    let mut inner_iter = [0u8; 64];
    inner_iter[20] = 0x80;
    inner_iter[56..64].copy_from_slice(&(672u64).to_be_bytes());

    let mut outer_block = [0u8; 64];
    outer_block[20] = 0x80;
    outer_block[56..64].copy_from_slice(&(672u64).to_be_bytes());

    let mut h = ipad_state;
    sha1::block_api::compress(&mut h, &[inner_1]);
    write_u32x5_be(&h, &mut outer_block[..20]);

    let mut u_state = opad_state;
    sha1::block_api::compress(&mut u_state, &[outer_block]);
    let mut t_state = u_state;

    for _ in 1..1000 {
        write_u32x5_be(&u_state, &mut inner_iter[..20]);
        h = ipad_state;
        sha1::block_api::compress(&mut h, &[inner_iter]);

        write_u32x5_be(&h, &mut outer_block[..20]);
        u_state = opad_state;
        sha1::block_api::compress(&mut u_state, &[outer_block]);

        for k in 0..5 {
            t_state[k] ^= u_state[k];
        }
    }

    let mut block_bytes = [0u8; 20];
    write_u32x5_be(&t_state, &mut block_bytes);
    block_bytes[params.verify_offset..params.verify_offset + 2] == params.expected_verify
}

/// 4-way interleaved PBKDF2-HMAC-SHA1 verification to saturate hardware SHA-1 pipelines.
pub(super) fn aes_quick_check_4(
    p0: &[u8],
    p1: &[u8],
    p2: &[u8],
    p3: &[u8],
    params: &AesCheckParams,
) -> u8 {
    let (ip0, op0) = hmac_sha1_states(p0);
    let (ip1, op1) = hmac_sha1_states(p1);
    let (ip2, op2) = hmac_sha1_states(p2);
    let (ip3, op3) = hmac_sha1_states(p3);

    let mut inner_1 = [0u8; 64];
    inner_1[..params.salt_len].copy_from_slice(&params.salt[..params.salt_len]);
    inner_1[params.salt_len..params.salt_len + 4]
        .copy_from_slice(&params.block_index.to_be_bytes());
    let msg_len = params.salt_len + 4;
    inner_1[msg_len] = 0x80;
    let bit_len_1 = ((64 + msg_len) as u64) * 8;
    inner_1[56..64].copy_from_slice(&bit_len_1.to_be_bytes());

    let mut in0 = [0u8; 64];
    in0[20] = 0x80;
    in0[56..64].copy_from_slice(&(672u64).to_be_bytes());
    let mut in1 = in0;
    let mut in2 = in0;
    let mut in3 = in0;

    let mut out0 = in0;
    let mut out1 = in0;
    let mut out2 = in0;
    let mut out3 = in0;

    let mut h0 = ip0;
    let mut h1 = ip1;
    let mut h2 = ip2;
    let mut h3 = ip3;
    sha1::block_api::compress(&mut h0, &[inner_1]);
    sha1::block_api::compress(&mut h1, &[inner_1]);
    sha1::block_api::compress(&mut h2, &[inner_1]);
    sha1::block_api::compress(&mut h3, &[inner_1]);

    write_u32x5_be(&h0, &mut out0[..20]);
    write_u32x5_be(&h1, &mut out1[..20]);
    write_u32x5_be(&h2, &mut out2[..20]);
    write_u32x5_be(&h3, &mut out3[..20]);

    let mut u0 = op0;
    let mut u1 = op1;
    let mut u2 = op2;
    let mut u3 = op3;
    sha1::block_api::compress(&mut u0, &[out0]);
    sha1::block_api::compress(&mut u1, &[out1]);
    sha1::block_api::compress(&mut u2, &[out2]);
    sha1::block_api::compress(&mut u3, &[out3]);

    let mut t0 = u0;
    let mut t1 = u1;
    let mut t2 = u2;
    let mut t3 = u3;

    for _ in 1..1000 {
        write_u32x5_be(&u0, &mut in0[..20]);
        write_u32x5_be(&u1, &mut in1[..20]);
        write_u32x5_be(&u2, &mut in2[..20]);
        write_u32x5_be(&u3, &mut in3[..20]);

        h0 = ip0;
        h1 = ip1;
        h2 = ip2;
        h3 = ip3;
        sha1::block_api::compress(&mut h0, &[in0]);
        sha1::block_api::compress(&mut h1, &[in1]);
        sha1::block_api::compress(&mut h2, &[in2]);
        sha1::block_api::compress(&mut h3, &[in3]);

        write_u32x5_be(&h0, &mut out0[..20]);
        write_u32x5_be(&h1, &mut out1[..20]);
        write_u32x5_be(&h2, &mut out2[..20]);
        write_u32x5_be(&h3, &mut out3[..20]);

        u0 = op0;
        u1 = op1;
        u2 = op2;
        u3 = op3;
        sha1::block_api::compress(&mut u0, &[out0]);
        sha1::block_api::compress(&mut u1, &[out1]);
        sha1::block_api::compress(&mut u2, &[out2]);
        sha1::block_api::compress(&mut u3, &[out3]);

        for k in 0..5 {
            t0[k] ^= u0[k];
            t1[k] ^= u1[k];
            t2[k] ^= u2[k];
            t3[k] ^= u3[k];
        }
    }

    let mut b0 = [0u8; 20];
    let mut b1 = [0u8; 20];
    let mut b2 = [0u8; 20];
    let mut b3 = [0u8; 20];
    write_u32x5_be(&t0, &mut b0);
    write_u32x5_be(&t1, &mut b1);
    write_u32x5_be(&t2, &mut b2);
    write_u32x5_be(&t3, &mut b3);

    let off = params.verify_offset;
    let m0 = u8::from(b0[off..off + 2] == params.expected_verify);
    let m1 = u8::from(b1[off..off + 2] == params.expected_verify);
    let m2 = u8::from(b2[off..off + 2] == params.expected_verify);
    let m3 = u8::from(b3[off..off + 2] == params.expected_verify);
    m0 | (m1 << 1) | (m2 << 2) | (m3 << 3)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crc32_table_is_standard() {
        assert_eq!(CRC32_TABLE[0], 0x0000_0000);
        assert_eq!(CRC32_TABLE[1], 0x7707_3096);
        assert_eq!(CRC32_TABLE[255], 0x2D02_EF8D);
    }

    #[test]
    fn zipcrypto_keys_change_with_password() {
        let keys = ZipCryptoKeys::from_password(b"test");
        assert_ne!(keys.k0, ZipCryptoKeys::INIT_K0);
        assert_ne!(keys.k1, ZipCryptoKeys::INIT_K1);
    }

    #[test]
    fn zipcrypto_decrypt_header_known_password() {
        let header = [
            0xfc, 0x3f, 0x66, 0xcc, 0x86, 0x4c, 0xf2, 0x99, 0x7d, 0x76, 0xcc, 0x94,
        ];
        let mut keys = ZipCryptoKeys::from_password(b"92eo");
        let mut decrypted = [0u8; 12];
        for (i, &cipher) in header.iter().enumerate() {
            decrypted[i] = keys.decrypt_byte(cipher);
        }
        assert!(
            decrypted[11] == 0x74 || decrypted[11] == 0x47,
            "Neither check byte matched: got {:#04x}",
            decrypted[11]
        );
    }

    #[test]
    fn aes_single_block_pbkdf2_matches_full_derivation() {
        // Test AES-128, AES-192, and AES-256 with short and >64-byte passwords.
        let long_pw = vec![b'x'; 80];
        let passwords: [&[u8]; 3] = [b"", b"92eo", &long_pw];
        for (salt_len, key_len) in [(8usize, 16usize), (12, 24), (16, 32)] {
            let salt: Vec<u8> = (0..salt_len).map(|i| (i * 17 + 3) as u8).collect();
            let verify_pos = 2 * key_len;
            let block_index = (verify_pos / 20) as u32 + 1;
            let verify_offset = verify_pos % 20;

            for pw in passwords {
                let mut derived = vec![0u8; verify_pos + 2];
                // Naive PBKDF2-HMAC-SHA1 reference
                let mut block = 1u32;
                let mut written = 0usize;
                while written < derived.len() {
                    let (ip, op) = hmac_sha1_states(pw);
                    let mut m = Vec::with_capacity(salt.len() + 4);
                    m.extend_from_slice(&salt);
                    m.extend_from_slice(&block.to_be_bytes());
                    let hmac_once = |msg: &[u8]| -> [u8; 20] {
                        let _ = (ip, op);
                        let mut k = [0u8; 64];
                        if pw.len() <= 64 {
                            k[..pw.len()].copy_from_slice(pw);
                        } else {
                            k[..20].copy_from_slice(&sha1::Sha1::digest(pw));
                        }
                        let mut h_in = sha1::Sha1::new();
                        let ipad: [u8; 64] = std::array::from_fn(|i| k[i] ^ 0x36);
                        h_in.update(ipad);
                        h_in.update(msg);
                        let inner = h_in.finalize();
                        let mut h_out = sha1::Sha1::new();
                        let opad: [u8; 64] = std::array::from_fn(|i| k[i] ^ 0x5c);
                        h_out.update(opad);
                        h_out.update(inner);
                        h_out.finalize().into()
                    };
                    let mut u = hmac_once(&m);
                    let mut t = u;
                    for _ in 1..1000 {
                        u = hmac_once(&u);
                        for j in 0..20 {
                            t[j] ^= u[j];
                        }
                    }
                    let take = (derived.len() - written).min(20);
                    derived[written..written + take].copy_from_slice(&t[..take]);
                    written += take;
                    block += 1;
                }

                let mut salt_arr = [0u8; 16];
                salt_arr[..salt_len].copy_from_slice(&salt);
                let expected = [derived[verify_pos], derived[verify_pos + 1]];
                let params = AesCheckParams {
                    salt: salt_arr,
                    salt_len,
                    block_index,
                    verify_offset,
                    expected_verify: expected,
                };
                assert!(aes_quick_check_single(pw, &params));
                let mut wrong = params;
                wrong.expected_verify[0] ^= 0xff;
                assert!(!aes_quick_check_single(pw, &wrong));
            }
        }
    }
}
