# Proposal: 8 x 128 KiB partition model with 4 x 32 KiB regions

User-proposed during step 001. Captured here as input to step 002
(revise-schema), checked against the survey, and compared with the
status quo. Not adopted yet -- this doc surfaces fit, friction, and
a recommendation.

## The proposal

Divide the 1 MiB of SRAM (`0x000000..0x0FFFFF`) into eight
**partitions** of 128 KiB each. Each partition is divided into
four **regions** of 32 KiB each:

| region | role          | offset within partition  | size   |
|--------|---------------|--------------------------|--------|
| r0     | code/statics  | partition + 0x00000      | 32 KiB |
| r1     | heap          | partition + 0x08000      | 32 KiB |
| r2     | spare / I/O / data | partition + 0x10000 | 32 KiB |
| r3     | stack         | partition + 0x18000      | 32 KiB |

Absolute layout for all eight partitions:

| P | base       | r0 code            | r1 heap            | r2 spare           | r3 stack           |
|---|------------|--------------------|--------------------|--------------------|--------------------|
| 0 | 0x000000   | 0x000000..0x007FFF | 0x008000..0x00FFFF | 0x010000..0x017FFF | 0x018000..0x01FFFF |
| 1 | 0x020000   | 0x020000..0x027FFF | 0x028000..0x02FFFF | 0x030000..0x037FFF | 0x038000..0x03FFFF |
| 2 | 0x040000   | 0x040000..0x047FFF | 0x048000..0x04FFFF | 0x050000..0x057FFF | 0x058000..0x05FFFF |
| 3 | 0x060000   | 0x060000..0x067FFF | 0x068000..0x06FFFF | 0x070000..0x077FFF | 0x078000..0x07FFFF |
| 4 | 0x080000   | 0x080000..0x087FFF | 0x088000..0x08FFFF | 0x090000..0x097FFF | 0x098000..0x09FFFF |
| 5 | 0x0A0000   | 0x0A0000..0x0A7FFF | 0x0A8000..0x0AFFFF | 0x0B0000..0x0B7FFF | 0x0B8000..0x0BFFFF |
| 6 | 0x0C0000   | 0x0C0000..0x0C7FFF | 0x0C8000..0x0CFFFF | 0x0D0000..0x0D7FFF | 0x0D8000..0x0DFFFF |
| 7 | 0x0E0000   | 0x0E0000..0x0E7FFF | 0x0E8000..0x0EFFFF | 0x0F0000..0x0F7FFF | 0x0F8000..0x0FFFFF |

EBR hardware stack at `0xFEEC00..0xFEF7FF` and MMIO at
`0xFF0000..0xFFFFFF` are unchanged and unrelated.

## Fit against the surveyed corpus

How well do existing repos already align to this grid?

### Strong fit (already at partition boundaries)

- **ocaml** and **tuplet**: ocaml.p24m loads at `0x040000` =
  partition 2 base. The OCaml heap with `heap_limit = 0x03F000`
  ends 4 KiB short of that boundary -- a deliberate guard,
  effectively the top of partition 1. Tuplet's input image at
  `0x080000` = partition 4 base.
- **snobol4**: source at `0x080000` = partition 4 base; optional
  data at `0x090000` = partition 4's r2 region. Almost perfect
  alignment.
- **apl** (batch): `.cor24` text image at `0x080000` = partition
  4 base.
- **macrolisp** snapshot at `0x080000` = partition 4 base.
- **macrolisp** multi-module slots at `0x001000..0x005000` --
  mid-partition-0, NOT aligned. Five modules at 4 KiB stride,
  fitting easily inside partition 0's r0 region.

### Moderate fit (lives in one partition)

- **forth**: code at `0x000000`, dictionary growing up; RSP at
  `0x0F0000` (partition 7 r2 base). Could be re-stated as "uses
  partition 0 for code+dict, partition 7 for RSP." Native COR24
  hardware stack stays in EBR.
- **sws (script)**: shell at `0x000000..0x00F3D2` (partition 0
  r0). Shared protocol regions at `0x0F0000`, `0x0F0400`,
  `0x0FFE00` -- all in partition 7's r2/r3 ranges.
- **yocto-ed**: source at `0x010000` (partition 0 r2),
  `SYE_CMD_ADDR=0x0F0000` (partition 7 r2). Fits.

### Awkward fit (existing layout cuts across partitions)

- **monitor**: program slots at `0x002000`, `0x003000`,
  `0x005000`, `0x020000`, `0x040000`. The first three are inside
  partition 0 r0; sws at `0x020000` = partition 1 base; the
  last at partition 2 base. Per-program slots are smaller than
  32 KiB, so packing several programs into partition 0's r0 is
  fine.
- **basic**, **plsw**, **smalltalk**: these don't load at
  high addresses at all -- everything is in partition 0. The
  partition model is harmless but adds no information for them.

### Bad fit (region sizes too small)

