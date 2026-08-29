//! Integration-level unit tests for NtfsFileSystem.

#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use cryptovol_core::{BlockReader, CryptovolError};
use cryptovol_fs_ntfs::{NtfsError, NtfsFileSystem};

// ── In-memory block reader ─────────────────────────────────────────────────────

struct MemReader(Vec<u8>);

impl BlockReader for MemReader {
    fn len(&self) -> u64 {
        self.0.len() as u64
    }

    fn read_at(&self, offset: u64, buf: &mut [u8]) -> Result<(), CryptovolError> {
        let start = usize::try_from(offset).map_err(|_| CryptovolError::OutOfBounds {
            offset,
            length: buf.len(),
            file_len: self.0.len() as u64,
        })?;
        let end = start
            .checked_add(buf.len())
            .ok_or(CryptovolError::OutOfBounds {
                offset,
                length: buf.len(),
                file_len: self.0.len() as u64,
            })?;
        if end > self.0.len() {
            return Err(CryptovolError::OutOfBounds {
                offset,
                length: buf.len(),
                file_len: self.0.len() as u64,
            });
        }
        buf.copy_from_slice(&self.0[start..end]);
        Ok(())
    }
}

// ── Volume builder helpers ─────────────────────────────────────────────────────

fn w16(v: &mut [u8], off: usize, val: u16) {
    v[off..off + 2].copy_from_slice(&val.to_le_bytes());
}
fn w32(v: &mut [u8], off: usize, val: u32) {
    v[off..off + 4].copy_from_slice(&val.to_le_bytes());
}
fn w64(v: &mut [u8], off: usize, val: u64) {
    v[off..off + 8].copy_from_slice(&val.to_le_bytes());
}

/// Writes the standard 512-byte FILE record header at `off`.
///
/// USA_n = 1, fixup[1] = 0x0000; sector endpoint at off+510 is set to USA_n.
fn file_record_header(vol: &mut [u8], off: usize, flags: u16) {
    vol[off..off + 4].copy_from_slice(b"FILE");
    w16(vol, off + 4, 48); // usa_offset = 48
    w16(vol, off + 6, 2); // usa_count = 2 (1 seq + 1 fixup for 1 sector)
    w16(vol, off + 16, 1); // sequence_number = 1
    w16(vol, off + 18, 1); // link_count = 1
    w16(vol, off + 20, 52); // first_attr_offset = 48 + 2*2
    w16(vol, off + 22, flags);
    w32(vol, off + 24, 512); // real_size
    w32(vol, off + 28, 512); // allocated_size
    w16(vol, off + 40, 2); // next_attr_id
    w16(vol, off + 48, 1); // usa_n = 1
                           // fixup[1] at off+50 = 0x0000 (already zero — restored value for sector 1 endpoint)
                           // apply USA: sector 1 endpoint (off+510..off+512) = usa_n
    vol[off + 510] = 1;
    vol[off + 511] = 0;
}

/// Writes a resident unnamed attribute at `off`. Returns the padded total byte count.
fn write_attr_res(vol: &mut [u8], off: usize, attr_type: u32, attr_id: u16, value: &[u8]) -> usize {
    let vlen = value.len();
    let total = (24 + vlen + 7) & !7;
    w32(vol, off, attr_type);
    w32(vol, off + 4, total as u32);
    vol[off + 8] = 0; // non-resident = false
    vol[off + 9] = 0; // name_length = 0
    w16(vol, off + 10, 0x18); // name_offset = 24
                              // flags at off+12 = 0
    w16(vol, off + 14, attr_id);
    w32(vol, off + 16, vlen as u32);
    w16(vol, off + 20, 0x18); // value_offset = 24
                              // indexed_flag at off+22 = 0; reserved at off+23 = 0
    vol[off + 24..off + 24 + vlen].copy_from_slice(value);
    total
}

