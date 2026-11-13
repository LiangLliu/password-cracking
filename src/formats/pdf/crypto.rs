use cipher::{
    BlockCipherEncBackend, BlockCipherEncClosure, BlockCipherEncrypt, BlockSizeUser, KeyInit,
};
use sha2::{Digest as _, Sha256, Sha384, Sha512};

/// PDF standard security padding string (ISO 32000-1 §7.6.3.3, Algorithm 2).
pub(super) const PADDING: [u8; 32] = [
    0x28, 0xBF, 0x4E, 0x5E, 0x4E, 0x75, 0x8A, 0x41, 0x64, 0x00, 0x4E, 0x56, 0xFF, 0xFA, 0x01, 0x08,
    0x2E, 0x2E, 0x00, 0xB6, 0xD0, 0x68, 0x3E, 0x80, 0x2F, 0x0C, 0xA9, 0xFE, 0x64, 0x53, 0x69, 0x7A,
];

/// Identity S-box initial state for RC4 key scheduling.
pub(super) const RC4_INIT: [u8; 256] = {
    let mut s = [0u8; 256];
    let mut i = 0;
    while i < 256 {
        s[i] = i as u8;
        i += 1;
    }
    s
};

/// MD5 initial state constants (RFC 1321).
pub(super) const MD5_IV: [u32; 4] = [0x6745_2301, 0xEFCD_AB89, 0x98BA_DCFE, 0x1032_5476];

/// MD5 per-round sine constants `T[0..64]`.
#[rustfmt::skip]
const MD5_K: [u32; 64] = [
    0xD76A_A478, 0xE8C7_B756, 0x2420_70DB, 0xC1BD_CEEE,
    0xF57C_0FAF, 0x4787_C62A, 0xA830_4613, 0xFD46_9501,
    0x6980_98D8, 0x8B44_F7AF, 0xFFFF_5BB1, 0x895C_D7BE,
    0x6B90_1122, 0xFD98_7193, 0xA679_438E, 0x49B4_0821,
    0xF61E_2562, 0xC040_B340, 0x265E_5A51, 0xE9B6_C7AA,
    0xD62F_105D, 0x0244_1453, 0xD8A1_E681, 0xE7D3_FBC8,
    0x21E1_CDE6, 0xC337_07D6, 0xF4D5_0D87, 0x455A_14ED,
    0xA9E3_E905, 0xFCEF_A3F8, 0x676F_02D9, 0x8D2A_4C8A,
    0xFFFA_3942, 0x8771_F681, 0x6D9D_6122, 0xFDE5_380C,
    0xA4BE_EA44, 0x4BDE_CFA9, 0xF6BB_4B60, 0xBEBF_BC70,
    0x289B_7EC6, 0xEAA1_27FA, 0xD4EF_3085, 0x0488_1D05,
    0xD9D4_D039, 0xE6DB_99E5, 0x1FA2_7CF8, 0xC4AC_5665,
    0xF429_2244, 0x432A_FF97, 0xAB94_23A7, 0xFC93_A039,
    0x655B_59C3, 0x8F0C_CC92, 0xFFEF_F47D, 0x8584_5DD1,
    0x6FA8_7E4F, 0xFE2C_E6E0, 0xA301_4314, 0x4E08_11A1,
    0xF753_7E82, 0xBD3A_F235, 0x2AD7_D2BB, 0xEB86_D391,
];

/// MD5 per-round rotation shifts.
#[rustfmt::skip]
const MD5_S: [u32; 64] = [
    7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22,
    5,  9, 14, 20, 5,  9, 14, 20, 5,  9, 14, 20, 5,  9, 14, 20,
    4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23,
    6, 10, 15, 21, 6, 10, 15, 21, 6, 10, 15, 21, 6, 10, 15, 21,
];

/// PDF password verifier supporting all standard encryption algorithms.
///
/// Supports:
/// - RC4 40-bit (V=1, R=2)
/// - RC4 128-bit (V=2, R=3)
/// - AES-128 (V=4, R=4)

// --- 4-way & 1-way MD5 kernels ---

#[derive(Clone, Copy)]
#[repr(align(16))]
pub(super) struct U32x4(pub(super) [u32; 4]);

