use std::process::Command;

#[test]
fn ody_host_binary_exists() {
    assert_eq!(env!("CARGO_PKG_NAME"), "ody-host");
}

#[test]
fn serve_subcommand_help_lists_transport_flags() {
    let output = Command::new(env!("CARGO_BIN_EXE_ody-host"))
        .args(["serve", "--help"])
        .output()
        .expect("failed to execute ody-host");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "ody-host serve --help should exit successfully; stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        stdout.contains("serve"),
        "help should mention serve subcommand"
    );
    assert!(stdout.contains("--stdio"), "help should list --stdio flag");
}

#[test]
fn mixed_global_flags_and_serve_are_rejected() {
    let output = Command::new(env!("CARGO_BIN_EXE_ody-host"))
        .args(["--stdio", "serve", "--socket-path", "/tmp/ody-mixed.sock"])
        .output()
        .expect("failed to execute ody-host");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !output.status.success(),
        "mixed global flags and serve should fail"
    );
    assert!(
        stderr.contains("global flags") || stderr.contains("serve"),
        "error should mention global flags or serve; got: {stderr}"
    );
}
