# Step 6: assembler-tool

Wire up the assembler invocation and the listing parser. No disk
cache yet -- that is Phase 4. This step gives `sw-launch` the
ability to turn a `.s` source into a `.bin` artifact and to extract
absolute addresses for declared `exports.symbols`.

## What lands

- `src/tool.rs`:
  - `trait Tool` with `version() -> Result<String>` and
    `build(&BuildJob) -> Result<BuildOutput>`
  - `Assembler` impl that calls
    `cor24-run --assemble <input> <out.bin> <out.lst>` via
    `std::process::Command`. Tool path comes from the Config's
    `[tools.assembler]` entry; default is to find `cor24-run` on
    PATH if no entry given.
  - `BuildJob { layer_name, input: Utf8PathBuf, output_dir:
    Utf8PathBuf, args: Vec<String> }`
  - `BuildOutput { artifact: Utf8PathBuf, listing:
    Option<Utf8PathBuf>, stdout: String, stderr: String }`
  - in-process memoization keyed on
    `(tool_path_canonical, input_path_canonical,
    input_sha256, args_sorted)` -- a `HashMap` on the
    `Assembler` value; second invocation in the same `sw-launch`
    process is a hit and does not spawn the tool
- `src/listing.rs`:
  - `Listing::parse(text: &str) -> Listing`
  - `Listing::resolve(&self, symbol: &str) -> Option<u32>` that
    finds `<symbol>:` and returns the absolute address printed by
    `cor24-run --assemble` in the listing
  - handle the formats actually emitted by `cor24-run` (look at
    `~/github/sw-embed/sw-cor24-pcode/vm/pvm.s` build output and
    real `.lst` files in sibling repos to confirm)

## Tests (write first)

Unit tests:

- `Listing::parse` + `resolve` against a fixture `.lst` excerpt
  that includes `code_ptr:`, `eval_stack:`, and a labeled
  reservation block; assert each address is recovered correctly
- `Assembler` memoization: a `RecordingTool` wrapper around
  `Assembler` that counts spawn attempts; calling `build` twice
  with the same `BuildJob` triggers exactly one spawn

Integration tests (`tests/assembler.rs`):

- gated on `cor24-run` being on PATH (skip with a clear `eprintln!`
  if absent; do not fail)
- assemble a tiny `.s` fixture in `tests/fixtures/echo.s` (a hello-
  world or echo small enough to fit at 0x0); produces `.bin` of the
  expected size; listing has the expected `_start:` symbol
- assemble two different sources in the same process: two spawns,
  not one

## Pre-commit gate

```bash
cargo test
cargo clippy --all-targets --all-features -- -D warnings
cargo fmt --all
markdown-checker -f "**/*.md"
sw-checklist
```

## Done when

- Memoization works; tests cover hit/miss.
- Listing parser handles the real-world format (tested against an
  excerpt from a real `.lst`).
- Integration tests against `cor24-run` pass when present, skip
  cleanly when absent.
- `sw-launch build <scenario>` (Scenario A) produces the artifact
  on disk in a known temp location and prints its path.
- `agentrail complete --summary "..." --reward 1`.

## Stop after complete

Step 7 (scenario-a-loadplan) is a fresh session.
