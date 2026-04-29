# sw-launcher Phase 4: caching, vendor lockfile, doctor, graph

This is the saga seed for `sw-launcher-phase4`. Open with:

```bash
agentrail init --name sw-launcher-phase4 --plan docs/saga-phase4-plan.md
```

## Goal

After Phase 3 the runtime engine works end to end for Scenarios
A, B, and C. What's missing is everything that turns a working
prototype into something agents and humans can rely on across
sessions and machines:

1. **Reproducible builds across machines / sessions.** The
   in-process `HashMap` cache from Phases 1-3 dies with the
   process. A second `sw-launch run` re-assembles every layer.
   A *content-addressed* cache under `~/.cache/sw-launch/`
   keyed on (tool path, input sha256, extra args) survives
   between runs and between projects.
2. **Pinned sibling-repo provenance.** Every Phase 1-3 test
   resolves `sw-cor24-pcode/vm/pvm.s` and
   `sw-cor24-ocaml/build/ocaml.p24m` by walking
   `~/github/sw-embed/...` directly. That's fine for the dev
   machine; it isn't reproducible. `sw-launch.lock` records
   the SHAs of every vendored input at `vendor sync` time and
   refuses to run if the working tree drifts.
3. **Self-diagnosis.** `sw-launch doctor` runs all the gating
   checks each integration test currently does by hand
   (cor24-run on PATH, pa24r/p24-load resolvable, vendored
   sources reachable, tool versions reported) and prints a
   single actionable report.
4. **Layer-graph introspection.** `sw-launch graph <scenario>`
   prints the layer DAG (text + `--json`) so users and agents
   can see what a scenario actually depends on without
   running it.

These four pieces don't add new scenario shapes; they harden
what Phases 1-3 already produce.

## Why this phase now

- The disk cache lifts an obvious wart: Phase 3's
  `scenario_c` end-to-end test re-assembles `pvm.bin` (~12k
  source lines through `cor24-run --assemble`) every run,
  which is fine for one CI lap but painful for an agent
  iterating on a manifest.
- The vendor lockfile is the natural pair to the disk cache:
  the cache key already includes input hashes, so the lock
  format is "write the same hashes the cache already
  computed, plus the resolved vendor path/SHA, into a
  human-editable TOML".
- `doctor` is the smallest pre-flight surface that lets a
  user run *one command* to know whether their environment
  can run the existing Phase 1-3 fixtures. Worth it before
  Phase 5 multiplies tool count.
- `graph` falls out of the existing `LoadPlan` for free; the
  expensive part (resolving layers + segments) is already
  written. It's mostly a presentation step.

After Phase 4, Phase 5 stubs additional emulator backends
(IBM 1130, RCA 1802, RISC-V32) and starts the sub-crate
extraction (`sw-launcher-validate`, `sw-launcher-tool`,
`sw-launcher-manifest`) that Phases 1-3 deferred.

## Working agreement

Same as Phases 1-3:

- TDD red/green/refactor for every step.
- Standard pre-commit gate green at every step's commit. The
  9 inherited sw-checklist trade-offs from Phase 3 carry
  over; new failures must be either fixed in the same step or
  documented as the same root constraint conflict.
- Commit before `agentrail complete`. Push after `complete`
  (then again after the finalize commit).
- Module count is at 8, max 7. Phase 4 likely needs new
  modules (`cache`, `lockfile`, `doctor`, `graph`). The
  *right* move is to begin the Phase 5 sub-crate extraction
  in Step 1 -- pull `tool` and the in-process cache into a
  `sw-launcher-tool` sub-crate, then add `cache` next to it.
  This stays under each crate's per-file caps and makes the
  disk-cache scope clean. Document the decision in the step
  commit.

## Required ground truth (probe before step 1)

Before starting step 1, confirm two things:

```bash
# 1. The XDG cache directory convention on Darwin and Linux:
echo "${XDG_CACHE_HOME:-$HOME/.cache}/sw-launch"

# 2. Phase 3's in-process cache works. Run scenario_b twice;
#    the second run should not re-invoke pa24r:
cargo test --test scenario_b -- --nocapture 2>&1 | grep -i cache
```

Phase 4 step 1 inherits the cache key shape Phase 3 already
uses (`(canonical tool path, sha256(input), extra_args)`); it
just spills it to disk.

## Step list

### Step 1 -- disk-cache-layout

**What lands**

- New module / sub-crate `sw-launcher-cache` (preferred:
  start the sub-crate extraction now). Falls back to a
  `src/cache.rs` module if the sub-crate split is too
  invasive in a single step -- but document the choice.
