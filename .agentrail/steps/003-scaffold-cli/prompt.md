# Step 3: scaffold-cli

Stand up the binary skeleton for `sw-launch`. TDD: write each test
before the matching code.

## What lands

- `Cargo.toml` adds: `clap = { version = "4", features = ["derive"] }`,
  `thiserror = "2"`, `anyhow = "1"`. Keep the dependency list
  minimal; serde/toml etc. land in step 4, not here.
- `src/main.rs`: thin entry point that calls into `cli::dispatch`.
- `src/cli.rs`: clap derive definitions for the subcommands listed in
  `docs/design.md` ("CLI surface"). Subcommands: `run`, `build`,
  `check`, `graph`, `cache list|explain|clean`, `vendor sync|status`,
  `doctor`. Each non-implemented action returns an error of kind
  `Error::NotImplemented` so they exit non-zero with a clean
  "not yet implemented" message. `--version` prints the cargo
  version (use `clap`'s built-in `#[command(version)]`).
- `src/error.rs`: `thiserror` enum with at least:
  `NotImplemented(&'static str)`, `Cli(String)`, `Io(#[from] std::io::Error)`.
  Each variant carries a stable error code (E0xxx) where applicable;
  document the mapping in a doc comment on the enum.
- `src/lib.rs`: re-exports so integration tests can import `cli` and
  `error`.

## Tests (write first)

`tests/cli.rs` -- integration tests using `assert_cmd`:

- `--version` prints `sw-launcher` followed by the cargo version,
  exits 0.
- `--help` mentions every top-level subcommand listed above, exits 0.
- `run` (no args) exits non-zero with a message that mentions
  "scenario" (the missing positional).
- `run nonexistent` exits non-zero with a `NotImplemented`-or-better
  message (config loading isn't here yet, so a "not yet implemented"
  is fine).
- `check`, `build`, `graph`, `cache list`, `vendor status`, `doctor`
  each exit non-zero with the "not yet implemented" sentinel string.

Unit tests in `error.rs` for the Display impl on each variant.

## Pre-commit gate

```bash
cargo test
cargo clippy --all-targets --all-features -- -D warnings
cargo fmt --all
markdown-checker -f "**/*.md"
sw-checklist
```

If `sw-checklist` reports a failure, fix it in this step. Do not
defer.

## Done when

- All tests above are green.
- Zero clippy warnings.
- `cargo fmt --check` is clean.
- `sw-checklist` is green.
- README updated with a one-paragraph "What is this" pointing at
  `docs/`.
- Commit message describes scaffolding scope; cites which design
  doc sections it implements ("CLI surface").
- `agentrail complete --summary "..." --reward 1`.

## Stop after complete

Step 4 (config-types) is a fresh session.
