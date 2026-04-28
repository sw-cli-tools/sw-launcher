# Step: schema-v1.2-variable-partitions

User design pivot received after step 003 closed: the fixed 8 x 128
KiB grid in schema v1.1 is too rigid. Schema v1.2 replaces it with
**memory profiles** (named partition shapes), justifies every
> 32 KiB heap claim with an analysis doc, and tightens the rule of
thumb (total < 1 MiB; heaps and stacks each ideally < 0.5 MiB).

This step writes only docs. No Rust. (Step 005-config-types --
formerly 004 -- types the v1.2 schema, not v1.1.)

## Inputs

- `docs/design.md` (schema v1.1 from step 002)
- `docs/survey/` (every per-repo doc, plus index, schema-gaps,
  partition-model-proposal)
- The user's pivot message (summarized below).

## User pivot summary

- **Variable partition count and size**, not a fixed grid.
- **Per-use-case profiles** (APL, PL/SW, REPL, simple-interp,
  compiled-app, etc.) declare partition shape.
- **Rule of thumb**: total <= 1 MiB SRAM; heaps and stacks each
  <= 0.5 MiB ideally; 32 KiB regions are a starting point, not a
  cap.
- **Multiple heaps / stacks of different sizes** allowed when
  justified.
- **>32 KiB heap requires justification**. The user is also adding
  GC to OCaml, which should shrink tuplet's heap need (it was
  doubling-then-failing because of leaked allocations).
- **GC > arena-bump-with-no-op-free** as a default expectation
  for new code.
- **REPL is resource-intensive**; simple-interp and compiled-app
  are not. Encode that as profile families.

## Part 1: heap analysis (`docs/heap-analysis.md`)

For each survey repo with claimed heap or working memory above 32
KiB, write a row:

| repo | claim (KiB) | nature | with GC (est) | floor (KiB) | profile family |

Repos to cover (from survey):
- macrolisp: ~288 KiB BSS (heap_car / heap_cdr / mark)
- ocaml: ~250 KiB OCaml value heap
- tuplet: same heap as ocaml + input image at 0x080000
- plsw: whole image ~1 MiB (compiler is itself a COR24 program)
- snobol4: ~76 KiB internal (SB 64 KiB, SRC 12 KiB, plus stacks)
- forth: dictionary growing up from dict_end (variable)
- monitor / sws / yocto-ed: shared-memory protocol regions
  (4-5 KiB total, fine)

For each, answer:
1. **Why this big?** Cite the source (a Pascal record, a C array,
   a labeled .s reservation).
2. **What fraction is dead allocation?** Best estimate, prose ok.
3. **Would GC reclaim it?** Yes / partial / no, and why.
4. **Floor after GC + reasonable optimizations.** A defended
   number. If "unknown, needs measurement," say so and add to a
   "follow-up" list.
5. **Profile family it belongs to**: `repl-inline-compile` /
   `interpreter-only` / `compiled-app` / `compiler-image` /
   `resident-shell`.

Conclude with a **summary table** of "post-GC budgets we expect
each profile family to fit under" and a **follow-up list** for
repos where the floor needs measurement.

## Part 2: schema v1.2 update (`docs/design.md`)

Replace the v1.1 "Addressing model: partitions and regions"
section with v1.2:

- `[targets.cor24.regions]` keeps `sram`, `ebr_stack`, `mmio`.
- **Drop the fixed `[targets.cor24.partitions]` block.**
- Introduce `[memory_profiles.<name>]`:

  ```toml
  [memory_profiles.compiled-app]
  description = "Single image at 0; small heap; small stack."
  partitions = [
    { name = "code",  base = "0x000000", size = "0x010000",
      regions = [ { name = "code", size = "auto" } ] },
    { name = "heap",  base = "0x010000", size = "0x008000",
      regions = [ { name = "heap", size = "0x008000" } ] },
  ]
  budget = { heap_max = "0x008000", stack_max = "0x000C00" }

  [memory_profiles.repl-inline-compile]
  description = "REPL with in-line compilation; large heap."
  partitions = [
    { name = "code",  base = "0x000000", size = "0x040000", regions = [ ... ] },
    { name = "heap",  base = "0x040000", size = "0x080000", regions = [ ... ] },
    { name = "stack", base = "0x0C0000", size = "0x010000", regions = [ ... ] },
  ]
  budget = { heap_max = "0x080000", stack_max = "0x010000",
             justification_required = false }
  ```

