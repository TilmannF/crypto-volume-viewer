#![allow(
    missing_docs,
    clippy::expect_used,
    reason = "DTO mapping tests construct app-core values directly and assert field-by-field"
)]

//! Verifies GUI DTOs round-trip `cryptovol_app`'s safe, non-secret types by
//! value (not by `Debug` formatting), and that serializing response/DTO
//! types never reproduces the test password.

use cryptovol_app::{
    AppTimestamp, FileAttributes, FileEntry, FilesystemKind, HeaderCandidateRole, PimState,
    TcvcKdf, VolumeInfo,
};
use cryptovol_gui_lib::dto::file_entry::FileEntryDto;
use cryptovol_gui_lib::dto::session::{OpenContainerResponseDto, VolumeInfoDto};
use std::path::PathBuf;

const TEST_PASSWORD: &str = "correct password";

fn sample_volume_info() -> VolumeInfo {
    VolumeInfo {
        container_path: PathBuf::from("/tmp/container.hc"),
        container_size_bytes: 132_608,
        backend: "tcvc",
        cipher: "AES-XTS",
        kdf: TcvcKdf::Sha512,
        pim: PimState::Custom(1_000_000),
        header_role: HeaderCandidateRole::Backup,
        filesystem: FilesystemKind::ExFat,
        read_only: true,
        decrypted_data_offset: Some(131_072),
        decrypted_data_len: Some(1536),
    }
}

#[test]
fn volume_info_dto_round_trips_safe_fields() {
    let info = sample_volume_info();
    let dto = VolumeInfoDto::from(info.clone());

    assert_eq!(
        dto.container_path,
        info.container_path.display().to_string()
    );
    assert_eq!(dto.container_size_bytes, info.container_size_bytes);
    assert_eq!(dto.backend, info.backend);
    assert_eq!(dto.cipher, info.cipher);
    assert_eq!(dto.kdf, "sha512");
    assert_eq!(dto.pim, "custom:1000000");
    assert_eq!(dto.header_role, "backup");
    assert_eq!(dto.filesystem, "exfat");
    assert!(dto.read_only);
}

#[test]
fn volume_info_dto_default_pim_renders_as_default() {
    let mut info = sample_volume_info();
    info.pim = PimState::Default;
    info.kdf = TcvcKdf::Streebog;
    info.header_role = HeaderCandidateRole::Primary;
    info.filesystem = FilesystemKind::Ntfs;

    let dto = VolumeInfoDto::from(info);

    assert_eq!(dto.pim, "default");
    assert_eq!(dto.kdf, "streebog");
    assert_eq!(dto.header_role, "primary");
    assert_eq!(dto.filesystem, "ntfs");
}

fn sample_file_entry() -> FileEntry {
    FileEntry {
        name: "notes.txt".to_string(),
        path: Some("/dir/notes.txt".to_string()),
        is_dir: false,
        size: 4096,
        attributes: FileAttributes {
            read_only: true,
            hidden: false,
            system: false,
            directory: false,
            archive: true,
        },
        created: Some(AppTimestamp {
            year: 2026,
            month: 1,
            day: 2,
            hour: 3,
            minute: 4,
            second: 5,
        }),
        modified: None,
        accessed: None,
        filesystem: FilesystemKind::Fat,
    }
}

#[test]
fn file_entry_dto_round_trips_fields() {
    let entry = sample_file_entry();
    let dto = FileEntryDto::from(entry.clone());

    assert_eq!(dto.name, entry.name);
    assert_eq!(dto.path, entry.path);
    assert_eq!(dto.is_dir, entry.is_dir);
    assert_eq!(dto.size, entry.size);
    assert!(dto.attributes.read_only);
    assert!(!dto.attributes.hidden);
    assert!(dto.attributes.archive);
    let created = dto.created.expect("created timestamp should map through");
    assert_eq!(created.year, 2026);
    assert_eq!(created.month, 1);
    assert_eq!(created.day, 2);
    assert!(dto.modified.is_none());
    assert_eq!(dto.filesystem, "fat");
}

#[test]
fn open_container_response_serialization_never_contains_the_password() {
    let response = OpenContainerResponseDto {
        session_id: "11111111-1111-1111-1111-111111111111".to_string(),
        volume_info: VolumeInfoDto::from(sample_volume_info()),
    };

    let json = serde_json::to_string(&response).expect("response should serialize");
    assert!(
        !json.contains(TEST_PASSWORD),
        "serialized OpenContainerResponseDto must never contain the password: {json}"
    );
    assert!(
        !json.to_lowercase().contains("password"),
        "serialized OpenContainerResponseDto must not even mention a password field: {json}"
    );
}
