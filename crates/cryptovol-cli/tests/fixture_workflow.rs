#![allow(
    missing_docs,
    clippy::expect_used,
    clippy::panic,
    reason = "fixture workflow tests assert repository test contracts"
)]

use std::fs;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const STATIC_FIXTURE_ENV: &str = "CRYPTOVOL_STATIC_FAT_FIXTURE";

fn static_fixture_path() -> Option<PathBuf> {
    let val = std::env::var(STATIC_FIXTURE_ENV).ok()?;
    let p = PathBuf::from(val);
    if p.exists() {
        Some(p)
    } else {
        None
    }
}

fn lfn_fixture_path() -> Option<PathBuf> {
    let val = std::env::var("CRYPTOVOL_STATIC_FAT_LFN_FIXTURE").ok()?;
    let p = PathBuf::from(val);
    if p.exists() {
        Some(p)
    } else {
        None
    }
}

fn crypto_matrix_fixture(name: &str) -> Option<PathBuf> {
    let dir = std::env::var("CRYPTOVOL_STATIC_CRYPTO_MATRIX_DIR").ok()?;
    let p = PathBuf::from(dir).join(name);
    if p.exists() {
        Some(p)
    } else {
        None
    }
}

fn run_test_open(fixture: &Path, extra_args: &[&str], password: &str) -> std::process::Output {
    // rpassword reads from /dev/tty (not piped stdin) on macOS/Linux.
    // Use `expect` to spawn cryptovol in a pty so /dev/tty resolves to the pty.
    // cryptovol stdout is redirected to a temp file via `sh -c`; stderr stays on
    // the pty so expect can match the "Password:" prompt.
    let bin = env!("CARGO_BIN_EXE_cryptovol");
    let out_path = std::env::temp_dir().join(format!(
        "cryptovol_test_{}_{}.txt",
        std::process::id(),
        fixture.file_stem().unwrap_or_default().to_string_lossy()
    ));
    let extra = extra_args.join(" ");

    // Paths come from env vars; bare $var in Tcl is a single word (no word-split).
    // The shell command double-quotes all paths so spaces are handled.
    let script = r#"set timeout 60
set cmd "\"$env(CV_BIN)\" test-open \"$env(CV_FIX)\" $env(CV_EXTRA) > \"$env(CV_OUT)\""
spawn sh -c $cmd
expect "Password:"
send "$env(CV_PWD)\r"
expect eof
lassign [wait] pid spawnid os_error_flag value
exit $value
"#;

    let status = Command::new("expect")
        .arg("-c")
        .arg(script)
        .env("CV_BIN", bin)
        .env("CV_FIX", fixture)
        .env("CV_EXTRA", &extra)
        .env("CV_OUT", &out_path)
        .env("CV_PWD", password)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .expect("expect binary should be available (/usr/bin/expect on macOS)");

    let stdout = std::fs::read(&out_path).unwrap_or_default();
    let _ = std::fs::remove_file(&out_path);

    std::process::Output {
        status,
        stdout,
        stderr: vec![],
    }
}

const FIXTURE_SCRIPT: &str = "scripts/test-with-veracrypt-fixtures.sh";
const FIXTURE_PATH: &str = "testdata/generated/tcvc-aes-sha512-basic.hc";

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("crate should live two levels below workspace root")
        .to_path_buf()
}

