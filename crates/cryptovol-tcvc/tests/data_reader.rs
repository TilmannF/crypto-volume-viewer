#![allow(
    missing_docs,
    clippy::expect_used,
    clippy::panic,
    reason = "TC/VC data reader tests build synthetic fixtures with direct assertions"
)]

use aes::cipher::KeyInit;
use aes::Aes256;
use cryptovol_core::{BlockReader, CryptovolError, FileBlockReader};
use cryptovol_tcvc::{
    open_aes_sha512_volume, probe_filesystem, FilesystemProbeCandidate, TcvcOpenError, TcvcOpenInfo,
};
use pbkdf2::pbkdf2_hmac;
use sha2::Sha512;
use std::env;
use std::sync::OnceLock;
use xts_mode::{get_tweak_default, Xts128};

const CONTAINER_DATA_OFFSET: usize = 131_072;
const DATA_KEY: [u8; 64] = [
    0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19, 0x1a, 0x1b, 0x1c, 0x1d, 0x1e, 0x1f,
    0x20, 0x21, 0x22, 0x23, 0x24, 0x25, 0x26, 0x27, 0x28, 0x29, 0x2a, 0x2b, 0x2c, 0x2d, 0x2e, 0x2f,
    0x30, 0x31, 0x32, 0x33, 0x34, 0x35, 0x36, 0x37, 0x38, 0x39, 0x3a, 0x3b, 0x3c, 0x3d, 0x3e, 0x3f,
    0x40, 0x41, 0x42, 0x43, 0x44, 0x45, 0x46, 0x47, 0x48, 0x49, 0x4a, 0x4b, 0x4c, 0x4d, 0x4e, 0x4f,
];

#[derive(Debug)]
struct MemoryReader {
    data: Vec<u8>,
}

impl MemoryReader {
    fn new(data: Vec<u8>) -> Self {
        Self { data }
    }
}

impl BlockReader for MemoryReader {
    fn len(&self) -> u64 {
        self.data.len() as u64
    }

    fn read_at(&self, offset: u64, buf: &mut [u8]) -> Result<(), CryptovolError> {
        let start = usize::try_from(offset).map_err(|_| CryptovolError::OutOfBounds {
            offset,
            length: buf.len(),
            file_len: self.len(),
        })?;
        let end = start
            .checked_add(buf.len())
            .ok_or(CryptovolError::OutOfBounds {
                offset,
                length: buf.len(),
                file_len: self.len(),
            })?;

        if end > self.data.len() {
            return Err(CryptovolError::OutOfBounds {
                offset,
                length: buf.len(),
                file_len: self.len(),
            });
        }

        buf.copy_from_slice(&self.data[start..end]);
        Ok(())
    }
}

#[test]
fn opened_volume_exposes_safe_metadata_and_data_reader() {
    let encrypted = supported_profile_container_bytes();
    let encrypted_reader = MemoryReader::new(encrypted);

    let opened = open_aes_sha512_volume(&encrypted_reader, b"test-password")
        .expect("supported profile should open");
    let metadata: &TcvcOpenInfo = opened.metadata();

    assert_eq!(metadata.backend, "tcvc");
    assert_eq!(metadata.profile, "tcvc-aes-sha512-basic");
    assert!(metadata.read_only);
    assert_eq!(metadata.data_offset, 131_072);
    assert_eq!(metadata.data_length, 1536);
    assert_eq!(metadata.sector_size, 512);

    let data_reader = opened
        .data_reader(&encrypted_reader)
        .expect("opened volume should create a decrypted data reader");

    assert_eq!(data_reader.len(), metadata.data_length);
}

#[test]
fn data_reader_reads_aligned_sector() {
    let encrypted = supported_profile_container_bytes();
    let encrypted_reader = MemoryReader::new(encrypted);
    let opened = open_aes_sha512_volume(&encrypted_reader, b"test-password")
        .expect("supported profile should open");
    let data_reader = opened
        .data_reader(&encrypted_reader)
        .expect("opened volume should create a decrypted data reader");

    let mut sector = [0u8; 512];
    data_reader
        .read_at(0, &mut sector)
        .expect("aligned sector read should succeed");

    assert_eq!(sector, expected_plaintext()[0..512]);
}