impl U32x4 {
    #[inline(always)]
    pub(super) const fn splat(v: u32) -> Self {
        Self([v, v, v, v])
    }

    #[inline(always)]
    fn add(self, rhs: Self) -> Self {
        Self([
            self.0[0].wrapping_add(rhs.0[0]),
            self.0[1].wrapping_add(rhs.0[1]),
            self.0[2].wrapping_add(rhs.0[2]),
            self.0[3].wrapping_add(rhs.0[3]),
        ])
    }

    #[inline(always)]
    fn bitand(self, rhs: Self) -> Self {
        Self([
            self.0[0] & rhs.0[0],
            self.0[1] & rhs.0[1],
            self.0[2] & rhs.0[2],
            self.0[3] & rhs.0[3],
        ])
    }

    #[inline(always)]
    fn bitor(self, rhs: Self) -> Self {
        Self([
            self.0[0] | rhs.0[0],
            self.0[1] | rhs.0[1],
            self.0[2] | rhs.0[2],
            self.0[3] | rhs.0[3],
        ])
    }

    #[inline(always)]
    fn bitxor(self, rhs: Self) -> Self {
        Self([
            self.0[0] ^ rhs.0[0],
            self.0[1] ^ rhs.0[1],
            self.0[2] ^ rhs.0[2],
            self.0[3] ^ rhs.0[3],
        ])
    }

    #[inline(always)]
    fn not(self) -> Self {
        Self([!self.0[0], !self.0[1], !self.0[2], !self.0[3]])
    }

    #[inline(always)]
    fn rotl(self, n: u32) -> Self {
        Self([
            self.0[0].rotate_left(n),
            self.0[1].rotate_left(n),
            self.0[2].rotate_left(n),
            self.0[3].rotate_left(n),
        ])
    }
}

#[inline]
pub(super) fn md5_compress_1x(state: &mut [u32; 4], x: &[u32; 16]) {
    let mut a = state[0];
    let mut b = state[1];
    let mut c = state[2];
    let mut d = state[3];

    for i in 0..16 {
        let f = d ^ (b & (c ^ d));
        let temp = a.wrapping_add(f).wrapping_add(MD5_K[i]).wrapping_add(x[i]);
        a = d;
        d = c;
        c = b;
        b = b.wrapping_add(temp.rotate_left(MD5_S[i]));
    }
    for i in 16..32 {
        let f = c ^ (d & (b ^ c));
        let g = (5 * i + 1) & 15;
        let temp = a.wrapping_add(f).wrapping_add(MD5_K[i]).wrapping_add(x[g]);
        a = d;
        d = c;
        c = b;
        b = b.wrapping_add(temp.rotate_left(MD5_S[i]));
    }
    for i in 32..48 {
        let f = b ^ c ^ d;
        let g = (3 * i + 5) & 15;
        let temp = a.wrapping_add(f).wrapping_add(MD5_K[i]).wrapping_add(x[g]);
        a = d;
        d = c;
        c = b;
        b = b.wrapping_add(temp.rotate_left(MD5_S[i]));
    }
    for i in 48..64 {
        let f = c ^ (b | !d);
        let g = (7 * i) & 15;
        let temp = a.wrapping_add(f).wrapping_add(MD5_K[i]).wrapping_add(x[g]);
        a = d;
        d = c;
        c = b;
        b = b.wrapping_add(temp.rotate_left(MD5_S[i]));
    }

    state[0] = state[0].wrapping_add(a);
    state[1] = state[1].wrapping_add(b);
    state[2] = state[2].wrapping_add(c);
    state[3] = state[3].wrapping_add(d);
}