- **macrolisp** BSS: heap_car / heap_cdr / mark cumulatively
  ~288 KiB. A single partition's r1 heap region is only 32 KiB.
  Macrolisp would need to claim r1 across multiple partitions or
  the model needs a "double-wide heap" escape hatch.
- **ocaml** heap: currently uses ~250 KiB (partition 0 r1 + r2 +
  r3 + partition 1 r0 + r1 + r2). Pinning OCaml's heap to a
  *single* partition's 32 KiB r1 would shrink it 8x.
- **plsw** compiler image: ~1 MiB total, exceeds any single
  partition. Plsw needs the whole SRAM contiguously.

## Tension: 32 KiB regions are too small for many heaps

The proposal's 32 KiB region size is right-sized for stacks and
small statics but undersized for heaps in three of the most
demanding repos (macrolisp, ocaml, tuplet). Two ways out:

A. **Partition unification**. A layer can declare
   `partition = 0..N`, `regions = ["r0", "r1", "r2"]`, etc., and
   claim multiple regions across multiple partitions. The
   schema's overlap rule still works -- it just operates on
   "claimed regions" rather than "free addresses."

B. **Partition skipping**. A layer can declare `partition = 4`
   `regions = ["r0", "r1"]` and explicitly omit r2/r3, leaving
   them free for an unrelated layer. Combined with (A), this
   gives full flexibility.

Both keep the partition model as the *default* shape while
admitting layers like macrolisp's heap and plsw's compiler.

## Tension: 128 KiB partitions encourage tighter alignment than
existing repos use

If the schema *defaults* to 128 KiB-aligned partition bases,
existing repos' mid-partition addresses (macrolisp's 0x1000-step
modules, monitor's 0x002000/0x003000/0x005000 slots) become
"deviations" rather than "normal." Two options:

A. **Soft alignment**. Partitions are advisory. The schema lets
   any layer load at any address, but provides a
   `partition_aware = true | false` mode that flags non-aligned
   addresses with a warning.

B. **Hard alignment with sub-partition allocator**. Partitions
   are mandatory at the layer-grain level, but a partition can
   sub-host multiple "slots" (like monitor's program registry).
   Slot alignment within a partition is a per-partition setting.

(A) is less invasive; (B) buys more validation.

## Recommendation for step 002

1. **Adopt the partition grid as the default scenario shape**,
   with these properties:
   - 8 partitions of 128 KiB (`0x00000`..`0x100000` step
     `0x20000`).
   - Each partition has 4 named regions: `code`, `heap`,
     `spare`, `stack` of 32 KiB each.
   - A layer's segments default to specific (partition, region)
     coordinates rather than absolute addresses.

2. **Allow multi-region claims** with explicit syntax:
   ```toml
   [layers.ocaml_interp.segments.value_heap]
   kind = "heap"
   claims = [
     { partition = 0, region = "heap" },
     { partition = 0, region = "spare" },
     { partition = 0, region = "stack" },
     { partition = 1, region = "code" },
     # ... up to the heap_limit at end of partition 1's heap region
   ]
   ```

3. **Allow opt-out** via `[layers.<n>.absolute_addresses = true]`
   for layers like plsw's monolithic compiler that don't fit
   any partition shape.

4. **Validation rules**: in addition to E0001..E0016, add:
   - Eppp: layer's partition+region claim conflicts with another
     layer's claim.
   - Eqqq: layer claims region across partitions but not
     contiguously (e.g. partition 0 r3 + partition 2 r0 with
     partition 1 untouched is suspect).
   - Errr: stack region (`r3`) used as code (`r0` semantics)
     without `claim_role_override`.

5. **Default scenario A re-expressed**: a single layer claims
   `partition = 0, regions = ["code", "heap", "stack"]`. UART
   data lives outside the partition grid (it doesn't occupy
   SRAM).

6. **Tuplet re-expressed**:
   - pvm: `partition = 0, regions = ["code", "heap"]` (code +
     embedded eval/call/heap segments)
   - ocaml heap: claims partition 0 r2 + r3 + partition 1
     r0 + r1 (heap_limit at end of r1 = `0x03F000`)
   - ocaml.p24m: `partition = 2, regions = ["code", "heap",
     "spare"]`
   - input image: `partition = 4, region = "code"` (or
     `kind = "data"` if we keep the existing distinction)

7. **Document the EBR + MMIO exemptions** -- they are not
   partitioned.

## Risk: what this breaks

Repos already in production keep working: their absolute
addresses still parse and validate. The partition model is a
*lens*, not a constraint, when `absolute_addresses = true`.

The cost is mostly cognitive: a fresh reader sees TOML expressed
in partition coordinates and has to learn the grid. That cost
buys two things:

- New scenarios author themselves into a known shape; collisions
  between layers become *partition* collisions, not arbitrary
  hex-address collisions, which is much easier to reason about.
- The `sw-launch graph` output gets a meaningful, repeatable
  visual: the 8x4 grid with each cell colored by which layer
  claims it.

Verdict: worth adopting. Step 002 should land it as the default
with the multi-region escape hatches, plus a short migration
note for repos whose existing addresses don't align (most
already do).