#[test]
fn data_reader_reads_unaligned_range_inside_one_sector() {
    let encrypted = supported_profile_container_bytes();
    let encrypted_reader = MemoryReader::new(encrypted);
    let opened = open_aes_sha512_volume(&encrypted_reader, b"test-password")
        .expect("supported profile should open");
    let data_reader = opened
        .data_reader(&encrypted_reader)
        .expect("opened volume should create a decrypted data reader");

    let mut bytes = [0u8; 37];
    data_reader
        .read_at(19, &mut bytes)
        .expect("unaligned single-sector read should succeed");

    assert_eq!(bytes, expected_plaintext()[19..56]);
}

#[test]
fn data_reader_reads_range_crossing_two_sectors() {
    let encrypted = supported_profile_container_bytes();
    let encrypted_reader = MemoryReader::new(encrypted);
    let opened = open_aes_sha512_volume(&encrypted_reader, b"test-password")
        .expect("supported profile should open");
    let data_reader = opened
        .data_reader(&encrypted_reader)
        .expect("opened volume should create a decrypted data reader");

    let mut bytes = [0u8; 80];
    data_reader
        .read_at(480, &mut bytes)
        .expect("sector-crossing read should succeed");

    assert_eq!(bytes, expected_plaintext()[480..560]);
}

#[test]
fn data_reader_allows_zero_length_reads_up_to_len() {
    let encrypted = supported_profile_container_bytes();
    let encrypted_reader = MemoryReader::new(encrypted);
    let opened = open_aes_sha512_volume(&encrypted_reader, b"test-password")
        .expect("supported profile should open");
    let data_reader = opened
        .data_reader(&encrypted_reader)
        .expect("opened volume should create a decrypted data reader");

    let mut empty = [];

    data_reader
        .read_at(0, &mut empty)
        .expect("zero-length read at start should succeed");
    data_reader
        .read_at(data_reader.len(), &mut empty)
        .expect("zero-length read at EOF should succeed");
}

#[test]
fn data_reader_reads_near_eof() {
    let encrypted = supported_profile_container_bytes();
    let encrypted_reader = MemoryReader::new(encrypted);
    let opened = open_aes_sha512_volume(&encrypted_reader, b"test-password")
        .expect("supported profile should open");
    let data_reader = opened
        .data_reader(&encrypted_reader)
        .expect("opened volume should create a decrypted data reader");

    let mut tail = [0u8; 13];
    let offset = data_reader.len() - tail.len() as u64;
    data_reader
        .read_at(offset, &mut tail)
        .expect("near-EOF read should succeed");

    assert_eq!(
        tail,
        expected_plaintext()[expected_plaintext().len() - 13..]
    );
}

#[test]
fn data_reader_rejects_out_of_bounds_reads() {
    let encrypted = supported_profile_container_bytes();
    let encrypted_reader = MemoryReader::new(encrypted);
    let opened = open_aes_sha512_volume(&encrypted_reader, b"test-password")
        .expect("supported profile should open");
    let data_reader = opened
        .data_reader(&encrypted_reader)
        .expect("opened volume should create a decrypted data reader");

    let mut one_byte = [0u8; 1];
    let err = data_reader
        .read_at(data_reader.len(), &mut one_byte)
        .expect_err("non-empty read at EOF should fail");

    assert!(matches!(err, CryptovolError::OutOfBounds { .. }));
}

#[test]
fn data_reader_rejects_unaligned_data_area() {
    let mut header = valid_decrypted_header();
    write_u64_be(&mut header, 108, (CONTAINER_DATA_OFFSET + 1) as u64);
    refresh_header_crc(&mut header);
    let encrypted_reader = MemoryReader::new(container_with_decrypted_header(header, 0));
    let opened = open_aes_sha512_volume(&encrypted_reader, b"test-password")
        .expect("header with unaligned data area should still authenticate");

    let error = opened
        .data_reader(&encrypted_reader)
        .expect_err("unaligned data area should not create a data reader");

    assert!(matches!(error, TcvcOpenError::UnsupportedProfile { .. }));
    assert_error_formatting_is_non_secret(&error);
}