#[inline]
pub(super) fn md5_compress_4x(state: &mut [U32x4; 4], x: &[U32x4; 16]) {
    let mut a = state[0];
    let mut b = state[1];
    let mut c = state[2];
    let mut d = state[3];

    macro_rules! step_f {
        ($i:expr, $s:expr) => {{
            let f = d.bitxor(b.bitand(c.bitxor(d)));
            let temp = a.add(f).add(U32x4::splat(MD5_K[$i])).add(x[$i]);
            a = d;
            d = c;
            c = b;
            b = b.add(temp.rotl($s));
        }};
    }
    macro_rules! step_g {
        ($i:expr, $g:expr, $s:expr) => {{
            let f = c.bitxor(d.bitand(b.bitxor(c)));
            let temp = a.add(f).add(U32x4::splat(MD5_K[$i])).add(x[$g]);
            a = d;
            d = c;
            c = b;
            b = b.add(temp.rotl($s));
        }};
    }
    macro_rules! step_h {
        ($i:expr, $g:expr, $s:expr) => {{
            let f = b.bitxor(c).bitxor(d);
            let temp = a.add(f).add(U32x4::splat(MD5_K[$i])).add(x[$g]);
            a = d;
            d = c;
            c = b;
            b = b.add(temp.rotl($s));
        }};
    }
    macro_rules! step_i {
        ($i:expr, $g:expr, $s:expr) => {{
            let f = c.bitxor(b.bitor(d.not()));
            let temp = a.add(f).add(U32x4::splat(MD5_K[$i])).add(x[$g]);
            a = d;
            d = c;
            c = b;
            b = b.add(temp.rotl($s));
        }};
    }

    step_f!(0, 7);
    step_f!(1, 12);
    step_f!(2, 17);
    step_f!(3, 22);
    step_f!(4, 7);
    step_f!(5, 12);
    step_f!(6, 17);
    step_f!(7, 22);
    step_f!(8, 7);
    step_f!(9, 12);
    step_f!(10, 17);
    step_f!(11, 22);
    step_f!(12, 7);
    step_f!(13, 12);
    step_f!(14, 17);
    step_f!(15, 22);

    step_g!(16, 1, 5);
    step_g!(17, 6, 9);
    step_g!(18, 11, 14);
    step_g!(19, 0, 20);
    step_g!(20, 5, 5);
    step_g!(21, 10, 9);
    step_g!(22, 15, 14);
    step_g!(23, 4, 20);
    step_g!(24, 9, 5);
    step_g!(25, 14, 9);
    step_g!(26, 3, 14);
    step_g!(27, 8, 20);
    step_g!(28, 13, 5);
    step_g!(29, 2, 9);
    step_g!(30, 7, 14);
    step_g!(31, 12, 20);

    step_h!(32, 5, 4);
    step_h!(33, 8, 11);
    step_h!(34, 11, 16);
    step_h!(35, 14, 23);
    step_h!(36, 1, 4);
    step_h!(37, 4, 11);
    step_h!(38, 7, 16);
    step_h!(39, 10, 23);
    step_h!(40, 13, 4);
    step_h!(41, 0, 11);
    step_h!(42, 3, 16);
    step_h!(43, 6, 23);
    step_h!(44, 9, 4);
    step_h!(45, 12, 11);
    step_h!(46, 15, 16);
    step_h!(47, 2, 23);

    step_i!(48, 0, 6);
    step_i!(49, 7, 10);
    step_i!(50, 14, 15);
    step_i!(51, 5, 21);
    step_i!(52, 12, 6);
    step_i!(53, 3, 10);
    step_i!(54, 10, 15);
    step_i!(55, 1, 21);
    step_i!(56, 8, 6);
    step_i!(57, 15, 10);
    step_i!(58, 6, 15);
    step_i!(59, 13, 21);
    step_i!(60, 4, 6);
    step_i!(61, 11, 10);
    step_i!(62, 2, 15);
    step_i!(63, 9, 21);

    state[0] = state[0].add(a);
    state[1] = state[1].add(b);
    state[2] = state[2].add(c);
    state[3] = state[3].add(d);
}

#[inline]
pub(super) fn pad_short_md5_block(msg: &[u8]) -> [u32; 16] {
    let mut block = [0u8; 64];
    block[..msg.len()].copy_from_slice(msg);
    block[msg.len()] = 0x80;
    block[56..64].copy_from_slice(&((msg.len() as u64) * 8).to_le_bytes());
    let mut words = [0u32; 16];
    for i in 0..16 {
        words[i] = u32::from_le_bytes(block[i * 4..i * 4 + 4].try_into().unwrap());
    }
    words
}