- On-disk layout under
  `${XDG_CACHE_HOME:-$HOME/.cache}/sw-launch/`:
  ```
  artifacts/<sha256-of-cache-key>/   # the artifact bytes
    <stem>.<ext>
    provenance.toml      # tool path/version, input sha,
                         # extra_args, timestamp, host
  index.toml             # cache-key -> dir, last-accessed
  ```
- API: `Cache::get_or_fill<F>(key, fill: F) -> Result<PathBuf>`
  where `fill` is the existing in-process build closure.
- Phase 1-3's in-process `HashMap` becomes a thin layer on
  top of the disk cache: hot path stays a HashMap lookup,
  miss falls through to disk, miss-and-disk-miss runs
  `fill`.
- Cache *write* is atomic: write to temp dir under
  `artifacts/`, fsync, rename. `provenance.toml` is written
  alongside.
- Tests:
  1. cache hit returns the same path on second call without
     re-running `fill`.
  2. cache miss runs `fill` exactly once and persists.
  3. corrupt entry (truncated artifact) is detected on next
     read by re-hashing and re-filling.
  4. Concurrent `get_or_fill` calls for the same key are
     serialized via `flock` on the entry directory; only one
     `fill` runs.

**Done when**

- All Phase 1-3 tests still pass with the disk cache active.
- A second `cargo test --test scenario_c -- --nocapture` does
  not re-run `cor24-run --assemble` or `pa24r` or `p24-load`
  for unchanged inputs.

**Citations**: `docs/design.md` "Cache key" (lines 1172-1195).

### Step 2 -- cache-explain-list-clean

**What lands**

- `sw-launch cache list` -- table of cached entries: short
  cache key, layer-name-when-built, tool, input sha,
  artifact size, last-accessed.
- `sw-launch cache explain <cache-key-prefix>` -- pretty-
  prints the entry's `provenance.toml` (the full tool path,
  argv, input sha, output sha, timestamp).
- `sw-launch cache clean` -- deletes entries past a TTL
  (default 30 days) and orphans (entries in `index.toml`
  whose `artifacts/` dir vanished, and vice versa).
  - `--all` wipes everything.
  - `--older-than <duration>` overrides the TTL.
  - `--dry-run` lists what would be removed.
- Tests: each subcommand has a happy-path integration test
  that pre-seeds a fake cache directory, runs the command,
  and asserts on the output and on the post-state.

**Done when**

- `sw-launch cache list` shows entries from any prior run.
- `cache clean --dry-run` is non-destructive.

**Citations**: `docs/design.md` "CLI surface" (line 1477) +
"Cache key".

### Step 3 -- vendor-lockfile

**What lands**

- New module / sub-crate `sw-launcher-lockfile` (or
  `src/lockfile.rs`).
- `sw-launch vendor sync` walks every layer's source, resolves
  vendor / sibling paths, computes input shas, and writes
  `sw-launch.lock` next to `sw-launch.toml`:
  ```toml
  schema_version = 1
  [vendored.<layer>]
  resolved_path = "/abs/path/at/sync/time"
  vendor_repo   = "sw-embed/sw-cor24-pcode"   # if known
  vendor_sha    = "<git-sha-at-sync-time>"    # if .git
  input_sha256  = "..."
  synced_at     = "2026-04-29T17:00:00Z"
  ```
- `sw-launch vendor status` prints which lockfile entries are
  fresh, drifted, missing, or unverified.
- `sw-launch run` and `sw-launch build` refuse to proceed if
  the lockfile is missing OR if any input's sha doesn't match
  the lockfile entry; `--update-lock` regenerates.
- New error codes:
  - E0040 `lockfile-missing`
  - E0041 `lockfile-stale` (input sha drifted)
  - E0042 `lockfile-unresolvable` (vendor path doesn't
    exist any more)
- Tests:
  1. `vendor sync` produces a deterministic lockfile (sorted
     keys, fixed timestamp via injectable clock).
  2. modifying a vendored file fires E0041 on next `run`.
  3. `--update-lock` rewrites only drifted entries; fresh
     entries keep their `synced_at`.

**Done when**

- Phase 1-3 fixtures continue to pass (their tests run
  `vendor sync` once during setup).
- A drifted input is rejected with a clear E0041 message.

**Citations**: `docs/design.md` "Lockfile format"
(lines 1196-1226).

### Step 4 -- doctor

**What lands**

- New `src/doctor.rs` (or `sw-launcher-doctor` sub-crate).
- `sw-launch doctor` runs:
  - `cor24-run --version` (PASS / FAIL with version string)
  - `pa24r` resolvable + version
  - `p24-load` resolvable + version
  - `~/.cache/sw-launch` writable
  - For each `sw-launch.toml` in CWD: lockfile present,
    every vendored path resolves, every input sha matches
