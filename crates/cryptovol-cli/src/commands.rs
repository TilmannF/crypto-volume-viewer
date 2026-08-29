//! Command execution and exit-code mapping for the `cryptovol` binary.

use crate::{render_probe_fs_success, ProbeFsOutput};
use cryptovol_app::{
    AppError, ContainerBackupHeaderCandidate, ContainerHeaderInspection, ExtractOptions, FileEntry,
    FilesystemKind, OpenVolumeRequest, VolumeSession,
};
use cryptovol_core::FileBlockReader;
use cryptovol_tcvc::{
    open_with_options, probe_filesystem, HeaderCandidateRole, TcvcKdf, TcvcOpenError,
    TcvcOpenOptions,
};
use secrecy::SecretString;
use std::path::PathBuf;
use std::process::ExitCode;
use zeroize::Zeroize;

/// Documented process exit-code categories.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CliExitCode {
    /// Command completed successfully.
    Success,
    /// Generic command failure.
    GenericError,
    /// Invalid arguments; clap owns normal argument parsing failures.
    InvalidArguments,
    /// The container format or profile is unsupported.
    UnsupportedFormat,
    /// Authentication failed or could not be completed.
    AuthenticationFailed,
    /// The decrypted filesystem was not recognized or usable.
    FilesystemNotRecognized,
    /// File extraction failed after the volume was opened.
    ExtractionFailed,
}

impl CliExitCode {
    /// Returns the numeric process exit code.
    pub const fn code(self) -> u8 {
        match self {
            Self::Success => 0,
            Self::GenericError => 1,
            Self::InvalidArguments => 2,
            Self::UnsupportedFormat => 3,
            Self::AuthenticationFailed => 4,
            Self::FilesystemNotRecognized => 5,
            Self::ExtractionFailed => 6,
        }
    }

    /// Converts this category to [`ExitCode`].
    pub fn into_exit_code(self) -> ExitCode {
        ExitCode::from(self.code())
    }
}

fn parse_kdf_hint(kdf: Option<&str>) -> Result<Option<TcvcKdf>, ()> {
    match kdf {
        None => Ok(None),
        Some("sha512") => Ok(Some(TcvcKdf::Sha512)),
        Some("sha256") => Ok(Some(TcvcKdf::Sha256)),
        Some("whirlpool") => Ok(Some(TcvcKdf::Whirlpool)),
        Some("blake2s") => Ok(Some(TcvcKdf::Blake2s256)),
        Some("streebog") => Ok(Some(TcvcKdf::Streebog)),
        Some(other) => {
            eprintln!(
                "unsupported KDF: {other}; supported values: sha512, sha256, whirlpool, blake2s, streebog"
            );
            Err(())
        }
    }
}

/// Executes `cryptovol info`.
pub fn info(container: &str) -> ExitCode {
    match cryptovol_app::inspect_container(container) {
        Ok(info) => {
            println!("File: {container}");
            println!("Size: {} bytes", info.container_size_bytes);
            println!("Readable: yes");
            println!("Read-only mode: yes");
            println!("Crypto backend: not opened by this command");
            println!("Filesystem: not detected by this command");
            print_tcvc_backend(&info.header_inspection);
            CliExitCode::Success.into_exit_code()
        }
        Err(err) => {
            eprintln!("{err}");
            CliExitCode::GenericError.into_exit_code()
        }
    }
}

/// Opens a TC/VC volume for `container`, prompting for a password and
/// mapping [`AppError`] to the documented exit codes on failure.
///
/// The failure message is printed via `eprintln!`, matching the `ls` and
/// `extract` commands' existing stderr-based error reporting.
fn open_volume_for_command(
    container: &str,
    pim: Option<u32>,
    kdf_hint: Option<TcvcKdf>,
) -> Result<VolumeSession, ExitCode> {
    if let Err(err) = FileBlockReader::open(container) {
        eprintln!("{err}");
        return Err(CliExitCode::GenericError.into_exit_code());
    }

    let password = match rpassword::prompt_password("Password: ") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("Could not open volume: authentication failed or unsupported parameters.");
            return Err(CliExitCode::AuthenticationFailed.into_exit_code());
        }
    };

    let request = OpenVolumeRequest {
        container_path: PathBuf::from(container),
        password: SecretString::from(password),
        pim,
        kdf_hint,
    };

    cryptovol_app::open_volume(request).map_err(|err| match err {
        AppError::AuthFailed => {
            eprintln!("Could not open volume: authentication failed or unsupported parameters.");
            CliExitCode::AuthenticationFailed.into_exit_code()
        }
        AppError::UnsupportedFormat(_) => {
            eprintln!("Could not open volume: authentication failed or unsupported parameters.");
            CliExitCode::UnsupportedFormat.into_exit_code()
        }
        other => {
            eprintln!("Could not open volume: {other}");
            CliExitCode::GenericError.into_exit_code()
        }
    })
}