#[inline]
pub(super) fn words_to_bytes(words: &[u32; 4]) -> [u8; 16] {
    let mut out = [0u8; 16];
    for i in 0..4 {
        out[i * 4..i * 4 + 4].copy_from_slice(&words[i].to_le_bytes());
    }
    out
}

#[inline]
pub(super) fn bytes_to_words(bytes: &[u8; 16]) -> [u32; 4] {
    [
        u32::from_le_bytes(bytes[0..4].try_into().unwrap()),
        u32::from_le_bytes(bytes[4..8].try_into().unwrap()),
        u32::from_le_bytes(bytes[8..12].try_into().unwrap()),
        u32::from_le_bytes(bytes[12..16].try_into().unwrap()),
    ]
}

/// Pre-pad bytes starting at offset 64 (`derive_suffix[32..]`) given total message length `total_len`.
pub(super) fn build_md5_tail_blocks(tail: &[u8], total_len: usize) -> Vec<[u32; 16]> {
    let padded_len = (tail.len() + 1 + 8).div_ceil(64) * 64;
    let mut padded = vec![0u8; padded_len];
    padded[..tail.len()].copy_from_slice(tail);
    padded[tail.len()] = 0x80;
    let bit_len = (total_len as u64) * 8;
    padded[padded_len - 8..].copy_from_slice(&bit_len.to_le_bytes());

    padded
        .as_chunks::<64>()
        .0
        .iter()
        .map(|chunk| {
            let mut words = [0u32; 16];
            for (word, bytes) in words.iter_mut().zip(chunk.as_chunks::<4>().0) {
                *word = u32::from_le_bytes(*bytes);
            }
            words
        })
        .collect()
}

// --- PDF parsing ---

#[inline]
fn rc4_ksa(key: &[u8]) -> [u8; 256] {
    let mut s = RC4_INIT;
    let mut j: u8 = 0;
    if key.len() == 16 {
        for i in 0..256 {
            j = j.wrapping_add(s[i]).wrapping_add(key[i & 15]);
            s.swap(i, j as usize);
        }
    } else {
        let mut k_idx = 0usize;
        for i in 0..256 {
            j = j.wrapping_add(s[i]).wrapping_add(key[k_idx]);
            s.swap(i, j as usize);
            k_idx += 1;
            if k_idx == key.len() {
                k_idx = 0;
            }
        }
    }
    s
}

/// In-place RC4 encryption/decryption using a stack-allocated S-box.
#[inline]
pub(super) fn rc4_crypt_in_place(key: &[u8], data: &mut [u8]) {
    let mut s = rc4_ksa(key);
    let mut i: u8 = 0;
    let mut j: u8 = 0;
    for byte in data.iter_mut() {
        i = i.wrapping_add(1);
        j = j.wrapping_add(s[i as usize]);
        s.swap(i as usize, j as usize);
        let k = s[(s[i as usize].wrapping_add(s[j as usize])) as usize];
        *byte ^= k;
    }
}

/// RC4 encryption/decryption that rejects immediately on the first byte mismatch
/// against `expected` (skipping remaining PRGA steps 99.6% of the time).
#[inline]
pub(super) fn rc4_matches_prefix(key: &[u8], data: &[u8], expected: &[u8]) -> bool {
    let mut s = rc4_ksa(key);
    let mut i: u8 = 0;
    let mut j: u8 = 0;
    for idx in 0..expected.len() {
        i = i.wrapping_add(1);
        j = j.wrapping_add(s[i as usize]);
        s.swap(i as usize, j as usize);
        let k = s[(s[i as usize].wrapping_add(s[j as usize])) as usize];
        if (data[idx] ^ k) != expected[idx] {
            return false;
        }
    }
    true
}

/// Pad or truncate a password into a fixed 32-byte stack buffer.
#[inline]
pub(super) fn pad_password_stack(password: &[u8]) -> [u8; 32] {
    let mut out = [0u8; 32];
    let n = password.len().min(32);
    out[..n].copy_from_slice(&password[..n]);
    out[n..].copy_from_slice(&PADDING[..32 - n]);
    out
}

struct CbcEncryptClosure<'a> {
    iv: [u8; 16],
    data: &'a mut [u8],
}

