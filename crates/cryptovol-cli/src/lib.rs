//! Library support for the `cryptovol` command-line application.
//!
//! This crate contains command-line argument types, command execution,
//! user-facing output rendering, and exit code mapping.

pub mod commands;

use cryptovol_fs_exfat::ExfatEntry;
use cryptovol_fs_fat::DirectoryEntry;
use cryptovol_fs_ntfs::{NtfsEntry, NtfsTimestamp};
use cryptovol_tcvc::{FilesystemProbeCandidate, HeaderCandidateRole, PimState, TcvcKdf};

/// Accepted `--kdf` hint values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum KdfArg {
    /// SHA-512 PBKDF2-HMAC header KDF.
    #[value(name = "sha512")]
    Sha512,
    /// SHA-256 PBKDF2-HMAC header KDF.
    #[value(name = "sha256")]
    Sha256,
    /// Whirlpool PBKDF2-HMAC header KDF.
    #[value(name = "whirlpool")]
    Whirlpool,
    /// BLAKE2s-256 PBKDF2-HMAC header KDF.
    #[value(name = "blake2s")]
    Blake2s,
    /// Streebog-512 PBKDF2-HMAC header KDF.
    #[value(name = "streebog")]
    Streebog,
}

impl From<KdfArg> for TcvcKdf {
    fn from(arg: KdfArg) -> Self {
        match arg {
            KdfArg::Sha512 => Self::Sha512,
            KdfArg::Sha256 => Self::Sha256,
            KdfArg::Whirlpool => Self::Whirlpool,
            KdfArg::Blake2s => Self::Blake2s256,
            KdfArg::Streebog => Self::Streebog,
        }
    }
}

/// Data rendered after a successful `probe-fs` command.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProbeFsOutput {
    /// Crypto backend name.
    pub backend: &'static str,
    /// Header candidate that opened the volume.
    pub header_role: HeaderCandidateRole,
    /// KDF/hash that opened the header.
    pub kdf: TcvcKdf,
    /// Default or custom PIM that opened the header.
    pub pim: PimState,
    /// Physical offset of the encrypted data area.
    pub data_offset: u64,
    /// Logical decrypted data size.
    pub data_size: u64,
    /// Conservative filesystem candidate.
    pub candidate: FilesystemProbeCandidate,
}

/// Renders one directory entry in the stable human-readable `ls` format.
pub fn render_ls_entry(entry: &DirectoryEntry) -> String {
    let type_char = if entry.is_dir { 'd' } else { '-' };
    format!("{type_char}{:>8}  {}", entry.size, entry.name)
}

/// Renders one directory entry in long format with metadata columns.
///
/// Format: `{type_char}{perm}  {size:>8}  {date_time}  {name}`
/// where `date_time` is `YYYY-MM-DD HH:MM` from the modified timestamp,
/// or `----       --:--` when no modified timestamp is available.
pub fn render_ls_entry_long(entry: &DirectoryEntry) -> String {
    let type_char = if entry.is_dir { 'd' } else { '-' };
    let perm = if entry.is_dir {
        "rwxr-xr-x"
    } else {
        "rw-r--r--"
    };
    let date_time = match &entry.modified {
        Some(ts) => format!(
            "{:04}-{:02}-{:02} {:02}:{:02}",
            ts.date.year, ts.date.month, ts.date.day, ts.time.hour, ts.time.minute
        ),
        None => "----       --:--".to_string(),
    };
    format!(
        "{type_char}{perm}  {:>8}  {date_time}  {}",
        entry.size, entry.name
    )
}

/// Renders one exFAT directory entry in the stable human-readable `ls` format.
pub fn render_exfat_entry(entry: &ExfatEntry) -> String {
    let type_char = if entry.is_dir { 'd' } else { '-' };
    format!("{type_char}{:>8}  {}", entry.size, entry.name)
}

/// Renders one exFAT directory entry in long format with metadata columns.
///
/// Format: `{type_char}{perm}  {size:>10}  {date_time}  {name}`
/// where `date_time` is `YYYY-MM-DD HH:MM` from the modified timestamp,
/// or 16 spaces when no modified timestamp is available.
pub fn render_exfat_entry_long(entry: &ExfatEntry) -> String {
    let type_char = if entry.is_dir { 'd' } else { '-' };
    let attr = format!("{type_char}rw-r--r--");
    let date = match &entry.modified {
        Some(ts) => format!(
            "{:04}-{:02}-{:02} {:02}:{:02}",
            ts.year, ts.month, ts.day, ts.hour, ts.minute
        ),
        None => "                ".to_string(),
    };
    format!("{:<10}  {:>10}  {date}  {}", attr, entry.size, entry.name)
}

