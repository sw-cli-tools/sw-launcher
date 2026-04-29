//! Unit tests for `sw_launcher::tool`. The integration tests in
//! `tests/assembler.rs` cover the real `cor24-run` invocation;
//! these tests use a no-op binary to verify memoization and the
//! cache-key derivation without depending on cor24-run on PATH.

use std::fs;
use std::io::Write;

use camino::{Utf8Path, Utf8PathBuf};
use sw_launcher::tool::{Assembler, BuildJob, SourceSpec, Tool, ToolKind};

fn make_input(dir: &Utf8PathBuf, name: &str, body: &[u8]) -> Utf8PathBuf {
    let p = dir.join(name);
    let mut f = fs::File::create(&p).unwrap();
    f.write_all(body).unwrap();
    p
}

/// On Unix systems we can spawn `/usr/bin/true` (or `/bin/true`)
/// to verify spawn count without depending on cor24-run; the
/// tool's argv is irrelevant since `true` ignores it.
fn no_op_tool() -> Option<Utf8PathBuf> {
    for candidate in ["/usr/bin/true", "/bin/true"] {
        let p = Utf8PathBuf::from(candidate);
        if p.exists() {
            return Some(p);
        }
    }
    None
}

#[test]
fn from_source_with_from_path_for_cor24_run_doesnt_panic() {
    // We don't assert it's there; we just assert the lookup
    // doesn't panic and returns either Ok or a CLI error.
    let _ = Tool::from_source(
        &SourceSpec::FromPath {
            binary: "cor24-run".into(),
        },
        Utf8Path::new("/"),
        ToolKind::Assembler,
    );
}

#[test]
fn build_with_same_input_is_cached_after_first_spawn() {
    let Some(tool) = no_op_tool() else {
        eprintln!("/usr/bin/true not present; skipping");
        return;
    };
    let tmp = tempdir_utf8();
    let input = make_input(&tmp, "in.s", b"; tiny program\n_start:\n");
    let mut asm = Assembler::new(tool);
    let job = BuildJob {
        layer_name: "tiny".into(),
        input: input.clone(),
        output_bin: tmp.join("out.bin"),
        output_lst: tmp.join("out.lst"),
        extra_args: Vec::new(),
    };
    asm.build(&job).expect("first build");
    asm.build(&job).expect("second build (memoized)");
    assert_eq!(
        asm.spawn_count, 1,
        "expected one spawn, got {}",
        asm.spawn_count
    );
}

#[test]
fn build_with_different_inputs_spawns_twice() {
    let Some(tool) = no_op_tool() else {
        eprintln!("/usr/bin/true not present; skipping");
        return;
    };
    let tmp = tempdir_utf8();
    let in_a = make_input(&tmp, "a.s", b"; a\n");
    let in_b = make_input(&tmp, "b.s", b"; different bytes\n");
    let mut asm = Assembler::new(tool);
    let job_a = BuildJob {
        layer_name: "a".into(),
        input: in_a,
        output_bin: tmp.join("a.bin"),
        output_lst: tmp.join("a.lst"),
        extra_args: Vec::new(),
    };
    let job_b = BuildJob {
        layer_name: "b".into(),
        input: in_b,
        output_bin: tmp.join("b.bin"),
        output_lst: tmp.join("b.lst"),
        extra_args: Vec::new(),
    };
    asm.build(&job_a).expect("a");
    asm.build(&job_b).expect("b");
    assert_eq!(asm.spawn_count, 2);
}

#[test]
fn build_with_extra_args_is_distinct_in_cache() {
    let Some(tool) = no_op_tool() else {
        eprintln!("/usr/bin/true not present; skipping");
        return;
    };
    let tmp = tempdir_utf8();
    let input = make_input(&tmp, "x.s", b"; x\n");
    let mut asm = Assembler::new(tool);
    let mut job = BuildJob {
        layer_name: "x".into(),
        input,
        output_bin: tmp.join("x.bin"),
        output_lst: tmp.join("x.lst"),
        extra_args: Vec::new(),
    };
    asm.build(&job).unwrap();
    job.extra_args.push("--base-addr".into());
    job.extra_args.push("0x010000".into());
    asm.build(&job).unwrap();
    assert_eq!(asm.spawn_count, 2);
}

fn tempdir_utf8() -> Utf8PathBuf {
    let base = std::env::temp_dir();
    let unique = format!(
        "sw-launcher-tool-test-{}-{}",
        std::process::id(),
        rand_suffix()
    );
    let dir = base.join(unique);
    fs::create_dir_all(&dir).unwrap();
    Utf8PathBuf::from_path_buf(dir).unwrap()
}

fn rand_suffix() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let n = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("{n:x}")
}

// ---- v1.2 step-1 additions: pcode kind + source resolution ------

#[test]
fn pcode_kind_memoizes_just_like_assembler() {
    let Some(no_op) = no_op_tool() else {
        eprintln!("/usr/bin/true not present; skipping");
        return;
    };
    let tmp = tempdir_utf8();
    let input = make_input(&tmp, "h.spc", b".end\n");
    let mut tool = Tool::new(no_op);
    tool.kind = ToolKind::Pcode;
    let job = BuildJob {
        layer_name: "h".into(),
        input,
        output_bin: tmp.join("h.p24"),
        output_lst: Utf8PathBuf::new(),
        extra_args: Vec::new(),
    };
    tool.build(&job).expect("first build");
    tool.build(&job).expect("second build (memoized)");
    assert_eq!(tool.spawn_count, 1);
}