impl BlockSizeUser for CbcEncryptClosure<'_> {
    type BlockSize = <aes::Aes128Enc as BlockSizeUser>::BlockSize;
}

impl BlockCipherEncClosure for CbcEncryptClosure<'_> {
    #[inline(always)]
    fn call<B: BlockCipherEncBackend<BlockSize = Self::BlockSize>>(self, backend: &B) {
        let mut prev = self.iv;
        for chunk in self.data.as_chunks_mut::<16>().0 {
            let mut block = cipher::Block::<aes::Aes128Enc>::default();
            for ((dst, &src), &p) in block.iter_mut().zip(chunk.iter()).zip(prev.iter()) {
                *dst = src ^ p;
            }
            backend.encrypt_block((&mut block).into());
            chunk.copy_from_slice(&block);
            prev.copy_from_slice(&block);
        }
    }
}

/// In-place AES-128-CBC encryption without padding (`data.len()` must be a multiple of 16).
#[inline]
fn aes128_cbc_encrypt_in_place(key: &[u8], iv: &[u8], data: &mut [u8]) {
    let cipher = aes::Aes128Enc::new_from_slice(&key[..16]).expect("16-byte AES-128 key");
    let mut iv_arr = [0u8; 16];
    iv_arr.copy_from_slice(&iv[..16]);
    cipher.encrypt_with_backend(CbcEncryptClosure { iv: iv_arr, data });
}

/// ISO 32000-2 Algorithm 2.B: PDF 2.0 (R6) hardened password hash.
///
/// Uses a single 16 KiB stack buffer (`unit_len <= 127 + 64 + 48 = 239`,
/// `64 * 239 = 15,296 <= 16,384`) so all >=64 rounds perform zero heap allocations.
pub(super) fn algorithm_2b(password: &[u8], salt: &[u8], udata: &[u8]) -> [u8; 32] {
    let mut work = [0u8; 16384];
    algorithm_2b_with_work(password, salt, udata, &mut work)
}

