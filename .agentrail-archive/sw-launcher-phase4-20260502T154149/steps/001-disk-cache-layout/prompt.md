# Step 1: disk-cache-layout

Replace the in-process `HashMap` cache in `src/tool.rs` with
a content-addressed cache that survives between runs. This
is also the first step of the long-deferred sub-crate
extraction: pull `tool` + `cache` into a `sw-launcher-tool`
sub-crate so the main crate's module count finally drops
under its cap.

## What lands

### Sub-crate extraction (preferred)

Convert `sw-launcher` into a Cargo workspace:

```
sw-launcher/                <- crate root, becomes the workspace
  Cargo.toml                <- [workspace] table + [package] for the
                               binary crate
  src/                      <- main binary; uses sw-launcher-tool
  crates/
    sw-launcher-tool/
      Cargo.toml
      src/
        lib.rs              <- pub use {tool, cache};
        tool.rs             <- moved from sw-launcher/src/tool.rs
        cache.rs            <- new: disk cache implementation
```

The main crate's `src/manifest.rs`, `src/cli.rs`, etc. continue
to import via `use sw_launcher_tool::{Tool, ToolKind, ...};`.

If the workspace conversion blows step scope (cargo metadata,
re-exports, test fixtures, doc links), fall back to a
single-crate `src/cache.rs` and document the deferral.
Either way is acceptable; the constraint is "ship a working
disk cache, no scope creep".

### Disk cache implementation

On-disk layout under `${XDG_CACHE_HOME:-$HOME/.cache}/sw-launch/`:

```
artifacts/<sha256-of-cache-key>/
  <stem>.<ext>           <- the artifact bytes
  provenance.toml        <- tool path/version, input sha,
                            extra_args, timestamp, host
index.toml               <- cache-key -> dir, last-accessed
```

API surface (in `cache.rs`):

```rust
pub struct Cache { root: PathBuf }

impl Cache {
    pub fn open() -> Result<Self>;  // resolves XDG_CACHE_HOME
    pub fn open_at(root: PathBuf) -> Result<Self>;  // for tests
    pub fn get_or_fill<F>(&self, key: &CacheKey, fill: F) -> Result<PathBuf>
    where F: FnOnce(&Path) -> Result<()>;  // fill writes to a temp dir
}

pub struct CacheKey {
    pub tool_path: PathBuf,        // canonicalized
    pub input_sha: String,         // hex
    pub extra_args: Vec<String>,   // for p24-load --load-addr et al
    pub output_stem: String,       // for the artifact filename
    pub output_ext: String,
}

impl CacheKey {
    pub fn digest(&self) -> String;  // sha256 of canonical encoding
}
```

Behaviour:

- `get_or_fill` looks up `artifacts/<digest>/`. If present and
  the artifact's sha matches `provenance.toml`'s recorded
  output_sha, return the path.
- On miss (or sha mismatch from a partial / corrupted entry),
  create a temp dir under the cache root, run `fill`, sha the
  output, write `provenance.toml`, atomically rename the temp
  dir to `artifacts/<digest>/`.
- Concurrent calls for the same key serialize via `flock(2)`
  on a per-key lock file under `locks/<digest>.lock`. Only
  one `fill` runs.
- Phase 1-3's in-process `HashMap` becomes a thin process-
  local layer in front of `Cache`: HashMap hit -> return;
  miss -> Cache::get_or_fill -> insert into HashMap on
  success.

`Tool::build` (currently in `src/tool.rs`) becomes the
`fill` closure callsite. The cache key is the existing
in-process key (canonical tool path + sha256(input) +
extra_args).

### Tests

In the new sub-crate's `tests/` (or in `tests/cache_unit.rs`
if not split):

1. **cache_hit_returns_same_path**: pre-seed cache with a
   known artifact + provenance; `get_or_fill` returns the
   path without invoking `fill`.
2. **cache_miss_runs_fill_once**: empty cache; track fill
   invocations via an `AtomicUsize`; `get_or_fill` called
   twice for the same key results in fill count == 1.
3. **corrupt_artifact_is_refilled**: pre-seed a cache entry
   whose artifact bytes don't match the recorded output_sha;
   next `get_or_fill` re-runs fill.
4. **concurrent_fills_are_serialized**: two threads call
   `get_or_fill` for the same key; assert fill is called
   exactly once and both threads see the same returned path.
   (Use `std::thread::spawn` + a `Barrier`; `flock` ensures
   serialization on Unix.)
5. **second_run_skips_assembler**: integration test in
   `tests/scenario_a.rs` (or new `tests/cache_persist.rs`)
   that runs `sw-launch run echo` twice in the same tempdir
   with `XDG_CACHE_HOME` set to the tempdir; assert the
   second run's wall time < some loose bound and that the
   cor24-run --assemble invocation count tracked via a
   sentinel file is exactly 1. If wall-time assertions are
   flaky, swap to a stub tool that touches a counter file.

### Pre-existing tests

All Phase 1-3 tests must still pass with the disk cache
active. The integration tests need to either (a) honor
`XDG_CACHE_HOME` set to a tempdir per test, or (b) pass an
explicit `--cache-dir <tempdir>` flag if we add one. Pick
one approach and apply it consistently. The CLI flag is
preferred because `XDG_CACHE_HOME` is process-global and
multi-test parallelism could collide; an explicit
`SW_LAUNCH_CACHE_DIR` env var honored only by the binary
under test is the cleanest pattern.

## Pre-commit gate

Standard:

- `cargo fmt --check`
- `cargo clippy --all-targets --all-features -- -D warnings`
- `cargo test` (must include the new cache tests + all
  Phase 1-3 tests)
- `markdown-checker -f` on any docs touched
- `sw-checklist` (the 9 carry-forward fails should *drop*
  if the sub-crate extraction lands; document the count
  delta either way)

## Done when

- `sw-launch run echo` (Phase 1 fixture) twice in a row
  reads `XDG_CACHE_HOME` and only assembles once.
- `sw-launch run pcode-hello` and `sw-launch run nested-demo`
  similarly skip pa24r / p24-load on the second run.
- All four cache unit tests pass.
- A `sub-crate extraction did / did-not happen` decision is
  documented in the step commit (and if it did, the carry-
  forward sw-checklist count drops; if it did not, the same
  9 fails carry into step 2).

## Citations

- `docs/design.md` "Cache key" (lines 1172-1195) -- the cache
  key shape this step persists.
- Phase 4 plan: `docs/saga-phase4-plan.md` step 1.

## Scope guardrails

- Do **not** ship cache subcommands (`list`, `explain`,
  `clean`) -- that's step 2.
- Do **not** add lockfile logic -- that's step 3.
- Do **not** add a TTL / eviction policy -- step 2's `clean`
  subcommand does that.
- The disk cache is content-addressed; this step does not
  add any vendor-resolution semantics. A vendored input that
  changes its bytes naturally invalidates its cache entry
  via the input_sha.