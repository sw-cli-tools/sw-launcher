//! Unit tests for `sw_launcher::error`.

use sw_launcher::error::{Error, ErrorCode};

#[test]
fn error_code_formats_with_four_digits() {
    assert_eq!(ErrorCode(1).to_string(), "E0001");
    assert_eq!(ErrorCode(90).to_string(), "E0090");
    assert_eq!(ErrorCode(127).to_string(), "E0127");
}

#[test]
fn display_impls_include_codes_and_payload() {
    let e = Error::not_implemented("run");
    let s = e.to_string();
    assert!(s.contains("E0090"), "got: {s}");
    assert!(s.contains("not yet implemented"), "got: {s}");
    assert!(s.contains("run"), "got: {s}");

    let e = Error::cli("missing scenario");
    let s = e.to_string();
    assert!(s.contains("E0091"), "got: {s}");
    assert!(s.contains("missing scenario"), "got: {s}");

    let inner = std::io::Error::new(std::io::ErrorKind::NotFound, "no such file");
    let e: Error = inner.into();
    let s = e.to_string();
    assert!(s.contains("E0092"), "got: {s}");
    assert!(s.contains("I/O"), "got: {s}");
}

#[test]
fn exit_codes_are_distinct_and_nonzero() {
    let ni = Error::not_implemented("x").exit_code();
    let cli = Error::cli("x").exit_code();
    let io = Error::Io {
        source: std::io::Error::other("x"),
    }
    .exit_code();
    for code in [ni, cli, io] {
        assert_ne!(code, 0, "exit codes must be non-zero");
    }
    assert_ne!(ni, cli);
    assert_ne!(cli, io);
}
