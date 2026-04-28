//! Unit tests for `sw_launcher::listing`. Real cor24-run listings
//! emit `<symbol>:` on a column-21-leading-space line followed by
//! one or more `<addr>: <bytes>    <instr>` lines. A symbol whose
//! reservation has no emitted bytes (e.g. `.skip 16` BSS) gets
//! the address of the next data line OR the end-of-file address.

use std::fs;

use camino::Utf8PathBuf;
use sw_launcher::listing::Listing;

fn fixture_path(name: &str) -> Utf8PathBuf {
    let mut p = Utf8PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.push("tests/fixtures");
    p.push(name);
    p
}

#[test]
fn echo_lst_resolves_start_symbol() {
    let text = fs::read_to_string(fixture_path("echo.lst")).unwrap();
    let lst = Listing::parse(&text);
    assert_eq!(lst.resolve("_start"), Some(0x000000));
    assert_eq!(lst.resolve("nonexistent"), None);
}

#[test]
fn multi_lst_resolves_three_symbols_including_trailing_label() {
    let text = fs::read_to_string(fixture_path("multi.lst")).unwrap();
    let lst = Listing::parse(&text);
    assert_eq!(lst.resolve("_start"), Some(0x0000));
    assert_eq!(lst.resolve("code_ptr"), Some(0x0007));
    // eval_stack is the trailing label with no emitted bytes;
    // its address is the byte immediately after the last data
    // line (0x0007 + 3 = 0x000A).
    assert_eq!(lst.resolve("eval_stack"), Some(0x000A));
}

#[test]
fn parse_handles_empty_or_garbage_input() {
    assert_eq!(Listing::parse("").resolve("anything"), None);
    assert_eq!(Listing::parse("not a listing").resolve("foo"), None);
}

#[test]
fn parse_handles_consecutive_labels() {
    // Two labels in a row, both attaching to the next data line.
    let lst = Listing::parse(
        "                    foo:\n\
                             bar:\n\
         0010: 00 11 22       .word 0\n",
    );
    assert_eq!(lst.resolve("foo"), Some(0x0010));
    assert_eq!(lst.resolve("bar"), Some(0x0010));
}
