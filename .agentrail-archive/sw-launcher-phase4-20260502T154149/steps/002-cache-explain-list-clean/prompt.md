# Step 2: cache-explain-list-clean

Add the user-facing CLI on top of the disk cache that step 1
landed: `sw-launch cache list`, `cache explain
<key-prefix>`, and `cache clean [--older-than|--all|--dry-run]`.

## What lands

### `sw-launch cache list`

- Walks `\${SW_LAUNCH_CACHE_DIR:-...}/sw-launch/artifacts/`,
  reads each entry's `provenance.toml`, prints a table:
  short-digest, layer-when-built (recorded in provenance --
  see step 2.5 below), tool path basename, input sha (8 hex),
  artifact size on disk, last-accessed timestamp.
- `--json` emits a structured array of the same fields.
- `--cache-dir <path>` overrides the default cache root.
- Empty cache exits 0 with no output (or `[]` under
  `--json`).

### `sw-launch cache explain <prefix>`

- Resolves `<prefix>` to the unique entry whose digest starts
  with it; ambiguous prefix is an error listing the matches.
- Prints the entry's full `provenance.toml` plus paths
  to the artifact + listing files.

### `sw-launch cache clean`

- Default: deletes entries whose `created_unix` is older
  than 30 days.
- `--older-than <duration>` overrides the TTL (parsed
  loosely: "7d", "12h").
- `--all` wipes every entry under `artifacts/` and `locks/`.
- `--dry-run` lists what would be removed without acting.

### Provenance enrichment (small follow-up)

The cache currently records tool path / input sha / extra
args / sha sums / timestamp / host. For `cache list` to
print a useful "layer" column, the `Tool::build` callsite
should pass an optional `layer_name` into the cache
key/provenance metadata (not into the digest -- different
layer names with the same input bytes should still hit the
same cache entry; just record the most recent layer name
that filled it).

## Tests

- `cache list` against a pre-seeded cache directory prints
  the expected rows; empty cache prints nothing.
- `cache explain <prefix>` happy path + ambiguous-prefix
  error.
- `cache clean --dry-run` is read-only (cache contents
  unchanged after).
- `cache clean --older-than 1s` removes a synthetic entry
  whose `created_unix` is in the past; --all wipes
  everything.
- `cache clean --dry-run --all` lists every entry.

## Pre-commit gate

Standard. The 9 carry-forward sw-checklist failures should
not regress; new sub-crate-local helpers stay under the
per-crate caps.

## Done when

- The three subcommands work end to end against a tempdir
  cache.
- `cache list` shows entries from any prior Phase 1-3
  fixture run when run with the matching `SW_LAUNCH_CACHE_DIR`.
- All cache-subcommand tests pass.

## Citations

- docs/design.md "CLI surface" (line 1477)
- docs/saga-phase4-plan.md step 2

## Scope guardrails

- Do NOT add lockfile logic (step 3).
- Do NOT add `cache prune-by-vendor` (Phase 5).
- The `last-accessed` timestamp can be ctime-based; we don't
  need to update it on every cache hit yet.