#[test]
fn veracrypt_fixture_script_contract_is_documented() {
    let root = workspace_root();
    let script_path = root.join(FIXTURE_SCRIPT);

    assert!(
        script_path.exists(),
        "{FIXTURE_SCRIPT} should exist to generate VeraCrypt fixture tests"
    );
    assert!(
        script_path.is_file(),
        "{FIXTURE_SCRIPT} should be a regular script file"
    );

    #[cfg(unix)]
    {
        let mode = fs::metadata(&script_path)
            .expect("fixture script metadata should be readable")
            .permissions()
            .mode();
        assert_ne!(
            mode & 0o111,
            0,
            "{FIXTURE_SCRIPT} should be executable on Unix-like systems"
        );
    }

    let script = fs::read_to_string(&script_path).expect("fixture script should be readable");
    assert!(
        script.contains(FIXTURE_PATH),
        "{FIXTURE_SCRIPT} should generate {FIXTURE_PATH}"
    );
    assert!(
        script.contains("CRYPTOVOL_TEST_CONTAINER"),
        "{FIXTURE_SCRIPT} should pass the generated fixture path through the gated test env var"
    );
    assert!(
        script.contains("--ignored"),
        "{FIXTURE_SCRIPT} should run ignored fixture-gated tests, not require fixtures during normal cargo test"
    );
}

#[test]
fn generated_veracrypt_containers_are_git_ignored() {
    let root = workspace_root();
    let output = Command::new("git")
        .args(["check-ignore", "-q", FIXTURE_PATH])
        .current_dir(&root)
        .output()
        .expect("git check-ignore should run");

    assert!(
        output.status.success(),
        "{FIXTURE_PATH} should be ignored so generated encrypted containers are not committed"
    );
}

#[test]
#[ignore]
fn ls_root_with_static_fixture() {
    let path = match static_fixture_path() {
        Some(p) => p,
        None => {
            eprintln!("skipped: {STATIC_FIXTURE_ENV} not set or fixture absent");
            return;
        }
    };

    let reader = cryptovol_core::FileBlockReader::open(&path).expect("fixture should be readable");
    let opened = cryptovol_tcvc::open_aes_sha512_volume(&reader, b"test-password")
        .expect("fixture should open with test-password");
    let data_reader = opened
        .data_reader(&reader)
        .expect("data reader should be created");
    let fs = cryptovol_fs_fat::FatFileSystem::open(&data_reader).expect("FAT should parse");

    let entries = fs.list_dir("/").expect("root listing should succeed");
    let rendered: Vec<String> = entries.iter().map(cryptovol_cli::render_ls_entry).collect();
    let output = rendered.join("\n");

    assert!(
        output.contains("HELLO.TXT"),
        "root listing must contain HELLO.TXT: {output}"
    );
    assert!(
        output.contains("SYDNEY.JPG"),
        "root listing must contain SYDNEY.JPG: {output}"
    );
    assert!(
        output.contains("DIR"),
        "root listing must contain DIR: {output}"
    );
    assert!(
        !output.contains("not implemented"),
        "output must not be a placeholder: {output}"
    );
}

// --- test-open output format tests (will pass after T-009 updates the output) ---

#[test]
#[ignore = "requires CRYPTOVOL_STATIC_FAT_LFN_FIXTURE pointing to testdata/static/tcvc-aes-sha512-fat-lfn-unicode.hc"]
fn test_open_output_sha512_default_reports_kdf_and_pim() {
    let path = match lfn_fixture_path() {
        Some(p) => p,
        None => {
            eprintln!("skipped: CRYPTOVOL_STATIC_FAT_LFN_FIXTURE not set or fixture absent");
            return;
        }
    };
    let output = run_test_open(&path, &[], "test-password");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "test-open with SHA-512/default baseline must succeed: stdout={stdout} stderr={stderr}"
    );
    assert!(
        stdout.contains("Backend: tcvc"),
        "output must include Backend: tcvc: {stdout}"
    );
    assert!(
        stdout.contains("Encryption: AES-XTS"),
        "output must include Encryption: AES-XTS: {stdout}"
    );
    assert!(
        stdout.contains("KDF/Hash: SHA-512"),
        "output must include KDF/Hash: SHA-512: {stdout}"
    );
    assert!(
        stdout.contains("PIM: default"),
        "output must include PIM: default: {stdout}"
    );
}