/// Executes `cryptovol test-open`.
pub fn test_open(container: &str, pim: Option<u32>, kdf: Option<&str>) -> ExitCode {
    let kdf_hint = match parse_kdf_hint(kdf) {
        Ok(hint) => hint,
        Err(()) => return CliExitCode::InvalidArguments.into_exit_code(),
    };

    if let Err(err) = FileBlockReader::open(container) {
        eprintln!("{err}");
        return CliExitCode::GenericError.into_exit_code();
    }

    let password = match rpassword::prompt_password("Password: ") {
        Ok(password) => password,
        Err(_) => {
            println!("Could not open volume: authentication failed or unsupported parameters.");
            return CliExitCode::AuthenticationFailed.into_exit_code();
        }
    };

    let request = OpenVolumeRequest {
        container_path: PathBuf::from(container),
        password: SecretString::from(password),
        pim,
        kdf_hint,
    };

    match cryptovol_app::open_volume(request) {
        Ok(session) => {
            let info = session.volume_info();
            let header = match info.header_role {
                HeaderCandidateRole::Primary => "primary",
                HeaderCandidateRole::Backup => "backup",
            };
            println!("TC/VC volume opened successfully.");
            println!("Backend: tcvc");
            println!("Header: {header}");
            println!("Encryption: AES-XTS");
            println!("KDF/Hash: {}", info.kdf.display_name());
            println!("PIM: {}", info.pim.display());
            println!("Read-only: yes");
            CliExitCode::Success.into_exit_code()
        }
        Err(AppError::AuthFailed) => {
            println!("Could not open volume: authentication failed or unsupported parameters.");
            CliExitCode::AuthenticationFailed.into_exit_code()
        }
        Err(AppError::UnsupportedFormat(_)) => {
            println!("Could not open volume: authentication failed or unsupported parameters.");
            CliExitCode::UnsupportedFormat.into_exit_code()
        }
        Err(err) => {
            eprintln!("Could not open volume: {err}");
            CliExitCode::GenericError.into_exit_code()
        }
    }
}

/// Executes `cryptovol probe-fs`.
///
/// This command still opens the volume directly through `cryptovol-tcvc`
/// rather than `cryptovol_app::open_volume`/`VolumeSession`, because its
/// output needs the raw decrypted-data offset/length, which
/// `cryptovol_app::VolumeInfo` does not expose and this task may not add.
pub fn probe_fs(container: &str, pim: Option<u32>, kdf: Option<&str>) -> ExitCode {
    let kdf_hint = match parse_kdf_hint(kdf) {
        Ok(hint) => hint,
        Err(()) => return CliExitCode::InvalidArguments.into_exit_code(),
    };

    let reader = match FileBlockReader::open(container) {
        Ok(reader) => reader,
        Err(err) => {
            eprintln!("{err}");
            return CliExitCode::GenericError.into_exit_code();
        }
    };

    let mut password = match rpassword::prompt_password("Password: ") {
        Ok(password) => password,
        Err(_) => {
            println!(
                "Could not probe filesystem: authentication failed or unsupported parameters."
            );
            return CliExitCode::AuthenticationFailed.into_exit_code();
        }
    };

    let options = TcvcOpenOptions {
        password: password.as_bytes().to_vec(),
        pim,
        kdf_hint,
    };
    password.zeroize();

    let result = open_with_options(&reader, &options);

    let opened = match result {
        Ok(opened) => opened,
        Err(TcvcOpenError::AuthenticationOrUnsupported) => {
            println!(
                "Could not probe filesystem: authentication failed or unsupported parameters."
            );
            return CliExitCode::AuthenticationFailed.into_exit_code();
        }
        Err(TcvcOpenError::UnsupportedProfile { .. }) => {
            println!(
                "Could not probe filesystem: authentication failed or unsupported parameters."
            );
            return CliExitCode::UnsupportedFormat.into_exit_code();
        }
        Err(TcvcOpenError::ReadFailed { error }) => {
            eprintln!("Could not probe filesystem: {error}");
            return CliExitCode::GenericError.into_exit_code();
        }
        Err(TcvcOpenError::InvalidPim { .. }) => {
            eprintln!(
                "Could not probe filesystem: authentication failed or unsupported parameters."
            );
            return CliExitCode::AuthenticationFailed.into_exit_code();
        }
    };

    let metadata = opened.metadata();
    let data_reader = match opened.data_reader(&reader) {
        Ok(data_reader) => data_reader,
        Err(TcvcOpenError::AuthenticationOrUnsupported) => {
            println!(
                "Could not probe filesystem: authentication failed or unsupported parameters."
            );
            return CliExitCode::AuthenticationFailed.into_exit_code();
        }
        Err(TcvcOpenError::UnsupportedProfile { .. }) => {
            println!(
                "Could not probe filesystem: authentication failed or unsupported parameters."
            );
            return CliExitCode::UnsupportedFormat.into_exit_code();
        }
        Err(TcvcOpenError::ReadFailed { error }) => {
            eprintln!("Could not probe filesystem: {error}");
            return CliExitCode::GenericError.into_exit_code();
        }
        Err(TcvcOpenError::InvalidPim { .. }) => {
            eprintln!(
                "Could not probe filesystem: authentication failed or unsupported parameters."
            );
            return CliExitCode::AuthenticationFailed.into_exit_code();
        }
    };

    let candidate = match probe_filesystem(&data_reader) {
        Ok(candidate) => candidate,
        Err(err) => {
            eprintln!("Could not probe filesystem: {err}");
            return CliExitCode::FilesystemNotRecognized.into_exit_code();
        }
    };

    print!(
        "{}",
        render_probe_fs_success(ProbeFsOutput {
            backend: metadata.backend,
            data_offset: metadata.data_offset,
            data_size: metadata.data_length,
            candidate,
        })
    );

    CliExitCode::Success.into_exit_code()
}