#[test]
fn data_reader_rejects_data_area_beyond_container() {
    let mut header = valid_decrypted_header();
    write_u64_be(&mut header, 116, (expected_plaintext().len() + 512) as u64);
    refresh_header_crc(&mut header);
    let encrypted_reader = MemoryReader::new(container_with_decrypted_header(header, 0));
    let opened = open_aes_sha512_volume(&encrypted_reader, b"test-password")
        .expect("header with oversized data area should still authenticate");

    let error = opened
        .data_reader(&encrypted_reader)
        .expect_err("data area beyond container should not create a data reader");

    assert!(matches!(error, TcvcOpenError::UnsupportedProfile { .. }));
    assert_error_formatting_is_non_secret(&error);
}

#[test]
#[ignore = "requires scripts/test-with-veracrypt-fixtures.sh to generate CRYPTOVOL_TEST_CONTAINER"]
fn generated_fixture_reads_first_decrypted_sector_and_probes_fat_like() {
    let container = match env::var("CRYPTOVOL_TEST_CONTAINER") {
        Ok(c) => c,
        Err(_) => {
            eprintln!("skipped: CRYPTOVOL_TEST_CONTAINER not set");
            return;
        }
    };
    let encrypted_reader = FileBlockReader::open(container).expect("fixture should open read-only");
    let opened = open_aes_sha512_volume(&encrypted_reader, b"test-password")
        .expect("generated fixture should open");
    let metadata = opened.metadata();
    let data_reader = opened
        .data_reader(&encrypted_reader)
        .expect("fixture should expose decrypted data reader");

    let mut decrypted_sector = [0u8; 512];
    data_reader
        .read_at(0, &mut decrypted_sector)
        .expect("first decrypted sector should be readable");

    assert!(
        decrypted_sector.iter().any(|byte| *byte != 0),
        "first decrypted sector should not be all zero"
    );

    let mut encrypted_sector = [0u8; 512];
    encrypted_reader
        .read_at(metadata.data_offset, &mut encrypted_sector)
        .expect("first encrypted data sector should be readable");
    assert_ne!(
        decrypted_sector, encrypted_sector,
        "decrypted first sector should differ from encrypted container bytes"
    );

    let candidate =
        probe_filesystem(&data_reader).expect("filesystem probe should read first sector");

    assert_eq!(candidate, FilesystemProbeCandidate::FatLike);
}

#[test]
fn filesystem_probe_reports_fat_like_for_fat_boot_sector() {
    let reader = MemoryReader::new(fat_like_boot_sector().to_vec());

    let candidate = probe_filesystem(&reader).expect("probe should read synthetic sector");

    assert_eq!(candidate, FilesystemProbeCandidate::FatLike);
}

#[test]
fn filesystem_probe_reports_unknown_for_unrecognized_sector() {
    let reader = MemoryReader::new(vec![0x7b; 512]);

    let candidate = probe_filesystem(&reader).expect("probe should read synthetic sector");

    assert_eq!(candidate, FilesystemProbeCandidate::Unknown);
}

#[test]
fn probe_fs_detects_exfat_oem_name() {
    let mut sector = [0u8; 512];
    sector[3..11].copy_from_slice(b"EXFAT   ");
    let reader = MemoryReader::new(sector.to_vec());

    let candidate = probe_filesystem(&reader).expect("probe should read synthetic sector");

    assert_eq!(candidate, FilesystemProbeCandidate::ExFat);
}

#[test]
fn probe_fs_detects_ntfs_oem_id() {
    let mut sector = [0u8; 512];
    sector[3..11].copy_from_slice(b"NTFS    ");
    let reader = MemoryReader::new(sector.to_vec());

    let candidate = probe_filesystem(&reader).expect("probe should read synthetic sector");

    assert_eq!(candidate, FilesystemProbeCandidate::Ntfs);
}