#[test]
#[ignore = "requires CRYPTOVOL_STATIC_CRYPTO_MATRIX_DIR pointing to testdata/static/crypto-matrix/"]
fn test_open_output_sha256_default_reports_kdf() {
    let path = match crypto_matrix_fixture("tcvc-aes-sha256-pim-default-fat-lfn-unicode.hc") {
        Some(p) => p,
        None => {
            eprintln!("skipped: CRYPTOVOL_STATIC_CRYPTO_MATRIX_DIR not set or fixture absent");
            return;
        }
    };
    let output = run_test_open(&path, &[], "test-password");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "test-open with SHA-256/default fixture must succeed: stdout={stdout} stderr={stderr}"
    );
    assert!(
        stdout.contains("KDF/Hash: SHA-256"),
        "output must include KDF/Hash: SHA-256: {stdout}"
    );
    assert!(
        stdout.contains("PIM: default"),
        "output must include PIM: default: {stdout}"
    );
}

#[test]
#[ignore = "requires CRYPTOVOL_STATIC_CRYPTO_MATRIX_DIR pointing to testdata/static/crypto-matrix/"]
fn test_open_output_sha512_pim500_reports_pim() {
    let path = match crypto_matrix_fixture("tcvc-aes-sha512-pim-500-fat-lfn-unicode.hc") {
        Some(p) => p,
        None => {
            eprintln!("skipped: CRYPTOVOL_STATIC_CRYPTO_MATRIX_DIR not set or fixture absent");
            return;
        }
    };
    let output = run_test_open(&path, &["--pim", "500"], "test-password");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "test-open with SHA-512/PIM-500 fixture must succeed: stdout={stdout} stderr={stderr}"
    );
    assert!(
        stdout.contains("KDF/Hash: SHA-512"),
        "output must include KDF/Hash: SHA-512: {stdout}"
    );
    assert!(
        stdout.contains("PIM: 500"),
        "output must include PIM: 500: {stdout}"
    );
}

// ---------------------------------------------------------------------------
// Streaming extraction integration tests (T-010)
//
// These tests verify that read_file_to_writer delivers identical bytes to
// read_file for FAT, exFAT, and NTFS, and that directory / missing-path
// errors propagate correctly (they map to ExtractionFailed / exit code 6 in
// the CLI layer).
//
// The tests are #[ignore]-gated so they are skipped in environments without
// the static fixtures, but they compile and run via:
//   CRYPTOVOL_STATIC_FAT_FIXTURE=... cargo test -p cryptovol-cli -- --ignored
// ---------------------------------------------------------------------------

fn exfat_fixture_path() -> Option<PathBuf> {
    let val = std::env::var("CRYPTOVOL_STATIC_EXFAT_LFN_FIXTURE").ok()?;
    let p = PathBuf::from(val);
    if p.exists() {
        Some(p)
    } else {
        None
    }
}

fn ntfs_fixture_path() -> Option<PathBuf> {
    let val = std::env::var("CRYPTOVOL_STATIC_NTFS_LFN_FIXTURE").ok()?;
    let p = PathBuf::from(val);
    if p.exists() {
        Some(p)
    } else {
        None
    }
}

// --- FAT streaming extraction ---

#[test]
#[ignore = "requires CRYPTOVOL_STATIC_FAT_FIXTURE env var pointing to testdata/static/tcvc-aes-sha512-fat-files.hc"]
fn fat_streaming_extract_hello_txt_matches_read_file() {
    let path = match static_fixture_path() {
        Some(p) => p,
        None => {
            eprintln!("skipped: {STATIC_FIXTURE_ENV} not set or absent");
            return;
        }
    };
    let reader = cryptovol_core::FileBlockReader::open(&path).expect("fixture readable");
    let opened = cryptovol_tcvc::open_aes_sha512_volume(&reader, b"test-password")
        .expect("fixture opens with test-password");
    let data = opened.data_reader(&reader).expect("data reader");
    let fs = cryptovol_fs_fat::FatFileSystem::open(&data).expect("FAT parses");

    let expected = fs
        .read_file("/HELLO.TXT")
        .expect("read_file should succeed");
    let mut actual = Vec::new();
    let n = fs
        .read_file_to_writer("/HELLO.TXT", &mut actual)
        .expect("read_file_to_writer should succeed");
    assert_eq!(
        n as usize,
        actual.len(),
        "returned byte count must match written"
    );
    assert_eq!(
        actual, expected,
        "streaming output must equal read_file output"
    );
}

