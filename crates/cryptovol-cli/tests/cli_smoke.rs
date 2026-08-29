#![allow(
    missing_docs,
    clippy::expect_used,
    clippy::panic,
    clippy::unwrap_used,
    reason = "CLI integration tests use direct assertions and process setup helpers"
)]
#![cfg_attr(
    unix,
    allow(
        unsafe_code,
        reason = "Unix CLI tests isolate child processes with pre_exec/setsid"
    )
)]

use std::fs;
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

use cryptovol_cli::commands::CliExitCode;
use cryptovol_cli::{
    render_exfat_entry, render_exfat_entry_long, render_probe_fs_success, ProbeFsOutput,
};
use cryptovol_fs_exfat::{ExfatAttributes, ExfatEntry, ExfatTimestamp};
use cryptovol_tcvc::FilesystemProbeCandidate;

fn new_command(args: &[&str]) -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_cryptovol"));
    cmd.args(args);
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        unsafe {
            cmd.pre_exec(|| {
                libc::setsid();
                Ok(())
            });
        }
    }
    cmd
}

fn cryptovol(args: &[&str]) -> Output {
    new_command(args)
        .stdin(Stdio::null())
        .output()
        .expect("cryptovol binary should run")
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

#[test]
fn cli_exit_code_values_match_policy() {
    assert_eq!(CliExitCode::Success.code(), 0);
    assert_eq!(CliExitCode::GenericError.code(), 1);
    assert_eq!(CliExitCode::InvalidArguments.code(), 2);
    assert_eq!(CliExitCode::UnsupportedFormat.code(), 3);
    assert_eq!(CliExitCode::AuthenticationFailed.code(), 4);
    assert_eq!(CliExitCode::FilesystemNotRecognized.code(), 5);
    assert_eq!(CliExitCode::ExtractionFailed.code(), 6);
}

fn test_file_path(name: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time should be after Unix epoch")
        .as_nanos();

    std::env::temp_dir().join(format!(
        "cryptovol-cli-{name}-{}-{unique}",
        std::process::id()
    ))
}

#[test]
fn binary_starts() {
    let output = cryptovol(&[]);

    assert!(
        output.status.success(),
        "expected success, stderr: {}",
        stderr(&output)
    );
}

#[test]
fn help_works() {
    let output = cryptovol(&["--help"]);

    assert!(
        output.status.success(),
        "expected help to succeed, stderr: {}",
        stderr(&output)
    );
    assert!(
        stdout(&output).contains("Usage:"),
        "expected clap help output, stdout: {}",
        stdout(&output)
    );
}

#[test]
fn test_open_help_does_not_expose_password_sources() {
    let output = cryptovol(&["test-open", "--help"]);

    assert!(
        output.status.success(),
        "expected test-open help to succeed, stderr: {}",
        stderr(&output)
    );
    let stdout = stdout(&output);
    assert!(
        !stdout.contains("--password"),
        "test-open help must not expose password command-line flags: {stdout}"
    );
    assert!(
        !stdout.contains("--password-file"),
        "test-open help must not expose password-file support: {stdout}"
    );
}

#[test]
fn top_level_help_lists_probe_fs() {
    let output = cryptovol(&["--help"]);

    assert!(
        output.status.success(),
        "expected help to succeed, stderr: {}",
        stderr(&output)
    );
    let stdout = stdout(&output);
    assert!(
        stdout.contains("probe-fs"),
        "top-level help should list probe-fs command: {stdout}"
    );
}

#[test]
fn probe_fs_help_does_not_expose_password_or_decrypted_output_sources() {
    let output = cryptovol(&["probe-fs", "--help"]);

    assert!(
        output.status.success(),
        "expected probe-fs help to succeed, stderr: {}",
        stderr(&output)
    );
    let stdout = stdout(&output);
    assert!(
        !stdout.contains("--password"),
        "probe-fs help must not expose password command-line flags: {stdout}"
    );
    assert!(
        !stdout.contains("--password-file"),
        "probe-fs help must not expose password-file support: {stdout}"
    );
    assert!(
        !stdout.contains("decrypted-output"),
        "probe-fs help must not expose raw decrypted output controls: {stdout}"
    );
}

#[test]
fn info_existing_file_reports_metadata() {
    let path = test_file_path("info-existing");
    fs::write(&path, b"hello cryptovol").expect("test file should be writable");
    let path_arg = path.to_string_lossy().into_owned();

    let output = cryptovol(&["info", &path_arg]);

    assert!(
        output.status.success(),
        "expected info to succeed, stderr: {}",
        stderr(&output)
    );
    let stdout = stdout(&output);
    assert!(
        stdout.contains(&format!("File: {path_arg}")),
        "expected file path in stdout: {stdout}"
    );
    assert!(
        stdout.contains("Size: 15 bytes"),
        "expected file size in stdout: {stdout}"
    );
    assert!(
        stdout.contains("Readable: yes"),
        "expected readable status in stdout: {stdout}"
    );
    assert!(
        stdout.contains("Read-only mode: yes"),
        "expected read-only status in stdout: {stdout}"
    );
    assert!(
        stdout.contains("Crypto backend: not opened by this command"),
        "expected crypto backend status in stdout: {stdout}"
    );
    assert!(
        stdout.contains("Filesystem: not detected by this command"),
        "expected filesystem status in stdout: {stdout}"
    );

    fs::remove_file(path).expect("test file should be removable");
}

#[test]
fn info_normal_file_reports_tcvc_candidates() {
    let path = test_file_path("info-tcvc-normal");
    fs::write(&path, vec![0x5a; 1024]).expect("test file should be writable");
    let path_arg = path.to_string_lossy().into_owned();

    let output = cryptovol(&["info", &path_arg]);

    assert!(
        output.status.success(),
        "expected info to succeed, stderr: {}",
        stderr(&output)
    );
    let stdout = stdout(&output);
    assert!(
        stdout.contains(&format!("File: {path_arg}")),
        "expected existing metadata in stdout: {stdout}"
    );
    assert!(
        stdout.contains("Size: 1024 bytes"),
        "expected existing size metadata in stdout: {stdout}"
    );
    assert!(
        stdout.contains("TC/VC backend:"),
        "expected TC/VC backend section in stdout: {stdout}"
    );
    assert!(
        stdout.contains("Primary header candidate: offset 0, length 512 bytes, readable yes"),
        "expected primary candidate output in stdout: {stdout}"
    );
    assert!(
        stdout.contains("Backup header candidate: offset 512, length 512 bytes, readable yes"),
        "expected backup candidate output in stdout: {stdout}"
    );
    assert!(
        stdout.contains("Header interpretation: not attempted by this command"),
        "expected header interpretation status in stdout: {stdout}"
    );
    assert!(
        stdout.contains("Decryption: not attempted by this command"),
        "expected decryption status in stdout: {stdout}"
    );

    fs::remove_file(path).expect("test file should be removable");
}

#[test]
fn info_tiny_file_reports_tcvc_too_small() {
    let path = test_file_path("info-tcvc-tiny");
    fs::write(&path, vec![0x11; 128]).expect("test file should be writable");
    let path_arg = path.to_string_lossy().into_owned();

    let output = cryptovol(&["info", &path_arg]);

    assert!(
        output.status.success(),
        "expected info to succeed, stderr: {}",
        stderr(&output)
    );
    let stdout = stdout(&output);
    assert!(
        stdout.contains("Size: 128 bytes"),
        "expected existing size metadata in stdout: {stdout}"
    );
    assert!(
        stdout.contains("TC/VC backend:"),
        "expected TC/VC backend section in stdout: {stdout}"
    );
    assert!(
        stdout.contains("Status: file too small for TC/VC header candidates"),
        "expected too-small status in stdout: {stdout}"
    );
    assert!(
        stdout.contains("Required minimum: 512 bytes"),
        "expected required minimum in stdout: {stdout}"
    );

    fs::remove_file(path).expect("test file should be removable");
}

#[test]
fn info_exact_header_file_reports_tcvc_overlap() {
    let path = test_file_path("info-tcvc-exact");
    fs::write(&path, vec![0x22; 512]).expect("test file should be writable");
    let path_arg = path.to_string_lossy().into_owned();

    let output = cryptovol(&["info", &path_arg]);

    assert!(
        output.status.success(),
        "expected info to succeed, stderr: {}",
        stderr(&output)
    );
    let stdout = stdout(&output);
    assert!(
        stdout.contains("Size: 512 bytes"),
        "expected existing size metadata in stdout: {stdout}"
    );
    assert!(
        stdout.contains("TC/VC backend:"),
        "expected TC/VC backend section in stdout: {stdout}"
    );
    assert!(
        stdout.contains("Primary header candidate: offset 0, length 512 bytes, readable yes"),
        "expected primary candidate output in stdout: {stdout}"
    );
    assert!(
        stdout.contains("Backup header candidate: overlaps primary candidate"),
        "expected backup overlap output in stdout: {stdout}"
    );
    assert!(
        stdout.contains("Header interpretation: not attempted by this command"),
        "expected header interpretation status in stdout: {stdout}"
    );
    assert!(
        stdout.contains("Decryption: not attempted by this command"),
        "expected decryption status in stdout: {stdout}"
    );

    fs::remove_file(path).expect("test file should be removable");
}

#[test]
fn info_missing_file_fails() {
    let path = test_file_path("info-missing");
    let path_arg = path.to_string_lossy().into_owned();

    let output = cryptovol(&["info", &path_arg]);

    assert!(
        !output.status.success(),
        "expected info to fail for missing file, stdout: {}",
        stdout(&output)
    );
    assert_eq!(output.status.code(), Some(1));
    let stderr = stderr(&output);
    assert!(
        stderr.contains("failed to open file") || stderr.contains("could not open file"),
        "expected clear missing-file error, stderr: {stderr}"
    );
}

#[test]
fn test_open_reports_failure_instead_of_placeholder() {
    let path = test_file_path("test-open-random");
    fs::write(&path, vec![0x5a; 1024]).expect("test file should be writable");
    let path_arg = path.to_string_lossy().into_owned();

    let output = cryptovol(&["test-open", &path_arg]);

    assert!(
        !stdout(&output).contains("not implemented"),
        "test-open must no longer print placeholder output"
    );
    assert!(
        !output.status.success(),
        "expected random data test-open to fail, stdout: {}, stderr: {}",
        stdout(&output),
        stderr(&output)
    );
    assert_eq!(output.status.code(), Some(4));
    assert!(
        stdout(&output)
            .contains("Could not open volume: authentication failed or unsupported parameters.")
            || stderr(&output).contains(
                "Could not open volume: authentication failed or unsupported parameters."
            ),
        "expected stable failure message, stdout: {}, stderr: {}",
        stdout(&output),
        stderr(&output)
    );

    fs::remove_file(path).expect("test file should be removable");
}

#[test]
fn probe_fs_reports_auth_safe_failure_for_random_input() {
    let path = test_file_path("probe-fs-random");
    fs::write(&path, vec![0x5a; 1024]).expect("test file should be writable");
    let path_arg = path.to_string_lossy().into_owned();

    let mut child = new_command(&["probe-fs", &path_arg])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("cryptovol probe-fs should start");

    {
        use std::io::Write;

        let stdin = child.stdin.as_mut().expect("stdin should be piped");
        stdin
            .write_all(b"wrong-test-password\n")
            .expect("password input should be writable");
    }

    let output = child
        .wait_with_output()
        .expect("cryptovol probe-fs should exit");

    assert!(
        !stdout(&output).contains("not implemented"),
        "probe-fs must not print placeholder output"
    );
    assert!(
        !output.status.success(),
        "expected random data probe-fs to fail, stdout: {}, stderr: {}",
        stdout(&output),
        stderr(&output)
    );
    assert_eq!(output.status.code(), Some(4));
    assert!(
        stdout(&output).contains(
            "Could not probe filesystem: authentication failed or unsupported parameters."
        ) || stderr(&output).contains(
            "Could not probe filesystem: authentication failed or unsupported parameters."
        ),
        "expected stable auth-safe failure message, stdout: {}, stderr: {}",
        stdout(&output),
        stderr(&output)
    );

    fs::remove_file(path).expect("test file should be removable");
}

#[test]
#[ignore = "requires scripts/test-with-veracrypt-fixtures.sh to generate CRYPTOVOL_TEST_CONTAINER"]
fn probe_fs_reports_safe_fixture_success_output() {
    let stdout = render_probe_fs_success(ProbeFsOutput {
        backend: "tcvc",
        data_offset: 131_072,
        data_size: 20 * 1024 * 1024 - 131_072,
        candidate: FilesystemProbeCandidate::FatLike,
    });

    assert!(stdout.contains("TC/VC volume opened successfully."));
    assert!(stdout.contains("Backend: tcvc"));
    assert!(stdout.contains("Encryption: AES-XTS"));
    assert!(stdout.contains("KDF/Hash: SHA-512"));
    assert!(stdout.contains("Read-only: yes"));
    assert!(stdout.contains("First sector: readable"));
    assert!(stdout.contains("Candidate: FAT-like"));
    assert!(
        stdout.contains("FAT listing/extraction: available for supported short-name FAT fixtures")
    );
    assert!(stdout.contains("Long filename support: available"));
    assert!(stdout.contains("Directory extraction: not supported"));
    for secret in [
        "test-password",
        "header key",
        "master key",
        "data key",
        "raw decrypted",
    ] {
        assert!(
            !stdout.contains(secret),
            "probe-fs success output must not expose secret marker {secret:?}: {stdout}"
        );
    }
}

#[test]
fn extract_help_shows_overwrite_and_parents_flags() {
    let output = cryptovol(&["extract", "--help"]);
    assert!(output.status.success());
    assert!(
        stdout(&output).contains("--overwrite"),
        "extract --help should mention --overwrite"
    );
    assert!(
        stdout(&output).contains("--parents"),
        "extract --help should mention --parents"
    );
}

#[test]
fn extract_is_not_a_placeholder_when_required_args_given() {
    let output = cryptovol(&["extract", "--help"]);
    assert!(output.status.success());
    assert!(
        !stdout(&output).contains("not implemented"),
        "extract --help should not contain placeholder text"
    );
}

#[test]
fn ls_is_wired_not_a_placeholder() {
    let output = cryptovol(&["ls", "sample.hc", "/"]);

    assert!(
        !stdout(&output).contains("not implemented"),
        "ls must not print placeholder output, stdout: {}",
        stdout(&output)
    );
    assert!(
        !output.status.success(),
        "expected ls to fail for missing file, stdout: {}, stderr: {}",
        stdout(&output),
        stderr(&output)
    );
    assert_eq!(output.status.code(), Some(1));
}

#[test]
fn invalid_command_fails() {
    let output = cryptovol(&["unknown-command"]);

    assert!(
        !output.status.success(),
        "expected invalid command to fail, stdout: {}",
        stdout(&output)
    );
    assert_eq!(output.status.code(), Some(2));
}

// --- --pim flag tests (will fully pass after T-007 adds --pim CLI support) ---

#[test]
fn test_open_help_includes_pim() {
    // TODO: will pass after T-007 adds --pim flag
    let output = cryptovol(&["test-open", "--help"]);
    assert!(
        output.status.success(),
        "test-open --help must succeed, stderr: {}",
        stderr(&output)
    );
    let out = stdout(&output);
    assert!(
        out.contains("--pim"),
        "test-open --help must list --pim: {out}"
    );
}

#[test]
fn probe_fs_help_includes_pim() {
    // TODO: will pass after T-007 adds --pim flag
    let output = cryptovol(&["probe-fs", "--help"]);
    assert!(
        output.status.success(),
        "probe-fs --help must succeed, stderr: {}",
        stderr(&output)
    );
    let out = stdout(&output);
    assert!(
        out.contains("--pim"),
        "probe-fs --help must list --pim: {out}"
    );
}

#[test]
fn ls_help_includes_pim() {
    // TODO: will pass after T-007 adds --pim flag
    let output = cryptovol(&["ls", "--help"]);
    assert!(
        output.status.success(),
        "ls --help must succeed, stderr: {}",
        stderr(&output)
    );
    let out = stdout(&output);
    assert!(out.contains("--pim"), "ls --help must list --pim: {out}");
}

#[test]
fn extract_help_includes_pim() {
    // TODO: will pass after T-007 adds --pim flag
    let output = cryptovol(&["extract", "--help"]);
    assert!(
        output.status.success(),
        "extract --help must succeed, stderr: {}",
        stderr(&output)
    );
    let out = stdout(&output);
    assert!(
        out.contains("--pim"),
        "extract --help must list --pim: {out}"
    );
}

#[test]
fn test_open_invalid_pim_exits_nonzero() {
    // --pim with a non-integer must cause a non-zero exit (argument parse error).
    let output = cryptovol(&["test-open", "nonexistent-container", "--pim", "notanumber"]);
    assert!(
        !output.status.success(),
        "--pim notanumber must cause non-zero exit, stdout: {}, stderr: {}",
        stdout(&output),
        stderr(&output)
    );
}

#[test]
fn test_open_pim_flag_accepted() {
    // --pim with a valid integer must NOT cause exit code 2 (unknown-argument error).
    // The command will fail for other reasons (missing file), but not from argument rejection.
    // TODO: will pass after T-007 adds --pim flag
    let output = cryptovol(&["test-open", "nonexistent-container.hc", "--pim", "500"]);
    assert_ne!(
        output.status.code(),
        Some(2),
        "--pim 500 must not cause an argument-parse error (exit 2), stdout: {}, stderr: {}",
        stdout(&output),
        stderr(&output)
    );
}

#[test]
fn probe_fs_renders_exfat_candidate() {
    let output = render_probe_fs_success(ProbeFsOutput {
        backend: "tcvc",
        data_offset: 131_072,
        data_size: 104_857_600,
        candidate: FilesystemProbeCandidate::ExFat,
    });
    assert!(
        output.contains("exFAT"),
        "probe-fs output must contain 'exFAT' for ExFat candidate: {output}"
    );
}

#[test]
fn render_exfat_entry_short_format() {
    let entry = ExfatEntry::from_metadata(
        "Emoji Rocket \u{1F680} Test.txt".to_string(),
        false,
        22,
        ExfatAttributes::default(),
        None,
        None,
        None,
    );
    let result = render_exfat_entry(&entry);
    assert!(
        result.contains("Emoji Rocket \u{1F680} Test.txt"),
        "render_exfat_entry must include filename: {result}"
    );
}

#[test]
fn render_exfat_entry_long_format() {
    let ts = ExfatTimestamp {
        year: 2026,
        month: 6,
        day: 29,
        hour: 14,
        minute: 31,
        second: 0,
    };
    let entry = ExfatEntry::from_metadata(
        "Emoji Rocket \u{1F680} Test.txt".to_string(),
        false,
        22,
        ExfatAttributes::default(),
        None,
        Some(ts),
        None,
    );
    let result = render_exfat_entry_long(&entry);
    assert!(
        result.contains("22"),
        "long format must include size: {result}"
    );
    assert!(
        result.contains("2026-06-29"),
        "long format must include date: {result}"
    );
    assert!(
        result.contains("Emoji Rocket \u{1F680} Test.txt"),
        "long format must include filename: {result}"
    );
    assert!(
        result.starts_with('-'),
        "long format for a file must start with '-': {result}"
    );
}

#[test]
fn render_exfat_dir_entry_long_format() {
    let entry = ExfatEntry::from_metadata(
        "Folder With Spaces".to_string(),
        true,
        0,
        ExfatAttributes::default(),
        None,
        None,
        None,
    );
    let result = render_exfat_entry_long(&entry);
    assert!(
        result.starts_with('d'),
        "long format for a directory must start with 'd': {result}"
    );
}