- **Default profile families** to include in the design doc with
  example numbers (refine after Part 1's analysis):
  - `compiled-app` -- single-image-at-0, small heap, small stack
  - `interpreter-only` -- runtime + interpreted source, modest heap
  - `repl-inline-compile` -- runtime + interpreter + compiler in
    image; large heap
  - `compiler-image` -- monolithic compiler-as-COR24-program (plsw)
  - `resident-shell` -- monitor + sws + program slots; many small
    images

- A scenario (or layer) cites a profile:

  ```toml
  [scenarios.apl-batch]
  memory_profile = "interpreter-only"
  ```

- Layers' `claims` reference *that profile's* partitions and
  regions by name, not absolute hex.

- Preserve `absolute_addresses = true` opt-out for layers that
  don't fit any profile (plsw monolith).

- **Multi-heap / multi-stack support**: a profile may declare
  >1 partition with `region.kind = "heap"` and >1 with
  `kind = "stack"`. Layers reference them by `(partition,
  region)` name.

- **Budget enforcement**: `budget.heap_max`, `budget.stack_max`
  are checked against the sum of all heap / stack region claims
  in the scenario. New error code E0028 for over-budget;
  E0029 warns at 80%.

- **Justification field**: a layer claiming a heap region above
  32 KiB must set `heap_justification = "..."` (free text plus a
  `category` from `dead-leak | algorithmic | bytecode-image |
  other`). Validator (E0030) refuses claims > 32 KiB without a
  justification block.

## Part 3: revise validation rules

Append to `docs/design.md` validation list:

- **E0028** scenario heap claims exceed `budget.heap_max`.
- **E0029** scenario heap claims exceed 80% of `budget.heap_max`
  (warning, not error).
- **E0030** heap region > 32 KiB without `heap_justification`.
- **E0031** scenario references a `memory_profile` not declared
  in `[memory_profiles.*]`.
- **E0032** layer claim names a region not present in the
  profile it references.
- **E0033** profile declares overlapping partitions (sanity
  check on the profile itself, distinct from cross-layer
  overlap).
- **E0034** total reserved exceeds 1 MiB rule-of-thumb (warning,
  not error; can be silenced with
  `acknowledge_oversized = true`).

E0001..E0027 numbers stay unchanged.

## Part 4: update plan.md and status.md

- `docs/plan.md`: insert "Phase 0.6 schema v1.2" between Phase 0.5
  (the v1.1 revision) and Phase 1.
- `docs/status.md`: record the v1.2 changelog. Note that the
  step-005 typed `Config` will land against v1.2.

## Part 5: rename of subsequent steps

This insert shifts the following pending steps up by one (slug
preserved). New numbering:
- 005-config-types (was 004)
- 006-scenario-validate (was 005)
- 007-assembler-tool (was 006)
- 008-scenario-a-loadplan (was 007)
- 009-scenario-a-runner (was 008)
- 010-phase1-status (was 009)

`agentrail insert` handles the renumbering automatically. No
manual edits to `.agentrail/`.

## Pre-commit gate

```bash
markdown-checker -f "docs/**/*.md" "*.md"
sw-checklist
cargo test           # nothing changed in code, but run anyway
cargo clippy --all-targets --all-features -- -D warnings
cargo fmt --check
```

## Done when

- `docs/heap-analysis.md` exists with a row per > 32 KiB-heap
  repo, each row complete (no question marks except in the
  follow-up list which is allowed).
- `docs/design.md` has been updated to schema v1.2:
  - `[memory_profiles.*]` block syntax documented with at least
    five named profile families.
  - Layers' `claims` examples updated to reference profiles.
  - `heap_justification` field documented.
  - `absolute_addresses` opt-out preserved.
  - E0028..E0034 appended.
- `docs/plan.md` Phase 0.6 entry added.
- `docs/status.md` v1.2 changelog entry added.
- All gates green.
- Commit message lists every doc changed and cites the user's
  design pivot as motivation.
- `agentrail complete --summary "..." --reward 1`.

## Stop after complete

Step 005-config-types (formerly 004) is a fresh session.
