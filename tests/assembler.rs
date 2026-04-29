//! Integration test for `Assembler` against the real `cor24-run`
//! binary. Skipped (with a one-line note on stderr) if cor24-run
//! is not on PATH.

use std::fs;

use camino::{Utf8Path, Utf8PathBuf};
use sw_launcher::listing::Listing;
use sw_launcher::tool::{BuildJob, SourceSpec, Tool, ToolKind};

#[test]
fn assemble_echo_fixture_with_real_cor24_run() {
    let Ok(mut asm) = Tool::from_source(
        &SourceSpec::FromPath {
            binary: "cor24-run".into(),
        },
        Utf8Path::new("/"),
        ToolKind::Assembler,
    ) else {
        eprintln!("cor24-run not on PATH; skipping integration test");
        return;
    };
    let mut input = Utf8PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    input.push("tests/fixtures/echo.s");

    let tmp = std::env::temp_dir().join(format!("sw-launcher-asm-{}", std::process::id()));
    let _ = fs::create_dir_all(&tmp);
    let tmp = Utf8PathBuf::from_path_buf(tmp).unwrap();

    let job = BuildJob {
        layer_name: "echo".into(),
        input,
        output_bin: tmp.join("echo.bin"),
        output_lst: tmp.join("echo.lst"),
        extra_args: Vec::new(),
    };
    let out = asm.build(&job).expect("first assemble");
    assert!(out.artifact.is_file());
    assert!(out.listing.is_file());
    let bin_size = fs::metadata(out.artifact.as_std_path()).unwrap().len();
    // echo.s is 4 + 4 + 2 + 3 = 13 bytes per cor24-run --assemble.
    assert_eq!(bin_size, 13);

    // Listing has the _start label resolving to 0.
    let text = fs::read_to_string(out.listing.as_std_path()).unwrap();
    let lst = Listing::parse(&text);
    assert_eq!(lst.resolve("_start"), Some(0x000000));

    // Second build is a memoization hit.
    asm.build(&job).expect("second assemble");
    assert_eq!(asm.spawn_count, 1);
}