/// Renders one [`FileEntry`] in the stable human-readable `ls` format.
///
/// The short format is identical across FAT, exFAT, and NTFS.
fn render_ls_entry(entry: &FileEntry) -> String {
    let type_char = if entry.is_dir { 'd' } else { '-' };
    format!("{type_char}{:>8}  {}", entry.size, entry.name)
}

/// Renders one [`FileEntry`] in long format, matching the current
/// per-filesystem column layout.
fn render_ls_entry_long(entry: &FileEntry) -> String {
    match entry.filesystem {
        FilesystemKind::ExFat => render_exfat_entry_long(entry),
        FilesystemKind::Ntfs => render_ntfs_entry_long(entry),
        FilesystemKind::Fat | FilesystemKind::Unknown => render_fat_entry_long(entry),
    }
}

fn render_fat_entry_long(entry: &FileEntry) -> String {
    let type_char = if entry.is_dir { 'd' } else { '-' };
    let perm = if entry.is_dir {
        "rwxr-xr-x"
    } else {
        "rw-r--r--"
    };
    let date_time = match &entry.modified {
        Some(ts) => format!(
            "{:04}-{:02}-{:02} {:02}:{:02}",
            ts.year, ts.month, ts.day, ts.hour, ts.minute
        ),
        None => "----       --:--".to_string(),
    };
    format!(
        "{type_char}{perm}  {:>8}  {date_time}  {}",
        entry.size, entry.name
    )
}