/// Writes a $STANDARD_INFORMATION attribute (48 zero bytes). Returns byte count.
fn write_si(vol: &mut [u8], off: usize, attr_id: u16) -> usize {
    write_attr_res(vol, off, 0x10, attr_id, &[0u8; 48])
}

/// Builds a $FILE_NAME attribute value.
fn make_fn_value(
    parent_mft: u64,
    data_size: u64,
    file_attrs: u32,
    namespace: u8,
    name: &str,
) -> Vec<u8> {
    let utf16: Vec<u16> = name.encode_utf16().collect();
    let n = utf16.len();
    let mut v = vec![0u8; 66 + n * 2];
    let parent_ref = (1u64 << 48) | parent_mft;
    v[0..8].copy_from_slice(&parent_ref.to_le_bytes());
    // timestamps (8..40) = 0; allocated_size (40..48) = 0
    v[48..56].copy_from_slice(&data_size.to_le_bytes());
    v[56..60].copy_from_slice(&file_attrs.to_le_bytes());
    // reparse (60..64) = 0
    v[64] = n as u8;
    v[65] = namespace;
    for (i, &unit) in utf16.iter().enumerate() {
        v[66 + i * 2] = (unit & 0xFF) as u8;
        v[66 + i * 2 + 1] = (unit >> 8) as u8;
    }
    v
}

/// Builds a $INDEX_ROOT attribute value with the given directory entries.
///
/// Each entry: `(name, mft_record_number, is_dir, namespace)`.
/// Uses the correct NTFS indexed-data length: 66 + filename_length*2.
fn make_index_root(entries: &[(&str, u64, bool, u8)]) -> Vec<u8> {
    let mut entry_bytes: Vec<u8> = Vec::new();
    for &(name, mft_num, is_dir, ns) in entries {
        let utf16: Vec<u16> = name.encode_utf16().collect();
        let n = utf16.len();
        let idx_data_len = 66 + n * 2;
        let entry_raw = 16 + idx_data_len;
        let entry_len = (entry_raw + 7) & !7;
        let mut e = vec![0u8; entry_len];
        let file_ref = (1u64 << 48) | mft_num;
        e[0..8].copy_from_slice(&file_ref.to_le_bytes());
        e[8..10].copy_from_slice(&(entry_len as u16).to_le_bytes());
        e[10..12].copy_from_slice(&(idx_data_len as u16).to_le_bytes());
        // flags at e[12..14] = 0 (not last entry)
        // indexed $FILE_NAME data at e[16..16+idx_data_len]:
        //   data[48..56] = data_size = e[64..72]
        //   data[56..60] = file_attributes = e[72..76]
        //   data[64]     = filename_length = e[80]
        //   data[65]     = namespace = e[81]
        //   data[66..]   = filename UTF-16LE = e[82..]
        let file_attrs: u32 = if is_dir { 0x10 } else { 0 };
        e[72..76].copy_from_slice(&file_attrs.to_le_bytes());
        e[80] = n as u8;
        e[81] = ns;
        for (i, &unit) in utf16.iter().enumerate() {
            e[82 + i * 2] = (unit & 0xFF) as u8;
            e[82 + i * 2 + 1] = (unit >> 8) as u8;
        }
        entry_bytes.extend_from_slice(&e);
    }
    // Last-entry marker
    let mut last = vec![0u8; 16];
    last[8..10].copy_from_slice(&16u16.to_le_bytes());
    last[12..14].copy_from_slice(&2u16.to_le_bytes()); // flags: last-entry bit
    entry_bytes.extend_from_slice(&last);

    let entries_total = entry_bytes.len() as u32;
    let mut root = vec![0u8; 32];
    // Root header (bytes 0-15):
    root[0..4].copy_from_slice(&0x30u32.to_le_bytes()); // indexed attr type = $FILE_NAME
    root[4..8].copy_from_slice(&1u32.to_le_bytes()); // collation rule
    root[8..12].copy_from_slice(&4096u32.to_le_bytes()); // index buffer byte size
    root[12] = 1; // clusters per index buffer
                  // Node header (bytes 16-31):
    root[16..20].copy_from_slice(&16u32.to_le_bytes()); // first_entry_offset from node header
    root[20..24].copy_from_slice(&(16 + entries_total).to_le_bytes());
    root[24..28].copy_from_slice(&(16 + entries_total).to_le_bytes());
    // root[28] = 0 (leaf node)
    root.extend_from_slice(&entry_bytes);
    root
}

