mod crypto;

use super::{BatchCheckResult, PasswordVerifier, PrefixBuf};
use crate::generators::CandidateBatch;
use anyhow::{Context, Result};
use crypto::{
    AesCheckParams, ZipCryptoCheck, ZipCryptoKeys, aes_quick_check_4, aes_quick_check_single,
};
use std::fs::File;
use std::io::{Cursor, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicU64, Ordering},
};
use zip::ZipArchive;

/// Archive size threshold up to which we preload bytes into RAM so workers
/// never contend on filesystem descriptors during full-entry verification.
const MEMORY_CACHE_LIMIT: u64 = 32 * 1024 * 1024;

static NEXT_VERIFIER_ID: AtomicU64 = AtomicU64::new(1);

/// ZIP password verifier supporting both ZipCrypto and AES encryption.
///
/// - For ZipCrypto, `quick_check` tests the 12-byte encryption header in memory
///   (and a second encrypted entry when available), rejecting ~99.6%–99.998%
///   of wrong passwords without any decompression.
/// - For WinZip AES, `quick_check` computes only the single PBKDF2-HMAC-SHA1
///   block containing the 2-byte verification value (4x less work for AES-256)
///   using cached password HMAC states and zero heap allocations.
pub struct ZipVerifier {
    id: u64,
    source: ZipSource,
    /// Encrypted entries sorted smallest-first (and ZipCrypto before AES) so
    /// false positives fail on the cheapest entry first.
    encrypted_indices: Vec<usize>,
    has_aes: bool,
    zipcrypto_checks: Vec<ZipCryptoCheck>,
    aes_check: Option<AesCheckParams>,
}

enum ZipSource {
    Memory(Arc<[u8]>),
    File(PathBuf),
}

impl ZipVerifier {
    pub fn new(path: &Path) -> Result<Self> {
        let file = File::open(path).context("Failed to open ZIP")?;
        let file_len = file.metadata().map(|m| m.len()).unwrap_or(0);

        let source = if file_len <= MEMORY_CACHE_LIMIT {
            drop(file);
            let bytes = std::fs::read(path).context("Failed to read ZIP")?;
            ZipSource::Memory(bytes.into())
        } else {
            drop(file);
            ZipSource::File(path.to_path_buf())
        };

        match &source {
            ZipSource::Memory(bytes) => {
                let mut cursor = Cursor::new(&bytes[..]);
                Self::init_from_reader(&mut cursor, source.clone_meta())
            }
            ZipSource::File(p) => {
                let mut f = File::open(p).context("Failed to open ZIP")?;
                Self::init_from_reader(&mut f, ZipSource::File(p.clone()))
            }
        }
    }

    fn init_from_reader<R: Read + Seek>(reader: &mut R, source: ZipSource) -> Result<Self> {
        let mut archive = ZipArchive::new(reader).context("Invalid ZIP file")?;

        struct EntryMeta {
            index: usize,
            is_aes: bool,
            compressed_size: u64,
            crc32: u32,
            header_start: u64,
            central_extra: Vec<u8>,
        }

        let mut entries = Vec::new();
        let mut has_aes = false;

        for i in 0..archive.len() {
            let entry = archive.by_index_raw(i)?;
            if entry.encrypted() {
                let is_aes = entry.compression() == zip::CompressionMethod::Aes
                    || entry
                        .extra_data_fields()
                        .any(|field| matches!(field, zip::ExtraField::AeXEncryption(_)));
                if is_aes {
                    has_aes = true;
                }
                entries.push(EntryMeta {
                    index: i,
                    is_aes,
                    compressed_size: entry.compressed_size(),
                    crc32: entry.crc32(),
                    header_start: entry.header_start(),
                    central_extra: entry.extra_data().unwrap_or_default().to_vec(),
                });
            }
        }

        if entries.is_empty() {
            anyhow::bail!("ZIP is not encrypted");
        }

        // Check ZipCrypto entries before AES, and smaller entries before large ones.
        entries.sort_by_key(|e| (e.is_aes, e.compressed_size));
        let encrypted_indices: Vec<usize> = entries.iter().map(|e| e.index).collect();

        // Reclaim the reader after dropping `archive` to read local headers without reopening.
        let reader = archive.into_inner();

        let mut zipcrypto_checks = Vec::new();
        let mut aes_check = None;

        for e in &entries {
            if !e.is_aes && zipcrypto_checks.len() < 2 {
                let check = extract_zipcrypto_params(reader, e.header_start, e.crc32)?;
                zipcrypto_checks.push(check);
            } else if e.is_aes && aes_check.is_none() {
                aes_check = extract_aes_params(reader, e.header_start, &e.central_extra)?;
            }
            if zipcrypto_checks.len() >= 2 && (aes_check.is_some() || !has_aes) {
                break;
            }
        }

        Ok(Self {
            id: NEXT_VERIFIER_ID.fetch_add(1, Ordering::Relaxed),
            source,
            encrypted_indices,
            has_aes,
            zipcrypto_checks,
            aes_check,
        })
    }