#[test]
#[ignore = "requires CRYPTOVOL_STATIC_FAT_FIXTURE env var pointing to testdata/static/tcvc-aes-sha512-fat-files.hc"]
fn fat_streaming_extract_nested_txt_matches_read_file() {
    let path = match static_fixture_path() {
        Some(p) => p,
        None => {
            eprintln!("skipped: {STATIC_FIXTURE_ENV} not set or absent");
            return;
        }
    };
    let reader = cryptovol_core::FileBlockReader::open(&path).expect("fixture readable");
    let opened = cryptovol_tcvc::open_aes_sha512_volume(&reader, b"test-password")
        .expect("fixture opens with test-password");
    let data = opened.data_reader(&reader).expect("data reader");
    let fs = cryptovol_fs_fat::FatFileSystem::open(&data).expect("FAT parses");

    let expected = fs
        .read_file("/DIR/NESTED.TXT")
        .expect("read_file should succeed");
    let mut actual = Vec::new();
    let n = fs
        .read_file_to_writer("/DIR/NESTED.TXT", &mut actual)
        .expect("read_file_to_writer should succeed");
    assert_eq!(n as usize, actual.len(), "byte count mismatch");
    assert_eq!(actual, expected, "streaming bytes must match");
}

#[test]
#[ignore = "requires CRYPTOVOL_STATIC_FAT_FIXTURE env var pointing to testdata/static/tcvc-aes-sha512-fat-files.hc"]
fn fat_streaming_extract_directory_returns_extraction_error() {
    let path = match static_fixture_path() {
        Some(p) => p,
        None => {
            eprintln!("skipped: {STATIC_FIXTURE_ENV} not set or absent");
            return;
        }
    };
    // Directory extraction error maps to CliExitCode::ExtractionFailed (exit 6) in the CLI layer.
    let reader = cryptovol_core::FileBlockReader::open(&path).expect("fixture readable");
    let opened = cryptovol_tcvc::open_aes_sha512_volume(&reader, b"test-password")
        .expect("fixture opens with test-password");
    let data = opened.data_reader(&reader).expect("data reader");
    let fs = cryptovol_fs_fat::FatFileSystem::open(&data).expect("FAT parses");

    let mut sink = Vec::new();
    let result = fs.read_file_to_writer("/DIR", &mut sink);
    assert!(
        matches!(result, Err(cryptovol_fs_fat::FatError::IsADirectory { .. })),
        "directory source must return IsADirectory, got {result:?}"
    );
}

#[test]
#[ignore = "requires CRYPTOVOL_STATIC_FAT_FIXTURE env var pointing to testdata/static/tcvc-aes-sha512-fat-files.hc"]
fn fat_streaming_extract_missing_path_returns_extraction_error() {
    let path = match static_fixture_path() {
        Some(p) => p,
        None => {
            eprintln!("skipped: {STATIC_FIXTURE_ENV} not set or absent");
            return;
        }
    };
    // Missing path maps to CliExitCode::ExtractionFailed (exit 6) in the CLI layer.
    let reader = cryptovol_core::FileBlockReader::open(&path).expect("fixture readable");
    let opened = cryptovol_tcvc::open_aes_sha512_volume(&reader, b"test-password")
        .expect("fixture opens with test-password");
    let data = opened.data_reader(&reader).expect("data reader");
    let fs = cryptovol_fs_fat::FatFileSystem::open(&data).expect("FAT parses");

    let mut sink = Vec::new();
    let result = fs.read_file_to_writer("/DOESNOTEXIST.TXT", &mut sink);
    assert!(
        matches!(result, Err(cryptovol_fs_fat::FatError::PathNotFound { .. })),
        "missing path must return PathNotFound, got {result:?}"
    );
}

