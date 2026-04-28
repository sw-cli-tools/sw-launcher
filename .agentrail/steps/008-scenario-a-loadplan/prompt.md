# Step 7: scenario-a-loadplan

Translate a validated Scenario A into a deterministic load plan and
into the `cor24-run` argv. TDD: golden snapshot first, code second.

## What lands

- `src/manifest.rs`:
  - `LoadPlan { memory_loads: Vec<MemoryLoad>, uart: Vec<UartChunk>,
    patches: Vec<ResolvedPatch>, entry: u32, segments: Vec<ResolvedSegment> }`
  - `MemoryLoad { layer: String, path: Utf8PathBuf, address: u32 }`
  - `UartChunk { layer: String, bytes: Vec<u8>, terminator:
    Option<u8> }`
  - `ResolvedPatch { address: u32, value: u32 }`
  - `LoadPlan::build(&Config, scenario: &str, &Artifacts) ->
    Result<LoadPlan>` where `Artifacts` is a map from layer name to
    its built `.bin` path and parsed listing
- `src/target/cor24.rs`:
  - `fn build_argv(&LoadPlan) -> Vec<String>` that emits the
    `cor24-run` command line:
    - `--load-binary <path>@<hex_addr>` per memory load (sorted by
      address ascending for determinism)
    - `--patch <addr>=<value>` per patch (sorted by address)
    - `--entry <hex>`
    - `--uart-input <bytes>` if any UART chunks (UART chunks are
      concatenated in the order declared in the scenario; embed
      terminators as bytes)
    - `--speed 0` and `-n <max-insn>` from RunCfg
    - `--terminal` only if profile says so

## Tests (write first)

Unit tests:

- segment accounting: a layer with `embedded` segments contributes
  ranges to `LoadPlan.segments` but no `MemoryLoad`; non-embedded
  reserved segments contribute neither a `MemoryLoad` nor a
  `UartChunk` (they're just zero-fill metadata) but do contribute
  ranges
- patches: `value = "self.address"` resolves to the segment's
  address; `value = "0x010000"` resolves literally
- ordering: two memory loads come out address-ascending regardless
  of TOML order

Snapshot tests (use a small in-tree snapshot helper; do *not* add an
external snapshot crate -- write expected text inline or under
`tests/golden/` and compare strings):

- Scenario A without embedded segments: argv matches the golden
  string byte-for-byte
- Scenario A with embedded eval_stack/call_stack/heap declared:
  argv unchanged (no extra `--load-binary`); `LoadPlan.segments`
  contains the three embedded ranges

## Pre-commit gate

```bash
cargo test
cargo clippy --all-targets --all-features -- -D warnings
cargo fmt --all
markdown-checker -f "**/*.md"
sw-checklist
```

## Done when

- `cargo test` green; snapshot strings stable on rerun.
- `sw-launch build <scenario>` followed by `--explain` prints a
  human-readable form of the LoadPlan; same content under
  `--report-json` is structured.
- Zero clippy warnings; sw-checklist green.
- Commit cites design.md "Memory layout" and "CLI surface".
- `agentrail complete --summary "..." --reward 1`.

## Stop after complete

Step 8 (scenario-a-runner) is a fresh session.