fn format_ntfs_timestamp(ts: &NtfsTimestamp) -> String {
    match cryptovol_app::AppTimestamp::from_ntfs(ts) {
        Some(app_ts) => format!(
            "{:04}-{:02}-{:02} {:02}:{:02}",
            app_ts.year, app_ts.month, app_ts.day, app_ts.hour, app_ts.minute
        ),
        None => "pre-1970     --:--".to_string(),
    }
}

/// Renders one NTFS directory entry in the stable human-readable `ls` format.
pub fn render_ntfs_entry(entry: &NtfsEntry) -> String {
    let type_char = if entry.is_dir { 'd' } else { '-' };
    format!("{type_char}{:>8}  {}", entry.size, entry.name)
}

/// Renders one NTFS directory entry in long format with metadata columns.
pub fn render_ntfs_entry_long(entry: &NtfsEntry) -> String {
    let type_char = if entry.is_dir { 'd' } else { '-' };
    let perm = if entry.is_dir {
        "rwxr-xr-x"
    } else {
        "rw-r--r--"
    };
    let date = match &entry.modified {
        Some(ts) => format_ntfs_timestamp(ts),
        None => "----           --:--".to_string(),
    };
    format!(
        "{type_char}{perm}  {:>10}  {date}  {}",
        entry.size, entry.name
    )
}

/// Renders successful `probe-fs` output.
pub fn render_probe_fs_success(output: ProbeFsOutput) -> String {
    format!(
        "TC/VC volume opened successfully.\n\
Backend: {backend}\n\
Header: {header}\n\
Encryption: AES-XTS\n\
KDF/Hash: {kdf}\n\
PIM: {pim}\n\
Read-only: yes\n\
\n\
Decrypted data:\n\
  Data offset: {data_offset}\n\
  Data size: {data_size}\n\
  First sector: readable\n\
\n\
Filesystem probe:\n\
  Candidate: {candidate}\n\
  Directory extraction: not supported\n",
        backend = output.backend,
        header = header_role_text(output.header_role),
        kdf = output.kdf.display_name(),
        pim = output.pim.display(),
        data_offset = output.data_offset,
        data_size = output.data_size,
        candidate = filesystem_candidate_text(output.candidate),
    )
}

/// Renders a matched header candidate role for user-facing output.
pub fn header_role_text(role: HeaderCandidateRole) -> &'static str {
    match role {
        HeaderCandidateRole::Primary => "primary",
        HeaderCandidateRole::Backup => "backup",
    }
}

/// Renders a filesystem probe candidate for user-facing output.
pub fn filesystem_candidate_text(candidate: FilesystemProbeCandidate) -> &'static str {
    match candidate {
        FilesystemProbeCandidate::FatLike => "FAT-like",
        FilesystemProbeCandidate::ExFat => "exFAT",
        FilesystemProbeCandidate::Ntfs => "NTFS",
        FilesystemProbeCandidate::Unknown => "unknown",
    }
}

#[cfg(test)]
mod kdf_arg_tests {
    use super::KdfArg;
    use clap::ValueEnum;
    use cryptovol_tcvc::TcvcKdf;

    #[test]
    fn kdf_arg_accepts_exactly_the_documented_values() {
        let names: Vec<String> = KdfArg::value_variants()
            .iter()
            .filter_map(ValueEnum::to_possible_value)
            .map(|v| v.get_name().to_owned())
            .collect();
        assert_eq!(
            names,
            ["sha512", "sha256", "whirlpool", "blake2s", "streebog"]
        );

        for (value, expected) in [
            ("sha512", TcvcKdf::Sha512),
            ("sha256", TcvcKdf::Sha256),
            ("whirlpool", TcvcKdf::Whirlpool),
            ("blake2s", TcvcKdf::Blake2s256),
            ("streebog", TcvcKdf::Streebog),
        ] {
            let arg = KdfArg::from_str(value, false);
            assert_eq!(arg.map(TcvcKdf::from), Ok(expected), "{value}");
        }

        for rejected in ["SHA512", "blake2s256", "sha-512", ""] {
            assert!(
                KdfArg::from_str(rejected, false).is_err(),
                "{rejected:?} must be rejected"
            );
        }
    }
}