/// Writes a non-resident $DATA attribute at `off`. Returns total byte count.
fn write_data_nonres(
    vol: &mut [u8],
    off: usize,
    attr_id: u16,
    highest_vcn: u64,
    data_size: u64,
    runlist: &[u8],
) -> usize {
    let mut rl = Vec::with_capacity(runlist.len() + 1);
    rl.extend_from_slice(runlist);
    rl.push(0x00); // end marker
    let rl_padded = (rl.len() + 7) & !7;
    let total = 64 + rl_padded;

    w32(vol, off, 0x80); // type = $DATA
    w32(vol, off + 4, total as u32);
    vol[off + 8] = 1; // non-resident
    vol[off + 9] = 0; // name_length = 0
    w16(vol, off + 10, 0x40); // name_offset = 64
                              // flags at off+12 = 0
    w16(vol, off + 14, attr_id);
    w64(vol, off + 16, 0); // lowest_vcn = 0
    w64(vol, off + 24, highest_vcn);
    w16(vol, off + 32, 0x40); // data_runs_offset = 64
                              // compression_unit at off+34 = 0; padding at off+36-39 = 0
    w64(vol, off + 40, data_size); // allocated_size
    w64(vol, off + 48, data_size); // data_size
    w64(vol, off + 56, data_size); // valid_data_length
    vol[off + 64..off + 64 + rl.len()].copy_from_slice(&rl);
    total
}