#[test]
fn probe_fs_ntfs_does_not_shadow_exfat() {
    let mut sector = [0u8; 512];
    sector[3..11].copy_from_slice(b"EXFAT   ");
    let reader = MemoryReader::new(sector.to_vec());

    let candidate = probe_filesystem(&reader).expect("probe should read synthetic sector");

    assert_eq!(candidate, FilesystemProbeCandidate::ExFat);
}

#[test]
fn probe_fs_does_not_detect_fat_as_exfat() {
    let reader = MemoryReader::new(fat_like_boot_sector().to_vec());

    let candidate = probe_filesystem(&reader).expect("probe should read synthetic sector");

    assert_eq!(candidate, FilesystemProbeCandidate::FatLike);
}

#[test]
fn data_reader_errors_do_not_expose_secrets() {
    let encrypted = supported_profile_container_bytes();
    let encrypted_reader = MemoryReader::new(encrypted);
    let error = open_aes_sha512_volume(&encrypted_reader, b"wrong-test-password")
        .expect_err("wrong password should fail");

    assert_error_formatting_is_non_secret(&error);
}

#[test]
fn opened_volume_debug_output_does_not_expose_secrets() {
    let encrypted = supported_profile_container_bytes();
    let encrypted_reader = MemoryReader::new(encrypted);
    let opened = open_aes_sha512_volume(&encrypted_reader, b"test-password")
        .expect("supported profile should open");

    let debug = format!("{opened:?}");

    for secret in [
        "test-password",
        "wrong-test-password",
        "header key",
        "master key",
        "data key",
        "raw decrypted",
        "aaaaaaaaaaaaaaaa",
    ] {
        assert!(
            !debug.contains(secret),
            "opened-volume debug output should not expose secret marker {secret:?}: {debug}"
        );
    }
}

fn assert_error_formatting_is_non_secret(error: &TcvcOpenError) {
    let display = error.to_string();
    let debug = format!("{error:?}");

    for rendered in [display, debug] {
        for secret in [
            "test-password",
            "wrong-test-password",
            "header key",
            "master key",
            "data key",
            "raw decrypted",
            "aaaaaaaaaaaaaaaa",
        ] {
            assert!(
                !rendered.contains(secret),
                "error output should not expose secret marker {secret:?}: {rendered}"
            );
        }
    }
}

fn supported_profile_container_bytes() -> Vec<u8> {
    static CONTAINER: OnceLock<Vec<u8>> = OnceLock::new();

    CONTAINER
        .get_or_init(|| {
            let mut container = vec![0u8; 131_072 + expected_plaintext().len()];
            container[..512].copy_from_slice(&encrypted_header());
            container[CONTAINER_DATA_OFFSET..].copy_from_slice(&encrypted_data());
            container
        })
        .clone()
}

fn expected_plaintext() -> &'static [u8; 1536] {
    static PLAINTEXT: [u8; 1536] = build_plaintext();
    &PLAINTEXT
}

fn fat_like_boot_sector() -> [u8; 512] {
    let mut sector = [0u8; 512];
    sector[0] = 0xeb;
    sector[1] = 0x58;
    sector[2] = 0x90;
    sector[3..11].copy_from_slice(b"MSDOS5.0");
    sector[510] = 0x55;
    sector[511] = 0xaa;
    sector
}

const fn build_plaintext() -> [u8; 1536] {
    let mut bytes = [0u8; 1536];
    let mut index = 0;

    while index < bytes.len() {
        bytes[index] = (index % 251) as u8;
        index += 1;
    }

    bytes
}

fn encrypted_header() -> [u8; 512] {
    encrypt_header(valid_decrypted_header())
}

fn encrypt_header(mut header: [u8; 512]) -> [u8; 512] {
    let mut header_key = [0u8; 64];
    pbkdf2_hmac::<Sha512>(b"test-password", &header[..64], 500_000, &mut header_key);

    encrypt_xts_area(&header_key, &mut header[64..], 0);
    header
}

fn container_with_decrypted_header(header: [u8; 512], data_len: usize) -> Vec<u8> {
    let mut container = vec![0u8; CONTAINER_DATA_OFFSET + data_len];
    container[..512].copy_from_slice(&encrypt_header(header));
    container
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
    let mut data = *expected_plaintext();
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
