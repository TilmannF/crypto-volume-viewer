# Streaming Extraction

## Overview

`cryptovol extract` reads encrypted container data on demand rather than decrypting the whole volume into memory. Each filesystem backend streams file data in bounded chunks directly to a destination temp file. No decrypted copy of the file is held in RAM beyond one chunk at a time.

## Memory model: before vs after

Before this change, extraction called `fs.read_file(path) -> Vec<u8>`, allocating the full decrypted file in RAM before writing it to disk. A 2 GiB file would require 2 GiB of heap.

After this change, extraction calls `fs.read_file_to_writer(path, &mut writer)`, writing data through a fixed-size chunk buffer. Only `EXTRACTION_CHUNK_SIZE` bytes (256 KiB) plus filesystem parser metadata reside in memory at any point.

## Chunk size

`EXTRACTION_CHUNK_SIZE = 256 KiB` is an internal constant defined in `cryptovol-core`. It is not user-configurable. The value balances syscall overhead against RSS footprint. All three filesystem backends import this constant and use it as their read buffer size.

## Filesystem support

**FAT:** Cluster data is streamed cluster-by-cluster. The FAT chain for a file is traversed on demand — each cluster's next-cluster pointer is fetched from the FAT table as the stream advances. No full FAT copy is loaded into memory.

**exFAT:** Contiguous files (no-FAT-chain flag set) are read as sequential runs within the allocation bitmap. FAT-chained files traverse the exFAT FAT table one entry at a time per cluster. The FAT table itself is treated as on-demand metadata; it is not buffered in full. Note that for very large volumes the FAT table can be proportional to total capacity; a future optimization could cache a sliding window of FAT entries.

**NTFS:** Resident `$DATA` attributes are small by definition (bounded by MFT record size, typically 1 KiB–4 KiB) and are copied once from the MFT record buffer. Non-resident `$DATA` is streamed run-by-run: each runlist entry is read in `EXTRACTION_CHUNK_SIZE` chunks; no Vec proportional to file size is allocated.

## Destination safety

The canonical streaming destination writer (`StreamingWriter`/`open_streaming_writer`) and the progress- and cancellation-aware copy core now live in `cryptovol-app`, not `cryptovol-cli`. `cryptovol-cli`'s `extract` command calls into it through `VolumeSession::extract_file` rather than defining its own writer, so the CLI and any future GUI frontend share exactly one implementation of this policy. See [architecture.md](architecture.md) for the crate boundary.

A temporary file is created in the same directory as the destination via the `tempfile` crate (`NamedTempFile::new_in`). Data is written to the temp file. On success, `NamedTempFile::persist` performs an atomic rename (same-filesystem move) to the final destination path. On any failure — source read error, write error, policy violation, or caller-requested cancellation — the `NamedTempFile` is dropped without calling `persist`, which deletes the temp file automatically. The original file at the destination is not modified until the atomic rename succeeds.

## Partial-file behavior

If extraction fails mid-stream, the final destination path is either absent (new-file case) or unchanged (overwrite case, since the original is replaced only when `persist` succeeds). No partial decrypted data appears at the final path. This also holds for caller-requested cancellation: `cryptovol-app` exposes an optional `CancellationToken` that a caller can cancel mid-copy, which returns `AppError::Cancelled` and leaves no destination-named file behind.

## Why not mmap

Memory-mapping would require `unsafe` Rust, complicates cross-platform support (Windows semantics differ), and provides no meaningful advantage for sequential extraction. The chunk-buffer approach is safe, portable, and sufficient for the use case.

## Progress reporting and cancellation

`cryptovol-app`'s `VolumeSession::extract_file` accepts a progress callback and reports `ProgressEvent::{Started, Advanced, Finished}` as bytes are written to the destination, plus an optional `CancellationToken` a caller can cancel mid-copy. This is available at the library level today; `cryptovol-cli`'s `extract` command does not yet surface a `--progress` flag or expose cancellation itself, so from the CLI's perspective the byte count is still only printed after completion. A future GUI frontend is expected to use these directly for a live progress bar and a cancel button.

## Unsupported features

The following are not implemented and are out of scope for the current streaming design:

- Parallel extraction
- Directory extraction (`extract-dir`)
- Async I/O