/// Builds a structurally valid 65536-byte in-memory NTFS volume.
///
/// Geometry: 512 bytes/sector, 1 sector/cluster, 128 sectors total.
/// MFT at cluster 4 (byte 2048), 512-byte file records.
///
/// Records:
///   0 = $MFT        1–4 = stubs
///   5 = root "/"    6 = hello.txt    7 = Folder With Spaces
///   8 = Rocket Science 🚀 For Beginners.txt
fn build_minimal_ntfs() -> Vec<u8> {
    let mut vol = vec![0u8; 65536];

    // ── Boot sector ────────────────────────────────────────────────────────
    vol[3..11].copy_from_slice(b"NTFS    ");
    w16(&mut vol, 11, 512); // BytesPerSector
    vol[13] = 1; // SectorsPerCluster
    w64(&mut vol, 40, 127); // TotalSectors
    w64(&mut vol, 48, 4); // MftLcn = cluster 4 = byte 2048
    w64(&mut vol, 56, 32); // MftMirrorLcn
    vol[64] = 0xF7; // ClustersPerFileRecordSegment = -9 → 2^9 = 512 bytes
    vol[68] = 0xF7; // ClustersPerIndexBuffer = -9 → 512 bytes
    w64(&mut vol, 72, 0xDEAD_BEEF_CAFE_BABE); // VolumeSerialNumber
    vol[510] = 0x55;
    vol[511] = 0xAA;

    // ── MFT record 0 ($MFT) at byte 2048 ──────────────────────────────────
    {
        let base = 2048_usize;
        file_record_header(&mut vol, base, 1);
        let mut pos = base + 52;
        pos += write_si(&mut vol, pos, 0);
        let fn0 = make_fn_value(5, 0, 0x06, 3, "$MFT");
        pos += write_attr_res(&mut vol, pos, 0x30, 1, &fn0);
        // Non-resident $DATA: one run at LCN=4, length=9 clusters
        // Runlist [0x11, 0x09, 0x04]: header=0x11 (len_size=1,off_size=1), len=9, delta=+4
        pos += write_data_nonres(&mut vol, pos, 2, 8, 4608, &[0x11, 0x09, 0x04]);
        w32(&mut vol, pos, 0xFFFF_FFFF); // end-of-attributes
    }

    // ── MFT records 1–4: minimal stubs ────────────────────────────────────
    for i in 1_usize..=4 {
        let base = 2048 + i * 512;
        file_record_header(&mut vol, base, 1);
        w32(&mut vol, base + 52, 0xFFFF_FFFF);
    }

    // ── MFT record 5 (root directory) at byte 4608 ────────────────────────
    {
        let base = 2048 + 5 * 512; // = 4608
        file_record_header(&mut vol, base, 3); // in-use | directory
        let mut pos = base + 52;
        pos += write_si(&mut vol, pos, 0);
        let ix5 = make_index_root(&[
            ("hello.txt", 6, false, 1),
            ("Folder With Spaces", 7, true, 1),
        ]);
        pos += write_attr_res(&mut vol, pos, 0x90, 1, &ix5);
        w32(&mut vol, pos, 0xFFFF_FFFF);
    }

    // ── MFT record 6 (hello.txt) at byte 5120 ─────────────────────────────
    {
        let base = 2048 + 6 * 512; // = 5120
        file_record_header(&mut vol, base, 1);
        let mut pos = base + 52;
        pos += write_si(&mut vol, pos, 0);
        let fn6 = make_fn_value(5, 5, 0x20, 1, "hello.txt");
        pos += write_attr_res(&mut vol, pos, 0x30, 1, &fn6);
        pos += write_attr_res(&mut vol, pos, 0x80, 2, b"hello");
        w32(&mut vol, pos, 0xFFFF_FFFF);
    }

    // ── MFT record 7 (Folder With Spaces) at byte 5632 ────────────────────
    {
        let base = 2048 + 7 * 512; // = 5632
        file_record_header(&mut vol, base, 3);
        let mut pos = base + 52;
        pos += write_si(&mut vol, pos, 0);
        let fn7 = make_fn_value(5, 0, 0x10, 1, "Folder With Spaces");
        pos += write_attr_res(&mut vol, pos, 0x30, 1, &fn7);
        let ix7 = make_index_root(&[("Rocket Science \u{1F680} For Beginners.txt", 8, false, 1)]);
        pos += write_attr_res(&mut vol, pos, 0x90, 2, &ix7);
        w32(&mut vol, pos, 0xFFFF_FFFF);
    }

    // ── MFT record 8 (Rocket Science file) at byte 6144 ───────────────────
    {
        let base = 2048 + 8 * 512; // = 6144
        file_record_header(&mut vol, base, 1);
        let mut pos = base + 52;
        pos += write_si(&mut vol, pos, 0);
        let fn8 = make_fn_value(7, 14, 0x20, 1, "Rocket Science \u{1F680} For Beginners.txt");
        pos += write_attr_res(&mut vol, pos, 0x30, 1, &fn8);
        pos += write_attr_res(&mut vol, pos, 0x80, 2, b"rocket science");
        w32(&mut vol, pos, 0xFFFF_FFFF);
    }

    vol
}

// ── Tests ──────────────────────────────────────────────────────────────────────

#[test]
fn ntfs_open_succeeds() {
    let result = NtfsFileSystem::open(MemReader(build_minimal_ntfs()));
    assert!(
        result.is_ok(),
        "open should succeed, got: {:?}",
        result.err()
    );
}

#[test]
fn ntfs_list_root_returns_two_entries() {
    let fs = NtfsFileSystem::open(MemReader(build_minimal_ntfs())).expect("open must succeed");
    let entries = fs.list_dir("/").expect("list_dir should succeed");
    assert_eq!(
        entries.len(),
        2,
        "root must have exactly 2 entries, got {}",
        entries.len()
    );
    let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
    assert!(
        names.contains(&"hello.txt"),
        "root must contain hello.txt; got {:?}",
        names
    );
    assert!(
        names.contains(&"Folder With Spaces"),
        "root must contain Folder With Spaces; got {:?}",
        names
    );
}

