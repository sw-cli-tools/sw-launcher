# Step 2: sidecar-patch-values

Add `value = "sidecar:<path>"` patch form. Used by sw-cor24-ocaml
to read `code_ptr_addr.txt` / `heap_limit_addr.txt` files written
by its build script.

TDD: tests first.

## Inputs

- Schema gap B1 in `docs/survey/schema-gaps.md`.
- `docs/survey/ocaml.md` section "Build-time-resolved patches".
- `src/manifest.rs::resolve_patch_term` (the Phase 2 step 2
  cross-layer resolver).

## What lands

src/manifest.rs
  - `resolve_patch_term` extends:
      `"sidecar:<path>"` -> read file at `<path>` (relative to
      config_dir if not absolute), parse a single hex literal
      (with or without `0x` prefix), use as the u32 patch value.
      Whitespace and trailing newlines tolerated.
  - The path is stored verbatim in the cache key (Phase 4's
    lockfile will pin the file's sha; Phase 3 just reads fresh).

src/validate.rs
  - New rule E0019 (already documented in design.md): warn when
    a sidecar's mtime is older than the producing artifact's.
    For Phase 3 step 2, since "the producing artifact" requires
    cross-layer awareness validate doesn't have, the rule is
    *implementable* but only as a manifest-time runtime check
    that surfaces a Diagnostic via stderr. Punt the validate-
    time version to Phase 4.
  - Test: a sidecar with a manifestly-stale mtime (touch -d
    "1970-01-01" sidecar.txt) produces a warning when
    sw-launch run reads it.

## Tests

tests/manifest_unit.rs (+3 tests)
  - sidecar_resolves_to_literal_hex_value:
      sidecar contains "1279" (or "0x1279"); value resolves to
      0x1279.
  - missing_sidecar_produces_clear_error:
      patch value = "sidecar:nonexistent.txt"; resolve fails
      with E0006-like error mentioning the path.
  - sidecar_with_non_hex_contents_errors:
      sidecar contains "not a number"; resolve fails with a
      useful "could not parse hex" message including the path.

## Pre-commit gate

Standard.

## Done when

- 3 new manifest unit tests pass.
- A scenario with `value = "sidecar:vendor/.../heap_limit_addr.txt"`
  resolves at LoadPlan::build time on a real ocaml repo, when
  present.
- `agentrail complete --summary "..." --reward 1`.

## Stop after complete

Step 3 (reserved-heap-segments) is a fresh session.