// --- exFAT streaming extraction ---

#[test]
#[ignore = "requires CRYPTOVOL_STATIC_EXFAT_LFN_FIXTURE env var pointing to testdata/static/tcvc-aes-sha512-exfat-lfn-unicode.hc"]
fn exfat_streaming_extract_emoji_file_matches_read_file() {
    let path = match exfat_fixture_path() {
        Some(p) => p,
        None => {
            eprintln!("skipped: CRYPTOVOL_STATIC_EXFAT_LFN_FIXTURE not set or absent");
            return;
        }
    };
    let reader = cryptovol_core::FileBlockReader::open(&path).expect("fixture readable");
    let opened = cryptovol_tcvc::open_aes_sha512_volume(&reader, b"test-password")
        .expect("fixture opens with test-password");
    let data = opened.data_reader(&reader).expect("data reader");
    let fs = cryptovol_fs_exfat::ExfatFileSystem::open(&data).expect("exFAT parses");

    let name = "/Emoji Rocket \u{1F680} Test.txt";
    let expected = fs.read_file(name).expect("read_file should succeed");
    let mut actual = Vec::new();
    let n = fs
        .read_file_to_writer(name, &mut actual)
        .expect("read_file_to_writer should succeed");
    assert_eq!(n as usize, actual.len(), "byte count mismatch");
    assert_eq!(actual, expected, "streaming bytes must match");
}

#[test]
#[ignore = "requires CRYPTOVOL_STATIC_EXFAT_LFN_FIXTURE env var pointing to testdata/static/tcvc-aes-sha512-exfat-lfn-unicode.hc"]
fn exfat_streaming_extract_directory_returns_extraction_error() {
    let path = match exfat_fixture_path() {
        Some(p) => p,
        None => {
            eprintln!("skipped: CRYPTOVOL_STATIC_EXFAT_LFN_FIXTURE not set or absent");
            return;
        }
    };
    let reader = cryptovol_core::FileBlockReader::open(&path).expect("fixture readable");
    let opened = cryptovol_tcvc::open_aes_sha512_volume(&reader, b"test-password")
        .expect("fixture opens with test-password");
    let data = opened.data_reader(&reader).expect("data reader");
    let fs = cryptovol_fs_exfat::ExfatFileSystem::open(&data).expect("exFAT parses");

    let mut sink = Vec::new();
    let result = fs.read_file_to_writer("/Folder With Spaces", &mut sink);
    assert!(
        matches!(
            result,
            Err(cryptovol_fs_exfat::ExfatError::AttemptedDirectoryExtraction { .. })
        ),
        "directory source must return AttemptedDirectoryExtraction, got {result:?}"
    );
}

#[test]
#[ignore = "requires CRYPTOVOL_STATIC_EXFAT_LFN_FIXTURE env var pointing to testdata/static/tcvc-aes-sha512-exfat-lfn-unicode.hc"]
fn exfat_streaming_extract_missing_path_returns_extraction_error() {
    let path = match exfat_fixture_path() {
        Some(p) => p,
        None => {
            eprintln!("skipped: CRYPTOVOL_STATIC_EXFAT_LFN_FIXTURE not set or absent");
            return;
        }
    };
    let reader = cryptovol_core::FileBlockReader::open(&path).expect("fixture readable");
    let opened = cryptovol_tcvc::open_aes_sha512_volume(&reader, b"test-password")
        .expect("fixture opens with test-password");
    let data = opened.data_reader(&reader).expect("data reader");
    let fs = cryptovol_fs_exfat::ExfatFileSystem::open(&data).expect("exFAT parses");

    let mut sink = Vec::new();
    let result = fs.read_file_to_writer("/does-not-exist.txt", &mut sink);
    assert!(
        matches!(
            result,
            Err(cryptovol_fs_exfat::ExfatError::PathNotFound { .. })
        ),
        "missing path must return PathNotFound, got {result:?}"
    );
}