    #[inline]
    fn quick_check_zipcrypto(&self, password: &[u8]) -> bool {
        let keys = ZipCryptoKeys::from_password(password);
        self.zipcrypto_checks.iter().all(|c| keys.check_header(c))
    }

    #[inline]
    fn quick_check_zipcrypto_rest(&self, password: &[u8]) -> bool {
        if self.zipcrypto_checks.len() <= 1 {
            return true;
        }
        let keys = ZipCryptoKeys::from_password(password);
        self.zipcrypto_checks[1..]
            .iter()
            .all(|c| keys.check_header(c))
    }
}

impl ZipSource {
    fn clone_meta(&self) -> Self {
        match self {
            Self::Memory(m) => Self::Memory(Arc::clone(m)),
            Self::File(p) => Self::File(p.clone()),
        }
    }
}

/// Read local file header at `header_start` and return `(flags, mod_time, extra_offset, extra_len, data_start)`.
fn read_local_header<R: Read + Seek>(
    reader: &mut R,
    header_start: u64,
) -> Result<(u16, u16, u64, usize, u64)> {
    reader.seek(SeekFrom::Start(header_start))?;
    let mut local_header = [0u8; 30];
    reader.read_exact(&mut local_header)?;
    if &local_header[0..4] != b"PK\x03\x04" {
        anyhow::bail!("Invalid local file header signature");
    }
    let flags = u16::from_le_bytes([local_header[6], local_header[7]]);
    let last_mod_time = u16::from_le_bytes([local_header[10], local_header[11]]);
    let name_len = u16::from_le_bytes([local_header[26], local_header[27]]) as u64;
    let extra_len = u16::from_le_bytes([local_header[28], local_header[29]]) as usize;
    let extra_start = header_start + 30 + name_len;
    let data_start = extra_start + extra_len as u64;
    Ok((flags, last_mod_time, extra_start, extra_len, data_start))
}

fn extract_zipcrypto_params<R: Read + Seek>(
    reader: &mut R,
    header_start: u64,
    crc32: u32,
) -> Result<ZipCryptoCheck> {
    let (flags, last_mod_time, _, _, data_start) = read_local_header(reader, header_start)?;
    reader.seek(SeekFrom::Start(data_start))?;
    let mut header = [0u8; 12];
    reader.read_exact(&mut header)?;
    let expected_byte = if flags & 0x0008 != 0 {
        ((last_mod_time >> 8) & 0xff) as u8
    } else {
        ((crc32 >> 24) & 0xff) as u8
    };
    Ok(ZipCryptoCheck {
        header,
        expected_byte,
    })
}

fn parse_aes_mode_from_extra(extra: &[u8]) -> Option<u8> {
    let mut i = 0;
    while i + 4 <= extra.len() {
        let id = u16::from_le_bytes([extra[i], extra[i + 1]]);
        let size = u16::from_le_bytes([extra[i + 2], extra[i + 3]]) as usize;
        let payload_start = i + 4;
        let Some(payload_end) = payload_start.checked_add(size) else {
            break;
        };
        if payload_end > extra.len() {
            break;
        }
        if id == 0x9901 && size >= 7 && &extra[payload_start + 2..payload_start + 4] == b"AE" {
            let mode = extra[payload_start + 4];
            if matches!(mode, 1..=3) {
                return Some(mode);
            }
        }
        i = payload_end;
    }
    None
}

