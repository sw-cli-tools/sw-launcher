# Heap analysis: where the bytes go and what could shrink them

Companion to `docs/memory-stance.md`. For every repo in
`docs/survey/` whose claimed heap (or whose total working memory
that behaves like a heap) exceeds 32 KiB, this doc:

1. Describes what the bytes are.
2. Categorizes the claim per the v1.2 `heap_justification.category`
   enum (`algorithmic-floor` / `bytecode-image` / `gc-slack` /
   `dead-leak` / `algorithmic-bloat`).
3. Cites a *historical* implementation of the same language as the
   upper benchmark we should not need to exceed.
4. Names the shrinkage work that would bring the claim down.

The default prior, per `memory-stance.md`, is **dead-leak until
proven otherwise**. That is reflected in the categorizations
below; readers should challenge them, not accept them.

## Summary table

| repo       | current claim   | category              | historical floor    | post-fix target | priority |
|------------|-----------------|-----------------------|---------------------|-----------------|----------|
| ocaml      | ~252 KiB heap   | dead-leak             | OCaml-on-PDP-10 < 256 KiB *total* | <= 64 KiB after GC | high (in flight) |
| tuplet     | inherits ocaml  | dead-leak             | n/a (downstream)    | shrinks with ocaml | follows ocaml |
| macrolisp  | ~288 KiB BSS    | dead-leak + algorithmic-bloat | Maclisp 256 KiB *total* | <= 64 KiB heap | medium |
| plsw       | ~1 MiB image    | algorithmic-bloat     | Pascal compilers in 64 KiB | <= 256 KiB image | medium |
| snobol4    | ~76 KiB internal| algorithmic-floor + dead-leak | SNOBOL4 in 64-256 KiB *total* | <= 64 KiB | low |
| forth      | dictionary-grows-up | algorithmic-floor (workload-dependent) | Forth kernels in 4-16 KiB | <= 16 KiB typical | n/a |
| basic      | embedded DIM 64-128 KiB | algorithmic-floor | Altair 4K BASIC | <= 32 KiB | low |
| smalltalk  | DIM inside basic | inherited | Smalltalk-72 in 128 KiB | follows basic | n/a |
| sws/yocto-ed | shared regions ~5 KiB | algorithmic-floor | n/a | already small | n/a |
| monitor    | program slots, ~8 KiB code | algorithmic-floor | DOS in 8 KiB | already small | n/a |

## Per-repo analysis

### ocaml -- ~252 KiB heap; primary tuplet driver

**What the bytes are.** OCaml interpreter on Pascal-compiled
p-code. Heap allocations are AST nodes (`Expr`, `Pat`, `Val`),
environments (`EnvEntry`), and value records, all `new()`'d in
`src/ocaml.pas`. There are ~50 allocation sites
(`docs/heap-survey.md` in the OCaml repo enumerates them). Each
record is 6-9 24-bit words.

**Why this big right now.** The interpreter has no GC. Every
`new()` is a permanent allocation; only `dispose` would free,
and the runtime treats `dispose` as no-op. A REPL session that
evaluates `1 + 2; 3 + 4; 5 + 6;` allocates and never frees the
intermediate Val/Expr trees from each statement. Long sessions
or deep recursion (parser, evaluator, list ops) burn through the
heap until `heap_limit` is hit and the runtime traps.

**Category: dead-leak.** Not algorithmic floor. The data fits
under 64 KiB in steady state; the missing GC is what's pushing
the limit to ~252 KiB.

**Historical benchmark.** The original ML implementation (LCF/ML
on PDP-10) ran in 256 KiB *total core* including the runtime,
the OS-equivalent, and the user's program. That was 1973. Our
COR24 OCaml has the entire 1 MiB to itself.

**Shrinkage backlog (in flight).**
1. The `sw-cor24-ocaml` saga `ocaml-heap-reclaim` is implementing
   a typed-allocator + mark/sweep GC. Step 002 of that saga
   surveyed every `new()` site (`docs/heap-survey.md` in the
   OCaml repo) and produced a field map for the marker. Once
   landed, expected post-GC steady-state heap: < 64 KiB for
   typical demos.