#[test]
fn ntfs_stat_file() {
    let fs = NtfsFileSystem::open(MemReader(build_minimal_ntfs())).expect("open must succeed");
    let entry = fs.stat("/hello.txt").expect("stat should succeed");
    assert_eq!(entry.name, "hello.txt", "entry name must be hello.txt");
    assert!(!entry.is_dir, "hello.txt must not be a directory");
    assert_eq!(entry.size, 5, "hello.txt size must be 5 bytes");
}

#[test]
fn ntfs_stat_directory() {
    let fs = NtfsFileSystem::open(MemReader(build_minimal_ntfs())).expect("open must succeed");
    let entry = fs.stat("/Folder With Spaces").expect("stat should succeed");
    assert!(entry.is_dir, "Folder With Spaces must be a directory");
}

#[test]
fn ntfs_list_nested_dir() {
    let fs = NtfsFileSystem::open(MemReader(build_minimal_ntfs())).expect("open must succeed");
    let entries = fs
        .list_dir("/Folder With Spaces")
        .expect("list_dir should succeed");
    assert_eq!(
        entries.len(),
        1,
        "nested dir must have 1 entry, got {}",
        entries.len()
    );
    assert_eq!(
        entries[0].name, "Rocket Science \u{1F680} For Beginners.txt",
        "nested entry name must match"
    );
}

#[test]
fn ntfs_stat_nested_path() {
    let fs = NtfsFileSystem::open(MemReader(build_minimal_ntfs())).expect("open must succeed");
    let path = "/Folder With Spaces/Rocket Science \u{1F680} For Beginners.txt";
    let entry = fs.stat(path).expect("stat on nested path should succeed");
    assert_eq!(entry.size, 14, "nested file size must be 14 bytes");
}

#[test]
fn ntfs_stat_nonexistent_returns_not_found() {
    let fs = NtfsFileSystem::open(MemReader(build_minimal_ntfs())).expect("open must succeed");
    // A real file must be findable first (this assertion fails in stub state).
    let found = fs.stat("/hello.txt");
    assert!(
        found.is_ok(),
        "stat on existing file must succeed, got: {:?}",
        found.err()
    );
    // A nonexistent path must return PathNotFound.
    let missing = fs.stat("/nonexistent.txt");
    assert!(
        matches!(missing, Err(NtfsError::PathNotFound { .. })),
        "nonexistent path must return PathNotFound, got: {missing:?}"
    );
}

#[test]
fn ntfs_read_file_returns_correct_bytes() {
    let fs = NtfsFileSystem::open(MemReader(build_minimal_ntfs())).expect("open must succeed");
    let data = fs
        .read_file("/hello.txt")
        .expect("read_file should succeed");
    assert_eq!(data, b"hello", "file content must be b\"hello\"");
}

#[test]
fn ntfs_read_file_nested() {
    let fs = NtfsFileSystem::open(MemReader(build_minimal_ntfs())).expect("open must succeed");
    let path = "/Folder With Spaces/Rocket Science \u{1F680} For Beginners.txt";
    let data = fs
        .read_file(path)
        .expect("read_file on nested path should succeed");
    assert_eq!(
        data, b"rocket science",
        "nested file content must be b\"rocket science\""
    );
}

#[test]
fn ntfs_read_file_directory_rejected() {
    let fs = NtfsFileSystem::open(MemReader(build_minimal_ntfs())).expect("open must succeed");
    let result = fs.read_file("/Folder With Spaces");
    assert!(
        matches!(result, Err(NtfsError::AttemptedDirectoryExtraction { .. })),
        "reading a directory must return AttemptedDirectoryExtraction, got: {result:?}"
    );
}
