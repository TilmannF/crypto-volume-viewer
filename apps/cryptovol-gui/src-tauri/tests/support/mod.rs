#![allow(
    dead_code,
    missing_docs,
    clippy::expect_used,
    reason = "shared test-only synthetic TC/VC container builder"
)]

//! Builds a minimal, deterministic TC/VC-compatible container on disk for
//! tests that need a real file `cryptovol_app::open_volume` can open,
//! without depending on VeraCrypt or any committed fixture.
//!
//! This is a trimmed, local copy of the synthetic container construction in
//! `crates/cryptovol-app/tests/support/mod.rs` (itself copied from
//! `crates/cryptovol-tcvc/tests/data_reader.rs`): neither crate exports this
//! as a test-support API, and these tests need bytes written to a real file
//! (for `FileBlockReader`) rather than an in-memory reader.

use aes::cipher::KeyInit;
use aes::Aes256;
use pbkdf2::pbkdf2_hmac;
use sha2::Sha512;
use std::path::PathBuf;
use xts_mode::{get_tweak_default, Xts128};

/// Password baked into every container built by [`write_synthetic_container`].
pub const TEST_PASSWORD: &str = "correct password";

const CONTAINER_DATA_OFFSET: usize = 131_072;
const DATA_KEY: [u8; 64] = [
    0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19, 0x1a, 0x1b, 0x1c, 0x1d, 0x1e, 0x1f,
    0x20, 0x21, 0x22, 0x23, 0x24, 0x25, 0x26, 0x27, 0x28, 0x29, 0x2a, 0x2b, 0x2c, 0x2d, 0x2e, 0x2f,
    0x30, 0x31, 0x32, 0x33, 0x34, 0x35, 0x36, 0x37, 0x38, 0x39, 0x3a, 0x3b, 0x3c, 0x3d, 0x3e, 0x3f,
    0x40, 0x41, 0x42, 0x43, 0x44, 0x45, 0x46, 0x47, 0x48, 0x49, 0x4a, 0x4b, 0x4c, 0x4d, 0x4e, 0x4f,
];

/// Writes a synthetic, [`TEST_PASSWORD`]-protected TC/VC container to
/// `dir/name` and returns its path. The decrypted data area is a FAT-like
/// boot sector followed by zeroed padding, so filesystem probing reports
/// `FilesystemProbeCandidate::FatLike`.
pub fn write_synthetic_container(dir: &tempfile::TempDir, name: &str) -> PathBuf {
    let path = dir.path().join(name);
    let bytes = supported_profile_container_bytes();
    std::fs::write(&path, bytes).expect("write synthetic container");
    path
}

fn supported_profile_container_bytes() -> Vec<u8> {
    let mut container = vec![0u8; CONTAINER_DATA_OFFSET + expected_plaintext().len()];
    container[..512].copy_from_slice(&encrypted_header());
    container[CONTAINER_DATA_OFFSET..].copy_from_slice(&encrypted_data());
    container
}

fn expected_plaintext() -> [u8; 1536] {
    let mut bytes = fat_like_boot_sector_padded();
    // Mirror cryptovol-tcvc's test plaintext shape (boot-sector-like first
    // bytes followed by deterministic filler) without depending on it.
    for (index, byte) in bytes.iter_mut().enumerate().skip(512) {
        *byte = (index % 251) as u8;
    }
    bytes
}

fn fat_like_boot_sector_padded() -> [u8; 1536] {
    let mut bytes = [0u8; 1536];
    bytes[0] = 0xeb;
    bytes[1] = 0x58;
    bytes[2] = 0x90;
    bytes[3..11].copy_from_slice(b"MSDOS5.0");
    bytes[510] = 0x55;
    bytes[511] = 0xaa;
    bytes
}

fn encrypted_header() -> [u8; 512] {
    encrypt_header(valid_decrypted_header())
}

fn encrypt_header(mut header: [u8; 512]) -> [u8; 512] {
    let mut header_key = [0u8; 64];
    pbkdf2_hmac::<Sha512>(
        TEST_PASSWORD.as_bytes(),
        &header[..64],
        500_000,
        &mut header_key,
    );

    encrypt_xts_area(&header_key, &mut header[64..], 0);
    header
}

fn valid_decrypted_header() -> [u8; 512] {
    let mut header = [0u8; 512];
    header[..64].copy_from_slice(&header_salt());
    header[64..68].copy_from_slice(b"VERA");
    write_u16_be(&mut header, 68, 5);
    write_u16_be(&mut header, 70, 5);
    write_u64_be(&mut header, 92, 0);
    write_u64_be(
        &mut header,
        100,
        (CONTAINER_DATA_OFFSET + expected_plaintext().len()) as u64,
    );
    write_u64_be(&mut header, 108, CONTAINER_DATA_OFFSET as u64);
    write_u64_be(&mut header, 116, expected_plaintext().len() as u64);
    write_u32_be(&mut header, 124, 0);
    write_u32_be(&mut header, 128, 512);
    header[256..320].copy_from_slice(&DATA_KEY);
    header[320..512].fill(0xa5);
    refresh_header_crc(&mut header);
    header
}

fn encrypted_data() -> [u8; 1536] {
    let mut data = expected_plaintext();
    encrypt_xts_area(&DATA_KEY, &mut data, (CONTAINER_DATA_OFFSET / 512) as u64);
    data
}

fn encrypt_xts_area(key: &[u8; 64], payload: &mut [u8], first_sector_index: u64) {
    let mut key_1 = [0u8; 32];
    let mut key_2 = [0u8; 32];
    key_1.copy_from_slice(&key[..32]);
    key_2.copy_from_slice(&key[32..]);

    let cipher_1 = Aes256::new(&key_1.into());
    let cipher_2 = Aes256::new(&key_2.into());
    let xts = Xts128::<Aes256>::new(cipher_1, cipher_2);

    xts.encrypt_area(
        payload,
        512,
        u128::from(first_sector_index),
        get_tweak_default,
    );
}

fn header_salt() -> [u8; 64] {
    let mut salt = [0u8; 64];
    for (index, byte) in salt.iter_mut().enumerate() {
        *byte = 0x80 | index as u8;
    }
    salt
}

fn refresh_header_crc(header: &mut [u8; 512]) {
    let key_area_crc = crc32fast::hash(&header[256..512]);
    write_u32_be(header, 72, key_area_crc);

    let metadata_crc = crc32fast::hash(&header[64..252]);
    write_u32_be(header, 252, metadata_crc);
}

fn write_u16_be(buf: &mut [u8], offset: usize, value: u16) {
    buf[offset..offset + 2].copy_from_slice(&value.to_be_bytes());
}

fn write_u32_be(buf: &mut [u8], offset: usize, value: u32) {
    buf[offset..offset + 4].copy_from_slice(&value.to_be_bytes());
}

fn write_u64_be(buf: &mut [u8], offset: usize, value: u64) {
    buf[offset..offset + 8].copy_from_slice(&value.to_be_bytes());
}