fn extract_aes_params<R: Read + Seek>(
    reader: &mut R,
    header_start: u64,
    central_extra: &[u8],
) -> Result<Option<AesCheckParams>> {
    let (_, _, extra_start, extra_len, data_start) = read_local_header(reader, header_start)?;
    let mut local_extra = vec![0u8; extra_len];
    if extra_len > 0 {
        reader.seek(SeekFrom::Start(extra_start))?;
        reader.read_exact(&mut local_extra)?;
    }

    let Some(mode) = parse_aes_mode_from_extra(&local_extra)
        .or_else(|| parse_aes_mode_from_extra(central_extra))
    else {
        return Ok(None);
    };

    let (salt_len, key_len) = match mode {
        1 => (8usize, 16usize),
        2 => (12, 24),
        3 => (16, 32),
        _ => return Ok(None),
    };

    reader.seek(SeekFrom::Start(data_start))?;
    let mut salt = [0u8; 16];
    reader.read_exact(&mut salt[..salt_len])?;
    let mut expected_verify = [0u8; 2];
    reader.read_exact(&mut expected_verify)?;

    let verify_pos = 2 * key_len;
    let block_index = (verify_pos / 20) as u32 + 1;
    let verify_offset = verify_pos % 20;

    Ok(Some(AesCheckParams {
        salt,
        salt_len,
        block_index,
        verify_offset,
        expected_verify,
    }))
}

fn verify_archive_entries<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
    indices: &[usize],
    password: &[u8],
) -> bool {
    let mut buf = [0u8; 8192];
    for &index in indices {
        match archive.by_index_decrypt(index, password) {
            Ok(mut reader) => loop {
                match reader.read(&mut buf) {
                    Ok(0) => break,
                    Ok(_) => {}
                    Err(_) => return false,
                }
            },
            Err(_) => return false,
        }
    }
    true
}

impl PasswordVerifier for ZipVerifier {
    fn quick_check(&self, password: &[u8]) -> bool {
        if !self.zipcrypto_checks.is_empty() {
            return self.quick_check_zipcrypto(password);
        }
        match &self.aes_check {
            Some(params) => aes_quick_check_single(password, params),
            None => true,
        }
    }

    fn verify(&self, password: &[u8]) -> bool {
        use std::cell::RefCell;

        enum CachedArchive {
            Mem(ZipArchive<Cursor<Arc<[u8]>>>),
            Disk(ZipArchive<File>),
        }

        impl CachedArchive {
            fn verify(&mut self, indices: &[usize], password: &[u8]) -> bool {
                match self {
                    Self::Mem(a) => verify_archive_entries(a, indices, password),
                    Self::Disk(a) => verify_archive_entries(a, indices, password),
                }
            }
        }

        // Bounded per-thread cache keyed by verifier ID (max 4 archives) so
        // repeated library invocations never leak open file handles.
        thread_local! {
            static ARCHIVE_CACHE: RefCell<Vec<(u64, CachedArchive)>> = const { RefCell::new(Vec::new()) };
        }

        ARCHIVE_CACHE.with(|cache| {
            let mut cache = cache.borrow_mut();
            if let Some(pos) = cache.iter().position(|(id, _)| *id == self.id) {
                return cache[pos].1.verify(&self.encrypted_indices, password);
            }

            let entry = match &self.source {
                ZipSource::Memory(bytes) => {
                    let Ok(a) = ZipArchive::new(Cursor::new(Arc::clone(bytes))) else {
                        return false;
                    };
                    CachedArchive::Mem(a)
                }
                ZipSource::File(path) => {
                    let Ok(f) = File::open(path) else {
                        return false;
                    };
                    let Ok(a) = ZipArchive::new(f) else {
                        return false;
                    };
                    CachedArchive::Disk(a)
                }
            };

            if cache.len() >= 4 {
                cache.remove(0);
            }
            cache.push((self.id, entry));
            let last = cache.len() - 1;
            cache[last].1.verify(&self.encrypted_indices, password)
        })
    }