#[cfg(test)]
mod render_tests {
    use cryptovol_fs_fat::{DirectoryEntry, FatAttributes, FatDate, FatTime, FatTimestamp};

    #[allow(dead_code)]
    fn no_attrs() -> FatAttributes {
        FatAttributes {
            read_only: false,
            hidden: false,
            system: false,
            directory: false,
            archive: false,
        }
    }

    fn archive_attrs() -> FatAttributes {
        FatAttributes {
            read_only: false,
            hidden: false,
            system: false,
            directory: false,
            archive: true,
        }
    }

    fn dir_attrs() -> FatAttributes {
        FatAttributes {
            read_only: false,
            hidden: false,
            system: false,
            directory: true,
            archive: false,
        }
    }

    fn make_file_entry(name: &str, size: u32) -> DirectoryEntry {
        DirectoryEntry {
            name: name.to_string(),
            short_name: "SHORT.TXT".to_string(),
            is_dir: false,
            size,
            attributes: archive_attrs(),
            created: None,
            modified: None,
            accessed: None,
        }
    }

    fn make_dir_entry(name: &str) -> DirectoryEntry {
        DirectoryEntry {
            name: name.to_string(),
            short_name: "SHORTDIR".to_string(),
            is_dir: true,
            size: 0,
            attributes: dir_attrs(),
            created: None,
            modified: None,
            accessed: None,
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn make_entry_with_modified(
        name: &str,
        size: u32,
        year: u16,
        month: u8,
        day: u8,
        hour: u8,
        minute: u8,
        second: u8,
    ) -> DirectoryEntry {
        DirectoryEntry {
            name: name.to_string(),
            short_name: "SHORT.TXT".to_string(),
            is_dir: false,
            size,
            attributes: archive_attrs(),
            created: None,
            modified: Some(FatTimestamp {
                date: FatDate { year, month, day },
                time: FatTime {
                    hour,
                    minute,
                    second,
                },
            }),
            accessed: None,
        }
    }

    #[test]
    fn render_ls_entry_shows_long_name() {
        let entry = make_file_entry("Project Notes Final.txt", 49);
        let output = super::render_ls_entry(&entry);
        assert!(
            output.contains("Project Notes Final.txt"),
            "expected long name in output, got: {output}"
        );
        assert!(
            !output.contains("SHORT.TXT"),
            "short name must not appear in default ls output; got: {output}"
        );
    }

    #[test]
    fn render_ls_entry_long_format_file() {
        let entry = make_file_entry("Project Notes Final.txt", 49);
        let output = super::render_ls_entry_long(&entry); // NOT YET IMPLEMENTED
        assert!(
            output.contains("Project Notes Final.txt"),
            "name missing in long output: {output}"
        );
        assert!(
            output.contains("49"),
            "size missing in long output: {output}"
        );
        // type char: file must start with '-'
        let first_char = output.chars().next().unwrap_or(' ');
        assert_eq!(
            first_char, '-',
            "file entry should start with '-'; got: {output}"
        );
    }

    #[test]
    fn render_ls_entry_long_format_dir() {
        let entry = make_dir_entry("Folder With Spaces");
        let output = super::render_ls_entry_long(&entry); // NOT YET IMPLEMENTED
        assert!(
            output.contains("Folder With Spaces"),
            "dir name missing: {output}"
        );
        let first_char = output.chars().next().unwrap_or(' ');
        assert_eq!(
            first_char, 'd',
            "dir entry should start with 'd'; got: {output}"
        );
    }

    #[test]
    fn render_ls_entry_long_with_timestamp() {
        let entry =
            make_entry_with_modified("Emoji Rocket 🚀 Test.txt", 22, 2026, 6, 28, 14, 31, 20);
        let output = super::render_ls_entry_long(&entry); // NOT YET IMPLEMENTED
        assert!(
            output.contains("2026-06-28"),
            "date missing in long output: {output}"
        );
        assert!(
            output.contains("14:31"),
            "time missing in long output: {output}"
        );
        assert!(
            output.contains("Emoji Rocket 🚀 Test.txt"),
            "name missing in long output: {output}"
        );
    }
}
