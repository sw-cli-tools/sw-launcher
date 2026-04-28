//! Unit tests for `sw_launcher::tool`. The integration tests in
//! `tests/assembler.rs` cover the real `cor24-run` invocation;
//! these tests use a no-op binary to verify memoization and the
//! cache-key derivation without depending on cor24-run on PATH.

use std::fs;
use std::io::Write;

use camino::Utf8PathBuf;
use sw_launcher::tool::{Assembler, BuildJob};

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
fn from_path_finds_cor24_run_when_present() {
    // We don't assert it's there; we just assert the lookup
    // doesn't panic and returns either Ok or a CLI error.
    let _ = Assembler::from_path();
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