    fn verify_batch(&self, batch: &CandidateBatch, stop: &AtomicBool) -> BatchCheckResult {
        let len = batch.len();
        let mut attempts = 0u64;

        if let Some(primary) = self.zipcrypto_checks.first() {
            let mut i = 0usize;
            while i + 4 <= len {
                if i & 63 == 0 && stop.load(Ordering::Relaxed) {
                    return BatchCheckResult {
                        matched: None,
                        attempts,
                    };
                }
                let p0 = batch.get(i).unwrap();
                let p1 = batch.get(i + 1).unwrap();
                let p2 = batch.get(i + 2).unwrap();
                let p3 = batch.get(i + 3).unwrap();

                let mut k0 = ZipCryptoKeys::from_password(p0);
                let mut k1 = ZipCryptoKeys::from_password(p1);
                let mut k2 = ZipCryptoKeys::from_password(p2);
                let mut k3 = ZipCryptoKeys::from_password(p3);

                for &cipher in &primary.header[..11] {
                    k0.decrypt_byte(cipher);
                    k1.decrypt_byte(cipher);
                    k2.decrypt_byte(cipher);
                    k3.decrypt_byte(cipher);
                }

                let m0 = u8::from((primary.header[11] ^ k0.stream_byte()) == primary.expected_byte);
                let m1 = u8::from((primary.header[11] ^ k1.stream_byte()) == primary.expected_byte);
                let m2 = u8::from((primary.header[11] ^ k2.stream_byte()) == primary.expected_byte);
                let m3 = u8::from((primary.header[11] ^ k3.stream_byte()) == primary.expected_byte);
                let mask = m0 | (m1 << 1) | (m2 << 2) | (m3 << 3);

                if mask == 0 {
                    attempts += 4;
                    i += 4;
                    continue;
                }

                let cands = [p0, p1, p2, p3];
                for (lane, &cand) in cands.iter().enumerate() {
                    attempts += 1;
                    if (mask & (1 << lane)) != 0
                        && self.quick_check_zipcrypto_rest(cand)
                        && self.verify(cand)
                    {
                        return BatchCheckResult {
                            matched: Some(i + lane),
                            attempts,
                        };
                    }
                }
                i += 4;
            }

            while i < len {
                if stop.load(Ordering::Relaxed) {
                    break;
                }
                let pw = batch.get(i).unwrap();
                attempts += 1;
                if self.quick_check_zipcrypto(pw) && self.verify(pw) {
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

        if let Some(params) = &self.aes_check {
            let mut i = 0usize;
            while i + 4 <= len {
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
                let mask = aes_quick_check_4(p0, p1, p2, p3, params);
                if mask == 0 {
                    attempts += 4;
                    i += 4;
                    continue;
                }
                let cands = [p0, p1, p2, p3];
                for (lane, &cand) in cands.iter().enumerate() {
                    attempts += 1;
                    if (mask & (1 << lane)) != 0 && self.verify(cand) {
                        return BatchCheckResult {
                            matched: Some(i + lane),
                            attempts,
                        };
                    }
                }
                i += 4;
            }
            while i < len {
                if stop.load(Ordering::Relaxed) {
                    break;
                }
                let pw = batch.get(i).unwrap();
                attempts += 1;
                if aes_quick_check_single(pw, params) && self.verify(pw) {
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

        for (idx, pw) in batch.iter().enumerate() {
            if stop.load(Ordering::Relaxed) {
                break;
            }
            attempts += 1;
            if self.verify(pw) {
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
        if let Some(primary) = self.zipcrypto_checks.first() {
            let base_keys = ZipCryptoKeys::from_password(prefix);
            let mut pbuf = PrefixBuf::new(prefix);

            let len = last_bytes.len();
            let mut i = 0usize;
            let mut attempts = 0u64;

            while i + 4 <= len {
                if i & 63 == 0 && stop.load(Ordering::Relaxed) {
                    return BatchCheckResult {
                        matched: None,
                        attempts,
                    };
                }
                let mut k0 = base_keys;
                let mut k1 = base_keys;
                let mut k2 = base_keys;
                let mut k3 = base_keys;
                k0.update(last_bytes[i]);
                k1.update(last_bytes[i + 1]);
                k2.update(last_bytes[i + 2]);
                k3.update(last_bytes[i + 3]);

                for &cipher in &primary.header[..11] {
                    k0.decrypt_byte(cipher);
                    k1.decrypt_byte(cipher);
                    k2.decrypt_byte(cipher);
                    k3.decrypt_byte(cipher);
                }

                let m0 = u8::from((primary.header[11] ^ k0.stream_byte()) == primary.expected_byte);
                let m1 = u8::from((primary.header[11] ^ k1.stream_byte()) == primary.expected_byte);
                let m2 = u8::from((primary.header[11] ^ k2.stream_byte()) == primary.expected_byte);
                let m3 = u8::from((primary.header[11] ^ k3.stream_byte()) == primary.expected_byte);
                let mask = m0 | (m1 << 1) | (m2 << 2) | (m3 << 3);

                if mask == 0 {
                    attempts += 4;
                    i += 4;
                    continue;
                }

                for lane in 0..4 {
                    attempts += 1;
                    if (mask & (1 << lane)) != 0 {
                        let cand = pbuf.with_last(last_bytes[i + lane]);
                        if self.quick_check_zipcrypto_rest(cand) && self.verify(cand) {
                            return BatchCheckResult {
                                matched: Some(i + lane),
                                attempts,
                            };
                        }
                    }
                }
                i += 4;
            }

            while i < len {
                let mut k = base_keys;
                k.update(last_bytes[i]);
                attempts += 1;
                if k.check_header(primary) {
                    let cand = pbuf.with_last(last_bytes[i]);
                    if self.quick_check_zipcrypto_rest(cand) && self.verify(cand) {
                        return BatchCheckResult {
                            matched: Some(i),
                            attempts,
                        };
                    }
                }
                i += 1;
            }

            return BatchCheckResult {
                matched: None,
                attempts,
            };
        }

        // For AES, build batch or interleave 4-way directly when prefix fits on stack.
        if let Some(params) = &self.aes_check
            && prefix.len() < 128
        {
            let mut b0 = [0u8; 128];
            b0[..prefix.len()].copy_from_slice(prefix);
            let mut b1 = b0;
            let mut b2 = b0;
            let mut b3 = b0;
            let clen = prefix.len() + 1;
            let len = last_bytes.len();
            let mut i = 0usize;
            let mut attempts = 0u64;

            while i + 4 <= len {
                if stop.load(Ordering::Relaxed) {
                    return BatchCheckResult {
                        matched: None,
                        attempts,
                    };
                }
                b0[prefix.len()] = last_bytes[i];
                b1[prefix.len()] = last_bytes[i + 1];
                b2[prefix.len()] = last_bytes[i + 2];
                b3[prefix.len()] = last_bytes[i + 3];
                let mask =
                    aes_quick_check_4(&b0[..clen], &b1[..clen], &b2[..clen], &b3[..clen], params);
                if mask == 0 {
                    attempts += 4;
                    i += 4;
                    continue;
                }
                let slices = [&b0[..clen], &b1[..clen], &b2[..clen], &b3[..clen]];
                for (lane, &cand) in slices.iter().enumerate() {
                    attempts += 1;
                    if (mask & (1 << lane)) != 0 && self.verify(cand) {
                        return BatchCheckResult {
                            matched: Some(i + lane),
                            attempts,
                        };
                    }
                }
                i += 4;
            }
            while i < len {
                if stop.load(Ordering::Relaxed) {
                    break;
                }
                b0[prefix.len()] = last_bytes[i];
                attempts += 1;
                if aes_quick_check_single(&b0[..clen], params) && self.verify(&b0[..clen]) {
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

        // Default fallback
        let mut pbuf = PrefixBuf::new(prefix);
        let mut attempts = 0u64;
        for (idx, &b) in last_bytes.iter().enumerate() {
            if stop.load(Ordering::Relaxed) {
                break;
            }
            let cand = pbuf.with_last(b);
            attempts += 1;
            if self.quick_check(cand) && self.verify(cand) {
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
        "ZIP"
    }

    fn encryption_info(&self) -> &str {
        if self.has_aes { "AES" } else { "ZipCrypto" }
    }
}
