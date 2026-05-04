# Step 1: sub-crate-extract-validate

The first of the deferred sub-crate splits. Mirrors the
Phase 4 step 1 pattern (`sw-launcher-tool` extraction) and
the Phase 4 step 3 pattern (`sw-launcher-lockfile`).

After this step the main crate's `validate.rs` is gone;
its full surface is re-exported through `sw_launcher::validate`
so existing callers and tests don't need to update import
paths.

## What lands

### Sub-crate

New `crates/sw-launcher-validate/` with:

```
crates/sw-launcher-validate/
  Cargo.toml
  src/lib.rs              # pub mod validate; pub use validate::*;
  src/validate.rs         # moved from src/validate.rs (git mv)
  tests/validate_unit.rs  # moved from tests/validate_unit.rs
```

Cargo.toml deps: anyhow, camino, serde, sw-launcher-config (or
inline a minimal Config-shaped trait if extracting Config too is
out of scope -- see "Coupling" below).

### Coupling

The current `src/validate.rs` imports `crate::config::*`. To
extract `validate` cleanly without also extracting `config`,
options:

- **Option A (preferred, simpler)**: extract `config` *with*
  validate. The two are tightly coupled: validate is config's
  primary consumer. One sub-crate `sw-launcher-config-validate`
  with both modules.
- **Option B**: leave `config` in main crate; have validate
  import it via `sw_launcher_validate::ConfigShape` trait that
  the main crate impls. More layering, more cost, less to
  buy.

Pick A. The sub-crate name can stay `sw-launcher-validate`
even though it bundles `config`, since validate is the
visible-from-outside surface.

Actually -- on second thought, `config.rs` is consumed by
`manifest`, `cli`, `validate`, and `doctor`. Splitting it off
into validate's sub-crate would force every consumer to
depend on `sw-launcher-validate`. That's ugly.

Better path: `config` goes in *its own* tiny sub-crate first
(or simultaneously). So:

- `crates/sw-launcher-config/` -- bare Config + types
- `crates/sw-launcher-validate/` -- depends on
  sw-launcher-config

If splitting both feels like too much for one step, do
config first and defer validate to step 1b. Or do config +
validate together as one beefier step.

**Recommended**: do `config` + `validate` extraction in this
one step. The two are siblings; splitting them across two
saga steps just delays the same workspace plumbing twice.

### What moves

- `src/config.rs` -> `crates/sw-launcher-config/src/lib.rs`
- `src/validate.rs` -> `crates/sw-launcher-validate/src/lib.rs`
  - `sw-launcher-validate` Cargo.toml: `sw-launcher-config = { path = "../sw-launcher-config" }`
- `tests/config_unit.rs` and `tests/config_fixtures.rs` move
  to `crates/sw-launcher-config/tests/`.
- `tests/validate_unit.rs` moves to
  `crates/sw-launcher-validate/tests/`.

The main crate's `src/lib.rs` re-exports both:

```rust
pub use sw_launcher_config as config;
pub use sw_launcher_validate as validate;
```

Existing call sites that say `crate::config::*` /
`crate::validate::*` keep working through these re-exports.

### Anyhow boundary

Like `sw-launcher-tool` and `sw-launcher-lockfile`, the new
sub-crates use `anyhow::Result` internally. The main crate's
`From<anyhow::Error> for Error` (already in `src/error.rs`)
handles the boundary at every callsite.

`Diagnostic` and `Severity` keep their concrete types --
they're already in `validate.rs` and don't need to leak
across the boundary as anyhow errors.

### Tests

- All existing `validate_unit.rs` and `config_*` tests
  continue to pass with no behaviour change. The path
  `sw_launcher::validate::*` resolves through the re-export.
- 1 new doc-comment test in
  `crates/sw-launcher-validate/src/lib.rs` confirming the
  top-level `validate(cfg, scenario)` is reachable.

## Pre-commit gate

Standard. The 14 carry-forward sw-checklist failures should
*drop*. Expected delta:

- `validate.rs` File LOC 817 -> moves to a sub-crate that
  may or may not still flag (it's still 817 lines). But
  `[sw-launcher]` no longer reports it.
- `validate.rs` Module Function Count 23 -> reports under
  the sub-crate.
- `Crate Module Count [sw-launcher]: 8` -> drops to 6 (lose
  `validate`, lose `config`).

Net: probably -3 fails on the main crate, +2 fails on the
new sub-crate. Net -1 to -2.

If the sub-crate's module-fn count is still over-cap, that's
acceptable; the *purpose* of the split was per-crate
budgets, and the per-sub-crate over-cap is a legitimate
"this validation surface is genuinely 23 fns" trade-off, not
a structural bug.

## Done when

- 117 tests still pass (or higher, with doc test).
- sw-checklist count drops from 14 to 12 or fewer.
- Main crate's `Crate Module Count` failure resolves.
- Document the count delta in the step commit.

## Citations

- docs/saga-phase5-plan.md step 1.
- Phase 4 step 1 commit (8f04b46) -- the
  sw-launcher-tool extraction is the reference pattern.
- Phase 4 step 3 commit -- the sw-launcher-lockfile
  extraction.

## Scope guardrails

- Do NOT change validate's behaviour. This is pure code
  motion + workspace plumbing.
- Do NOT also extract `manifest` -- that's step 2.
- Do NOT also extract `error` -- the main crate keeps its
  Error type and the From<anyhow::Error> impl.
- Do NOT add new error codes. Step 4 (rca1802) introduces
  E0050 / E0051; this step is mechanical.