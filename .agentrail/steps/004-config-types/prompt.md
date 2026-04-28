# Step 4: config-types

Add the typed `Config` model that mirrors the schema in
`docs/design.md`. TDD: write parsing tests first.

## What lands

- `Cargo.toml` adds: `serde = { version = "1", features = ["derive"] }`,
  `toml = "0.8"`, `camino = "1"`.
- `src/config.rs`:
  - `Config` (top-level)
  - `Project { name, default_target, default_scenario }`
  - `Target { kind, word_bits, address_bits, endian, loader, regions }`
    with `Regions { sram, ebr_stack, mmio }` of `MemRange { start, end }`
  - `Scenario { target, layers, entry, run, expect }`
  - `Layer { kind, source, input, tool, artifact, load, exports,
    patches, segments }`
  - `Segment { kind, embedded, symbol?, load?, size, grows? }`
  - `LoadSpec` enum with `Memory { address }` and `Uart { max_bytes,
    terminator }`
  - `Patch { target, value }` with `value` parser that recognizes
    `self.address`, `self.end`, `self.size`, and hex literals
  - `RunCfg { timeout_ms, max_cycles, halt_on }`
  - `Expect { uart_contains, uart_regex, uart_not_contains,
    exit_code, stdout_lines_eq }`
  - top-level `schema_version` required, must equal 1
  - all structs `#[serde(deny_unknown_fields)]` *except* where the
    survey/design docs explicitly say a free-form section is allowed
- helpers: `Config::from_path(&Utf8Path) -> Result<Config>`,
  `Config::from_str(&str) -> Result<Config>`
- hex parsing helper for addresses/sizes (`parse_hex_u32`); accept
  `0x` prefix, underscores, lower or upper case

## Tests (write first)

`src/config.rs` `#[cfg(test)] mod tests` -- unit tests:

- minimal Scenario A TOML parses (use the example from
  `docs/design.md`)
- `schema_version != 1` rejects with a clear error
- unknown top-level key rejects
- unknown key inside `[layers.<x>]` rejects
- hex parsing accepts `0x010000`, `0x01_00_00`, rejects `010000` (no
  prefix), rejects `0x` only
- segment with `embedded = true` requires `symbol`
- segment with `embedded = false` requires `load.address` and `size`
- `value = "self.address"` parses into the appropriate enum variant
- target regions parse with hex bounds; nonsensical (start > end)
  rejects

`tests/config_fixtures.rs` -- integration tests:

- a `tests/fixtures/scenario_a.toml` parses end to end
- a hand-crafted "two-memory-layers + embedded stacks" fixture
  parses end to end (does not need to validate yet)

## Notes

- Validation logic does not live here. Keep `config.rs` strictly
  about deserialization and shape. Cross-field rules go in step 5
  (`validate.rs`).
- Do not parse listing files or resolve symbols here -- that is
  step 6's job.

## Pre-commit gate

```bash
cargo test
cargo clippy --all-targets --all-features -- -D warnings
cargo fmt --all
markdown-checker -f "**/*.md"
sw-checklist
```

## Done when

- Every unit test and integration test above passes.
- Zero clippy warnings.
- `sw-checklist` green.
- Commit references the design doc sections covered.
- `agentrail complete --summary "..." --reward 1`.

## Stop after complete

Step 5 (scenario-validate) is a fresh session.
