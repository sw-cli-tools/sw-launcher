# Step 5: scenario-validate

Implement the validators listed as E0001..E0016 in `docs/design.md`,
plus any new codes added by step 2's schema revision. TDD: write the
red test for each rule before the green implementation.

## What lands

- `src/validate.rs`:
  - one function per rule (or grouped by domain when natural)
  - a `Diagnostic { code: ErrorCode, span: Option<Span>, message,
    hint: Option<String> }` type
  - a `validate(&Config, scenario_name: &str) -> Result<(),
    Vec<Diagnostic>>` entry point that runs every rule and returns
    *all* diagnostics (don't stop at the first); the caller decides
    whether to fail fast or report all
- `src/error.rs`: extend with `ValidationFailed(Vec<Diagnostic>)`
- wire `cli::check` to load the config, run `validate`, print
  diagnostics in a stable format, and exit non-zero on any
  diagnostic
- `src/manifest.rs` (new, or stub): `ResolvedSegment { layer:
  String, name: String, range: Range<u32>, kind: SegmentKind }`
  used by overlap checking. Reading symbol offsets from listings
  is step 6 -- for now, embedded segments come from explicit
  fields in the test fixtures, not from listing parsing.

## Rules to cover (cite design.md E-codes)

E0001 scenario has at least one layer
E0002 referenced layer exists
E0003 no two memory ranges overlap (across all layers' segments)
E0004 UART layer has max_bytes and input fits
E0005 layer kind / load method compatibility
E0006 patch target resolves
E0007 layer DAG acyclic
E0008 scenario has halt_on or max_cycles
E0009 referenced tool exists in [tools.*]
E0010 lockfile freshness (skip implementation if no lockfile module
        yet -- record a TODO and stub the rule to always-pass; add to
        Phase 4 saga seed)
E0011 reserved segment lies within sram, never in ebr_stack or mmio
E0012 sum of reserved+loaded bytes <= sram size
E0013 stack/heap segment has nonzero size
E0014 embedded=true segment has a symbol
E0015 embedded=false segment has load.address and size
E0016 self.address/end/size only valid inside a segment block

Plus any new codes from step 2.

## Tests (write first)

For each rule, one test that constructs a minimal Config (in code,
or via a fixture string) that violates exactly that rule and asserts:

- the exact error code
- the offending layer/segment name appears in the diagnostic
- a hint is present when the design doc specifies one

Plus one positive integration test: a complete Scenario A fixture
(from step 4) passes validation cleanly.

## Pre-commit gate

```bash
cargo test
cargo clippy --all-targets --all-features -- -D warnings
cargo fmt --all
markdown-checker -f "**/*.md"
sw-checklist
```

## Done when

- Every E-code in scope has at least one negative test that asserts
  the exact code.
- The positive Scenario A fixture validates cleanly.
- `sw-launch check tests/fixtures/scenario_a.toml run echo`
  (or equivalent) exits 0; introducing a deliberate overlap exits
  non-zero with E0003.
- Zero clippy warnings; sw-checklist green.
- Commit references the design doc rules covered.
- `agentrail complete --summary "..." --reward 1`.

## Stop after complete

Step 6 (assembler-tool) is a fresh session.