// --- NTFS streaming extraction ---

#[test]
#[ignore = "requires CRYPTOVOL_STATIC_NTFS_LFN_FIXTURE env var pointing to testdata/static/tcvc-aes-sha512-ntfs-lfn-unicode.hc"]
fn ntfs_streaming_extract_emoji_file_matches_read_file() {
    let path = match ntfs_fixture_path() {
        Some(p) => p,
        None => {
            eprintln!("skipped: CRYPTOVOL_STATIC_NTFS_LFN_FIXTURE not set or absent");
            return;
        }
    };
    let reader = cryptovol_core::FileBlockReader::open(&path).expect("fixture readable");
    let opened = cryptovol_tcvc::open_aes_sha512_volume(&reader, b"test-password")
        .expect("fixture opens with test-password");
    let data = opened.data_reader(&reader).expect("data reader");
    let fs = cryptovol_fs_ntfs::NtfsFileSystem::open(&data).expect("NTFS parses");

    let name = "/Emoji Rocket \u{1F680} Test.txt";
    let expected = fs.read_file(name).expect("read_file should succeed");
    let mut actual = Vec::new();
    let n = fs
        .read_file_to_writer(name, &mut actual)
        .expect("read_file_to_writer should succeed");
    assert_eq!(n as usize, actual.len(), "byte count mismatch");
    assert_eq!(actual, expected, "streaming bytes must match");
}

#[test]
#[ignore = "requires CRYPTOVOL_STATIC_NTFS_LFN_FIXTURE env var pointing to testdata/static/tcvc-aes-sha512-ntfs-lfn-unicode.hc"]
fn ntfs_streaming_extract_directory_returns_extraction_error() {
    let path = match ntfs_fixture_path() {
        Some(p) => p,
        None => {
            eprintln!("skipped: CRYPTOVOL_STATIC_NTFS_LFN_FIXTURE not set or absent");
            return;
        }
    };
    let reader = cryptovol_core::FileBlockReader::open(&path).expect("fixture readable");
    let opened = cryptovol_tcvc::open_aes_sha512_volume(&reader, b"test-password")
        .expect("fixture opens with test-password");
    let data = opened.data_reader(&reader).expect("data reader");
    let fs = cryptovol_fs_ntfs::NtfsFileSystem::open(&data).expect("NTFS parses");

    let mut sink = Vec::new();
    let result = fs.read_file_to_writer("/Folder With Spaces", &mut sink);
    assert!(
        matches!(
            result,
            Err(cryptovol_fs_ntfs::NtfsError::AttemptedDirectoryExtraction { .. })
        ),
        "directory source must return AttemptedDirectoryExtraction, got {result:?}"
    );
}

#[test]
#[ignore = "requires CRYPTOVOL_STATIC_NTFS_LFN_FIXTURE env var pointing to testdata/static/tcvc-aes-sha512-ntfs-lfn-unicode.hc"]
fn ntfs_streaming_extract_missing_path_returns_extraction_error() {
    let path = match ntfs_fixture_path() {
        Some(p) => p,
        None => {
            eprintln!("skipped: CRYPTOVOL_STATIC_NTFS_LFN_FIXTURE not set or absent");
            return;
        }
    };
    let reader = cryptovol_core::FileBlockReader::open(&path).expect("fixture readable");
    let opened = cryptovol_tcvc::open_aes_sha512_volume(&reader, b"test-password")
        .expect("fixture opens with test-password");
    let data = opened.data_reader(&reader).expect("data reader");
    let fs = cryptovol_fs_ntfs::NtfsFileSystem::open(&data).expect("NTFS parses");

    let mut sink = Vec::new();
    let result = fs.read_file_to_writer("/does-not-exist.txt", &mut sink);
    assert!(
        matches!(
            result,
            Err(cryptovol_fs_ntfs::NtfsError::PathNotFound { .. })
        ),
        "missing path must return PathNotFound, got {result:?}"
    );
}