#[test]
fn pcode_kind_skips_listing_in_build_output() {
    let Some(no_op) = no_op_tool() else {
        eprintln!("/usr/bin/true not present; skipping");
        return;
    };
    let tmp = tempdir_utf8();
    let input = make_input(&tmp, "h.spc", b".end\n");
    let mut tool = Tool::new(no_op);
    tool.kind = ToolKind::Pcode;
    let job = BuildJob {
        layer_name: "h".into(),
        input,
        output_bin: tmp.join("h.p24"),
        output_lst: tmp.join("h.lst"),
        extra_args: Vec::new(),
    };
    let out = tool.build(&job).unwrap();
    assert_eq!(out.listing, Utf8PathBuf::new(), "pcode emits no listing");
}

#[test]
fn from_source_path_resolves_absolute_and_relative() {
    let Some(no_op) = no_op_tool() else {
        eprintln!("/usr/bin/true not present; skipping");
        return;
    };
    // Absolute: use the no-op binary path directly.
    let abs = SourceSpec::Path(no_op.clone());
    let t = Tool::from_source(&abs, Utf8Path::new("/"), ToolKind::Assembler).unwrap();
    assert_eq!(t.tool_path, no_op);

    // Relative: use a directory + a relative path; verify join.
    let dir = no_op.parent().unwrap();
    let rel = SourceSpec::Path(Utf8PathBuf::from(no_op.file_name().unwrap()));
    let t = Tool::from_source(&rel, dir, ToolKind::Assembler).unwrap();
    assert_eq!(t.tool_path, dir.join(no_op.file_name().unwrap()));
}

#[test]
fn from_source_from_path_resolves_or_errors_cleanly() {
    let exists = SourceSpec::FromPath {
        binary: "true".into(),
    };
    let res = Tool::from_source(&exists, Utf8Path::new("/"), ToolKind::Assembler);
    if no_op_tool().is_some() {
        assert!(res.is_ok(), "true should be on PATH");
    }

    let absent = SourceSpec::FromPath {
        binary: "definitely-not-a-real-binary-xyzzy".into(),
    };
    let err = Tool::from_source(&absent, Utf8Path::new("/"), ToolKind::Assembler).unwrap_err();
    assert!(err.to_string().contains("xyzzy"), "got: {err}");
}

#[test]
fn from_source_sibling_resolves_path_relative_to_config_dir() {
    let Some(no_op) = no_op_tool() else {
        eprintln!("/usr/bin/true not present; skipping");
        return;
    };
    // sibling { path = "..", artifact = "<base>/<name>" } where
    // base = no_op.parent().file_name() and name = no_op.file_name()
    // resolved relative to no_op.parent().parent() -> joins back to no_op.
    let parent = no_op.parent().unwrap();
    let grandparent = parent.parent().unwrap();
    let parent_name = parent.file_name().unwrap();
    let bin_name = no_op.file_name().unwrap();
    let spec = SourceSpec::Sibling {
        path: Utf8PathBuf::from(parent_name),
        artifact: Utf8PathBuf::from(bin_name),
    };
    let t = Tool::from_source(&spec, grandparent, ToolKind::Assembler).unwrap();
    assert_eq!(
        t.tool_path.canonicalize_utf8().ok(),
        no_op.canonicalize_utf8().ok()
    );
}

#[test]
fn from_source_sibling_missing_artifact_errors() {
    let spec = SourceSpec::Sibling {
        path: Utf8PathBuf::from("nowhere"),
        artifact: Utf8PathBuf::from("nothing"),
    };
    let err = Tool::from_source(&spec, Utf8Path::new("/"), ToolKind::Pcode).unwrap_err();
    assert!(
        err.to_string().contains("sibling tool not found"),
        "got: {err}"
    );
}

#[test]
fn pa24r_integration_assembles_hello_spc() {
    // Gated on pa24r being on PATH. The vendored binary at
    // ~/github/sw-embed/sw-cor24-pcode/target/release/pa24r is the
    // common location, but we don't probe sibling paths from a
    // unit test -- only PATH.
    let pa24r = match Tool::from_source(
        &SourceSpec::FromPath {
            binary: "pa24r".into(),
        },
        Utf8Path::new("/"),
        ToolKind::Pcode,
    ) {
        Ok(t) => t,
        Err(_) => {
            eprintln!("pa24r not on PATH; skipping pcode integration test");
            return;
        }
    };
    let mut tool = pa24r;
    let mut input = Utf8PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    input.push("tests/fixtures/scenario_b/hello.spc");
    let tmp = tempdir_utf8();
    let job = BuildJob {
        layer_name: "hello".into(),
        input,
        output_bin: tmp.join("hello.p24"),
        output_lst: Utf8PathBuf::new(),
        extra_args: Vec::new(),
    };
    let out = tool.build(&job).expect("pa24r should assemble hello.spc");
    let bytes = std::fs::metadata(out.artifact.as_std_path()).unwrap().len();
    assert!(
        bytes >= 18,
        ".p24 should have at least an 18-byte v1 header; got {bytes}"
    );
}