fn render_exfat_entry_long(entry: &FileEntry) -> String {
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

fn render_ntfs_entry_long(entry: &FileEntry) -> String {
    let type_char = if entry.is_dir { 'd' } else { '-' };
    let perm = if entry.is_dir {
        "rwxr-xr-x"
    } else {
        "rw-r--r--"
    };
    let date = match &entry.modified {
        Some(ts) => format!(
            "{:04}-{:02}-{:02} {:02}:{:02}",
            ts.year, ts.month, ts.day, ts.hour, ts.minute
        ),
        None => "----           --:--".to_string(),
    };
    format!(
        "{type_char}{perm}  {:>10}  {date}  {}",
        entry.size, entry.name
    )
}

/// Executes `cryptovol ls`.
pub fn ls(
    container: &str,
    path: &str,
    long: bool,
    pim: Option<u32>,
    kdf: Option<&str>,
) -> ExitCode {
    let kdf_hint = match parse_kdf_hint(kdf) {
        Ok(hint) => hint,
        Err(()) => return CliExitCode::InvalidArguments.into_exit_code(),
    };

    let session = match open_volume_for_command(container, pim, kdf_hint) {
        Ok(session) => session,
        Err(exit_code) => return exit_code,
    };

    match session.list_dir(path) {
        Ok(entries) => {
            for entry in &entries {
                if long {
                    println!("{}", render_ls_entry_long(entry));
                } else {
                    println!("{}", render_ls_entry(entry));
                }
            }
            CliExitCode::Success.into_exit_code()
        }
        Err(AppError::PathNotFound(p)) => {
            eprintln!("Not found: {p}");
            CliExitCode::FilesystemNotRecognized.into_exit_code()
        }
        Err(AppError::FilesystemNotRecognized) => {
            eprintln!("Filesystem not recognized.");
            CliExitCode::FilesystemNotRecognized.into_exit_code()
        }
        Err(AppError::InvalidInput(msg)) => {
            eprintln!("{}", capitalize_first(&msg));
            CliExitCode::FilesystemNotRecognized.into_exit_code()
        }
        Err(err) => {
            eprintln!("Could not list directory: {err}");
            CliExitCode::FilesystemNotRecognized.into_exit_code()
        }
    }
}

/// Capitalizes the first character of `s`, leaving the rest unchanged.
///
/// Used to render [`AppError::InvalidInput`] messages (lowercase by
/// convention) as the capitalized, sentence-style stderr text the CLI used
/// before routing through `cryptovol-app`.
fn capitalize_first(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

/// Executes `cryptovol extract`.
pub fn extract(
    container: &str,
    source_path: &str,
    destination_path: &str,
    overwrite: bool,
    parents: bool,
    pim: Option<u32>,
    kdf: Option<&str>,
) -> ExitCode {
    let kdf_hint = match parse_kdf_hint(kdf) {
        Ok(hint) => hint,
        Err(()) => return CliExitCode::InvalidArguments.into_exit_code(),
    };

    let session = match open_volume_for_command(container, pim, kdf_hint) {
        Ok(session) => session,
        Err(exit_code) => return exit_code,
    };

    let options = ExtractOptions {
        overwrite,
        parents,
        cancellation_token: None,
    };

    match session.extract_file(source_path, destination_path, options, |_event| {}) {
        Ok(summary) => {
            println!(
                "Extracted {source_path} to {destination_path} ({} bytes).",
                summary.bytes_written
            );
            CliExitCode::Success.into_exit_code()
        }
        Err(AppError::PathNotFound(p)) => {
            eprintln!("Not found: {p}");
            CliExitCode::ExtractionFailed.into_exit_code()
        }
        Err(AppError::DirectoryExtractionUnsupported(p)) => {
            eprintln!("Source is a directory: {p}");
            CliExitCode::ExtractionFailed.into_exit_code()
        }
        Err(AppError::FilesystemNotRecognized) => {
            eprintln!("Filesystem not recognized.");
            CliExitCode::FilesystemNotRecognized.into_exit_code()
        }
        Err(AppError::InvalidInput(msg)) => {
            eprintln!("{msg}");
            CliExitCode::ExtractionFailed.into_exit_code()
        }
        Err(err) => {
            eprintln!("Could not read file: {err}");
            CliExitCode::ExtractionFailed.into_exit_code()
        }
    }
}

fn print_tcvc_backend(inspection: &ContainerHeaderInspection) {
    println!();
    println!("TC/VC backend:");

    match inspection {
        ContainerHeaderInspection::TooSmall { required_minimum } => {
            println!("  Status: file too small for TC/VC header candidates");
            println!("  Required minimum: {required_minimum} bytes");
        }
        ContainerHeaderInspection::Candidates { primary, backup } => {
            println!(
                "  Primary header candidate: offset {}, length {} bytes, readable {}",
                primary.offset,
                primary.length,
                readable_text(primary.readable)
            );
            match backup {
                ContainerBackupHeaderCandidate::Candidate(candidate) => {
                    println!(
                        "  Backup header candidate: offset {}, length {} bytes, readable {}",
                        candidate.offset,
                        candidate.length,
                        readable_text(candidate.readable)
                    );
                }
                ContainerBackupHeaderCandidate::OverlapsPrimary => {
                    println!("  Backup header candidate: overlaps primary candidate");
                }
            }
            println!("  Header interpretation: not attempted by this command");
            println!("  Decryption: not attempted by this command");
        }
    }
}

fn readable_text(readable: bool) -> &'static str {
    if readable {
        "yes"
    } else {
        "no"
    }
}