2. Compact records: the `Val` record carries unused fields for
   non-list values; tagged-union packing could halve record
   size.
3. Intern integer literals and small strings.

**Post-fix target: 64 KiB heap.** The launcher schema must
support the current 252 KiB transitionally (via the
`gc-slack` category once GC lands, or `dead-leak` while it
hasn't), and should not normalize the current size.

### tuplet -- ~252 KiB inherited from ocaml + 1 binary image

**What the bytes are.** Tuplet runs OCaml programs; the heap is
the OCaml heap (above). Plus an opaque binary image at
`0x080000` consumed by the DSL via memory reads. The image is
*data*, not heap allocation -- it's `bytecode-image` regardless.

**Category for the heap: dead-leak (downstream of ocaml).** The
failure mode the user reported -- "kept redoubling in a failed
attempt to not run out of memory that was allocated but never
freed" -- is exactly the OCaml heap thrashing the limit. After
ocaml's GC lands, tuplet's `heap_limit` should *shrink*, not
stay at `0x03F000`.

**Category for the image: bytecode-image.** Legitimate; sized to
the demo data.

**Historical benchmark.** No direct ancestor; tuplet is novel.
But the *pattern* (DSL on top of an interpreted language) was
done in 1980s Lisp machines and Smalltalk in <512 KiB total.

**Shrinkage backlog.** Follow ocaml's GC work. After it lands,
re-measure tuplet's required heap and compress the limit.

**Post-fix target: same as ocaml**, < 64 KiB heap; the binary
image stays its own size.

### macrolisp -- ~288 KiB BSS

**What the bytes are.** Three parallel arrays:
- `heap_car` -- car pointers, ~96 KiB.
- `heap_cdr` -- cdr pointers, ~96 KiB.
- `mark` -- GC mark bits, ~96 KiB (one byte per cell).

Plus smaller arrays for symbols and strings. The cell count is
fixed at compile time at ~32K cells.

**Why this big right now.** Two issues.

1. The cell count is sized for "comfortable headroom for any
   demo," not for the actual demos. The largest current
   macrolisp demo uses ~2K cells. Sizing for 32K means 16x
   over-allocation.
2. 1-byte mark bits per cell when 1-bit would suffice. That
   alone is 8x bloat on the mark array.

**Category: algorithmic-bloat.** A small dose of dead-leak too --
the mark array is *touched* during GC but the floor allocation
shape is the bloat.

**Historical benchmark.** Maclisp on PDP-10 ran with 256 KiB of
*total core* including the OS. Our cell count alone is over twice
that. Original Lisp interpreters fit in 4-8 KiB on early
machines.

**Shrinkage backlog.**
1. Right-size the cell count per demo (or per profile).
   Compiled-app profile: 4K cells (~36 KiB total). REPL
   profile: 8K cells (~72 KiB total).
2. Pack mark bits 8-to-a-byte. Saves 84 KiB instantly.
3. Combine `heap_car` and `heap_cdr` into one
   array-of-structs to improve locality and cut header overhead
   if any.

**Post-fix target: <= 64 KiB heap for typical demos**, < 128 KiB
for the snapshot/REPL case. The current 288 KiB is unjustified.

### plsw -- ~1 MiB compiler image

**What the bytes are.** The PL/SW compiler is itself a COR24
program. `build/plsw.s` is ~356K lines of generated assembly
(Pascal-style transpiler output). The compiled binary
approaches the full 1 MiB SRAM.

**Why this big right now.** Per the survey:
> the PL/SW compiler is itself a COR24 program (build/plsw.s,
> ~356K lines), so compiling user .plsw runs the emulator twice
> -- once with the compiler image and source-over-UART, then
> again with the captured assembly.

The compiler is a *complete* PL/SW implementation in
machine-translated form. Most of the bytes are runtime support,
register save/restore, and large dispatch tables.

**Category: algorithmic-bloat.** A real Pascal compiler
historically fit in 64 KiB of code (UCSD Pascal, Turbo Pascal
1.0). 1 MiB is 16x.

**Historical benchmark.** UCSD Pascal compiler on a 64 KiB
Apple II. Turbo Pascal 1.0 in 33.5 KiB total. CDC 6600 Pascal in
< 60 KiB. *Every* Pascal compiler that worked at all in the
1970s-80s fit comfortably in < 256 KiB.

**Shrinkage backlog.**
1. The transpiler that produces `plsw.s` is the bloat source.
   Inspect the generated code for redundant register
   save/restore, inlined runtime helpers that should be calls,
   and dispatch tables that could be smaller.
2. A bytecode form (compile to p-code instead of native COR24
   .s) would shrink the compiler dramatically.
3. Even without re-architecting: a pass that deduplicates
   identical helper sequences should cut substantial size.

**Post-fix target: <= 256 KiB compiler image**. 1 MiB is the
default *because* nothing else has been done; it isn't a
budget, it's an absence of effort.

### snobol4 -- ~76 KiB internal buffers

**What the bytes are.** From the survey:
- SB (string buffer): 64 KiB.
- SRC (source buffer): 12 KiB.
- Plus eval/pattern/call stacks (small) and symbol/label tables
  (small).

**Why this big right now.** SB at 64 KiB is sized for "a working
SNOBOL4 program's working data set." Mostly used as a string
pool. SNOBOL4 is string-heavy by design, so a string pool is
algorithmic-floor.

**Category: algorithmic-floor with a side of dead-leak.** The
64 KiB SB is plausible for the language's working set. But
SNOBOL4 had no GC; intermediate strings accumulate, so the
*used* fraction grows over a run.

**Historical benchmark.** SNOBOL4 on IBM 360/370 typical jobs ran
in 64-256 KiB *total*. Our 76 KiB internal *plus* the program
code is in that range. This one is closest to defensible of any
in this list.

**Shrinkage backlog.**
1. Add a string-pool compactor that runs when SB hits 75% full.
   Keeps the *peak* working set small even on long runs.
2. Investigate intern-vs-copy on string operations.
3. Optionally, a smaller SB (32 KiB) for demos that don't need
   the full pool.

**Post-fix target: <= 64 KiB total internal**. Acceptable as-is
*if* the compactor lands; otherwise 32 KiB SB is the right
default and the larger size needs `acknowledge_oversized` per
scenario.

### forth -- dictionary growing up from `dict_end`

**What the bytes are.** Forth's "heap" is the dictionary --
contiguous storage for word headers and code. Grows up from a
linker-defined label `dict_end`. Size is workload-dependent:
the kernel ships ~16 KiB; user-defined words add to it. The
forth-from-forth and forth-on-forthish kernels are larger
because they include their own self-hosted compiler.

**Why this big right now.** Forth's dictionary *is* the program.
Treating it as "heap" is misleading; it's more like "code
segment that grows as you define new words." Algorithmic-floor.

**Category: algorithmic-floor.** No bloat to chase here.

**Historical benchmark.** FIG-Forth in 8-16 KiB. Modern Forth
kernels with full development environment in 32-64 KiB. We are
in that range.

**Shrinkage backlog.** None obvious. Per-kernel profiles in
schema v1.2 should reflect the actual sizes:
- core forth: 16 KiB dictionary
- forth-from-forth: 48 KiB (self-compiler)
- forth-on-forthish: 64 KiB

### basic -- embedded DIM arrays in pascal-compiled BASIC

**What the bytes are.** Static Pascal arrays inside `src/basic.pas`:
- `PS=16384` chars (program text)
- `gs[64]` GOSUB stack
- `fv/fl/fs/fr[16]` FOR loops
- `vars[26]` simple variables
- `apool[1024]` array pool

Total ~64 KiB depending on packing.

**Category: algorithmic-floor.** Sized for a teaching BASIC's
working set. Could shrink with smaller PS for embedded use, but
this is not the bad smell.

**Historical benchmark.** Altair 4K BASIC at 4 KiB. Microsoft
BASIC for the IBM PC at 32 KiB. We're in the same range.

**Shrinkage backlog.** Optional smaller `PS` for compiled-app
profile (single program, no editing) -- 4 KiB sufficient.

### smalltalk -- delegated to basic

The Smalltalk implementation is a host-side `.st`-to-BASIC
translator. The runtime memory is whatever BASIC reserves; no
new heap. Follows basic's analysis above.

### sws / yocto-ed / monitor -- shared regions ~5 KiB each, code ~64 KiB

**What the bytes are.** These three repos use small fixed
shared-memory regions (`RUN_CMD_BUF=0x0F0000` 1 KiB,
`RUN_OUT_BUF=0x0F0400` 4 KiB, `RUN_ENTRY_0=0x0FFE00` 3 bytes;
`SYE_CMD_ADDR=0x0F0000` ~1 KiB, post-quit buffer ~4 KiB;
monitor's `mon_run_request[32]` ~32 bytes plus the program-slot
table).

**Category: algorithmic-floor.** Shared protocol regions sized
for their purpose. Already small.

**Historical benchmark.** A 1981 DOS shell fit in 8 KiB. We're
fine.

**Shrinkage backlog.** None.

## Implications for v1.2 profile budgets

Working numbers, derived from the per-repo analysis above. Each
profile's `budget` block in `[memory_profiles.<name>]` should be
tight enough to flag bloat, loose enough to accommodate the
realistic ceiling.

| profile family       | code+data | heap   | stack | total | example repos |
|----------------------|-----------|--------|-------|-------|---------------|
| `compiled-app`       | <= 32 KiB | <= 16 KiB | <= 8 KiB | <= 64 KiB | apl, basic-runner, smalltalk-output |
| `interpreter-only`   | <= 64 KiB | <= 64 KiB | <= 16 KiB | <= 160 KiB | basic, snobol4 |
| `repl-inline-compile`| <= 128 KiB | <= 256 KiB | <= 32 KiB | <= 448 KiB | ocaml (post-GC), forth-from-forth |
| `compiler-image`     | <= 256 KiB | <= 64 KiB | <= 32 KiB | <= 384 KiB | plsw (post-shrink), pascal compiler |
| `resident-shell`     | <= 64 KiB code per slot, up to 8 slots | per-program | shared 8 KiB | <= 512 KiB total | monitor + sws + N programs |

These are *defaults*. Layers above them require the
`acknowledge_oversized = true` flag and a written
`heap_justification` block; the CI / `--strict` mode rejects
`category = "dead-leak"` and `category = "algorithmic-bloat"`
even when acknowledged.

## Follow-up list (measurements still needed)

1. **ocaml post-GC steady-state heap.** The
   `ocaml-heap-reclaim` saga should produce a measurement. Fold
   into v1.2 budgets when available.
2. **macrolisp typical-demo cell count.** Cell-count = 2K is a
   guess from the demo set; an instrumentation pass would give
   the real working set.
3. **plsw transpiler-output redundancy ratio.** A pass that
   identifies duplicate sequences in `plsw.s` would tell us
   whether dedup gets us to 256 KiB or to 512 KiB.
4. **snobol4 SB peak utilization** across the demo set.
   Determines whether 32 KiB SB is enough by default.
5. **forth dictionary high-water marks** per kernel. Current
   numbers are estimates; instrument once the launcher can run
   each kernel under a `--report-json` budget.

These measurements are not blockers for step 005-config-types;
the launcher schema should accept conservative numbers now and
tighten them as data arrives.

## What this means for the schema

Schema v1.2 (next, in this same step) needs:

1. `[memory_profiles.<name>]` blocks with `partitions = [...]`,
   `budget = { code_max, heap_max, stack_max, total_max }`, and
   an optional `justification_required` flag (default true).
2. Layers cite a profile via
   `[scenarios.<n>.memory_profile = "..."]` or
   `[layers.<n>.memory_profile = "..."]`.
3. `heap_justification` block on any layer claiming heap > 32
   KiB:

   ```toml
   [layers.<n>.heap_justification]
   category = "algorithmic-floor" | "bytecode-image" |
              "gc-slack" | "dead-leak" | "algorithmic-bloat"
   note     = "free text"
   measured_floor_kib = 64    # optional, for gc-slack
   tracking_issue = "..."     # optional
   ```

4. Validator (E0028..E0034 below) enforces budgets and rejects
   oversized claims without acknowledgement.

Step 004 Part 2 lands these in `docs/design.md`.