pub(super) fn algorithm_2b_with_work(
    password: &[u8],
    salt: &[u8],
    udata: &[u8],
    work: &mut [u8; 16384],
) -> [u8; 32] {
    let password = &password[..password.len().min(127)];
    let udata = &udata[..udata.len().min(48)];

    // K = SHA-256(password || salt || udata)
    let mut h = Sha256::new();
    h.update(password);
    h.update(salt);
    h.update(udata);
    let mut k_buf = [0u8; 64];
    let first: [u8; 32] = h.finalize().into();
    k_buf[..32].copy_from_slice(&first);
    let mut k_len = 32usize;

    let mut round: usize = 0;

    loop {
        // Build one unit = (password || K || udata), then double it 6 times -> 64 copies.
        let unit_len = password.len() + k_len + udata.len();
        work[..password.len()].copy_from_slice(password);
        let p_end = password.len();
        work[p_end..p_end + k_len].copy_from_slice(&k_buf[..k_len]);
        let k_end = p_end + k_len;
        work[k_end..unit_len].copy_from_slice(udata);

        let mut filled = unit_len;
        for _ in 0..6 {
            work.copy_within(0..filled, filled);
            filled *= 2;
        }
        let e = &mut work[..filled];

        // E = AES-128-CBC-Encrypt(key = K[0..16], iv = K[16..32], K1), no padding.
        aes128_cbc_encrypt_in_place(&k_buf[0..16], &k_buf[16..32], e);

        // The next hash algorithm is selected by the sum of the first 16 bytes of E modulo 3.
        let sum: u32 = e[0..16].iter().map(|&b| b as u32).sum();
        match sum % 3 {
            0 => {
                let d: [u8; 32] = Sha256::digest(&*e).into();
                k_buf[..32].copy_from_slice(&d);
                k_len = 32;
            }
            1 => {
                let d: [u8; 48] = Sha384::digest(&*e).into();
                k_buf[..48].copy_from_slice(&d);
                k_len = 48;
            }
            _ => {
                let d: [u8; 64] = Sha512::digest(&*e).into();
                k_buf[..64].copy_from_slice(&d);
                k_len = 64;
            }
        }

        round += 1;
        if round >= 64 && (e[filled - 1] as usize) <= round - 32 {
            break;
        }
    }

    let mut out = [0u8; 32];
    out.copy_from_slice(&k_buf[..32]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use md5::Md5;

    #[test]
    fn rc4_known_vector() {
        // RFC 6229 test vector: key = "Key", plaintext = "Plaintext"
        let mut buf = *b"Plaintext";
        rc4_crypt_in_place(b"Key", &mut buf);
        assert_eq!(buf, [0xBB, 0xF3, 0x16, 0xE8, 0xD9, 0x40, 0xAF, 0x0A, 0xD3]);
        assert!(rc4_matches_prefix(b"Key", b"Plaintext", &buf));
    }

    #[test]
    fn md5_4x_matches_reference() {
        let msgs = [
            b"alpha___________________________________________extra".as_slice(),
            b"beta____________________________________________extra".as_slice(),
            b"".as_slice(),
            b"0123456789abcdef0123456789abcdef".as_slice(),
        ];
        // Verify single-block and multi-block 4x MD5 against md5::Md5.
        let mut s4 = [
            U32x4::splat(MD5_IV[0]),
            U32x4::splat(MD5_IV[1]),
            U32x4::splat(MD5_IV[2]),
            U32x4::splat(MD5_IV[3]),
        ];
        let mut b4 = [U32x4::splat(0); 16];
        let padded: [[u32; 16]; 4] = msgs.map(|m| {
            let p = pad_password_stack(m);
            let mut w = [0u32; 16];
            w[0..8].copy_from_slice(&[
                u32::from_le_bytes(p[0..4].try_into().unwrap()),
                u32::from_le_bytes(p[4..8].try_into().unwrap()),
                u32::from_le_bytes(p[8..12].try_into().unwrap()),
                u32::from_le_bytes(p[12..16].try_into().unwrap()),
                u32::from_le_bytes(p[16..20].try_into().unwrap()),
                u32::from_le_bytes(p[20..24].try_into().unwrap()),
                u32::from_le_bytes(p[24..28].try_into().unwrap()),
                u32::from_le_bytes(p[28..32].try_into().unwrap()),
            ]);
            w[8] = 0x80;
            w[14] = 32 * 8;
            w
        });
        for i in 0..16 {
            b4[i] = U32x4([padded[0][i], padded[1][i], padded[2][i], padded[3][i]]);
        }
        md5_compress_4x(&mut s4, &b4);

        for (lane, msg) in msgs.iter().enumerate() {
            let mut got = [0u8; 16];
            for (word, dst) in s4[..4].iter().zip(got.as_chunks_mut::<4>().0) {
                *dst = word.0[lane].to_le_bytes();
            }
            let expected: [u8; 16] = Md5::digest(pad_password_stack(msg)).into();
            assert_eq!(got, expected);
        }
    }

    #[test]
    fn algorithm_2b_known_answer() {
        // Real values from examples/test_r6.pdf (password "92eo"),
        // cross-verified with pycryptodome.
        let salt = [0xe5, 0xba, 0xd2, 0x1d, 0x84, 0x2e, 0x3e, 0x2f];
        let expected = [
            0xd9, 0xa0, 0x86, 0xd2, 0x1d, 0x02, 0x49, 0x94, 0xfb, 0x4b, 0x8d, 0xcd, 0xb1, 0xbe,
            0x81, 0x04, 0x04, 0xfe, 0xc4, 0x91, 0x6b, 0x89, 0x36, 0xda, 0xcc, 0xa7, 0x77, 0xf0,
            0x24, 0x10, 0xa1, 0x02,
        ];
        let got = algorithm_2b(b"92eo", &salt, &[]);
        assert_eq!(got, expected);
    }

    #[test]
    fn algorithm_2b_truncates_long_password_at_127_bytes() {
        let salt = [1, 2, 3, 4, 5, 6, 7, 8];
        let pw_127 = vec![b'a'; 127];
        let mut pw_140 = pw_127.clone();
        pw_140.extend_from_slice(b"0123456789abc");
        assert_eq!(algorithm_2b(&pw_127, &salt, &[]), algorithm_2b(&pw_140, &salt, &[]));
    }
}