- Output: human-readable table by default, `--json` for
  agents, exit code 0 iff all checks pass.
- Tests:
  1. doctor exit 0 on a fully-working dev machine (gated on
     real tools the way scenario_b/c are).
  2. doctor exit 1 with E0040 if a lockfile is missing.
  3. doctor exit 1 with a clear "tool not on PATH" line when
     `pa24r` is hidden by a temp PATH override.

**Done when**

- `sw-launch doctor` is the single command an agent runs
  before any Phase 1-3 fixture; if it exits 0, the fixtures
  pass.

**Citations**: `docs/design.md` "CLI surface" (line 1477).

### Step 5 -- graph

**What lands**

- New `src/graph.rs` (or `sw-launcher-graph` sub-crate).
- `sw-launch graph <scenario>` prints the layer DAG:
  ```
  pcode_vm   (assembler:vendored:sw-cor24-pcode/vm/pvm.s)
    -> ocaml_interp   (binary:vendored:sw-cor24-ocaml/build/ocaml.p24m)
      -> ocaml_source (text:./demo.ml, uart EOT)
  ```
- `--json` emits a structured DAG (nodes + edges + per-layer
  cache key) suitable for piping into other tools.
- The DAG is derived from Phase 1-3's existing `LoadPlan`;
  step 5 is mostly presentation logic.
- Tests:
  1. text rendering of scenario_a, scenario_b, scenario_c is
     stable (snapshot test).
  2. `--json` round-trips through a known-good schema.
  3. cycle detection: a contrived cyclic patch reference
     fires E0006 (already a Phase 2 rule) before graph
     rendering attempts to print.

**Done when**

- `sw-launch graph nested-demo` produces the picture that
  matches `docs/survey/ocaml.md`'s nested-interpreter
  diagram.

**Citations**: `docs/design.md` "CLI surface" (line 1477).

### Step 6 -- phase4-status

Standard close-out: `docs/status.md` updates, draft Phase 5
saga seed (sub-crate extraction + multi-target stubs +
heap-budget enforcement against memory profiles),
`README.md` status section, `agentrail complete --done`.

## Survey citations

- `docs/design.md` "Cache key" (lines 1172-1195) -- the cache
  key shape; Phase 4 step 1 promotes it from in-process to
  on-disk.
- `docs/design.md` "Lockfile format" (lines 1196-1226) -- the
  TOML shape Phase 4 step 3 implements.
- `docs/design.md` "CLI surface" (line 1477) -- defines the
  `cache list|explain|clean`, `vendor sync|status`, `doctor`,
  and `graph` subcommands; Phase 4 ships all five.
- `docs/architecture.md` "Cache and vendor model" -- design
  background.

## Risks and mitigations

- **Disk-cache poisoning by manual edits.** Mitigation: every
  read re-hashes the cached artifact and re-fills on
  mismatch. The `provenance.toml` is metadata only; integrity
  is the artifact sha.
- **Concurrent runs race on the same cache entry.** Mitigation:
  `flock` on the entry directory; only one `fill` runs per
  key per host.
- **Lockfile churn.** If `vendor sync` rewrites unchanged
  entries, every CI run produces a noisy diff. Mitigation:
  read-modify-write only for drifted entries (`--update-lock`
  semantics).
- **Doctor false negatives on partial environments.** If the
  user only runs Scenario A, they don't need pa24r/p24-load.
  Mitigation: `doctor` reports missing tools as warnings if
  no `sw-launch.toml` in CWD references them; only escalates
  to errors if a referenced tool is missing.
- **Sub-crate extraction in step 1 balloons scope.** If it
  turns out to be more than one step's worth of work, fall
  back to a single-crate `src/cache.rs` and defer the split
  to Phase 5. Document either way in the step commit.

## Function/module-count budget

Phase 4 likely adds 4 new top-level modules / sub-crates
(`cache`, `lockfile`, `doctor`, `graph`). The crate module
count is already at 8 / cap 7; adding to it without
extracting sub-crates pushes it further over.

The recommended path:

- Step 1 extracts `tool` + `cache` into a `sw-launcher-tool`
  sub-crate. That removes 2 modules from the main crate while
  adding 0, netting -2.
- Steps 3 / 4 / 5 each add their concern as a sub-crate
  (`sw-launcher-lockfile`, `sw-launcher-doctor`,
  `sw-launcher-graph`), so the main crate stays under cap.
- Phase 5 finishes the split (`sw-launcher-validate`,
  `sw-launcher-manifest`).

If a step cannot afford the sub-crate extraction in its scope,
add the module to the main crate and document the failure as
the same root constraint as Phases 1-3.
