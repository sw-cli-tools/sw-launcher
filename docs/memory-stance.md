# Memory budget stance

Operating principle for step 004-schema-v1.2-variable-partitions
and every later memory-budget decision in this project.

## Historical context

The COR24 board emulator targets 1 MiB SRAM. That is *more*, not
less, than every machine these re-implemented languages were
originally designed for:

- **Forth** (1970s minicomputers): kernels ran in 4 KiB to 16 KiB,
  full systems with editor, assembler, compiler, and applications
  in 32 KiB to 64 KiB.
- **APL** (IBM 1130, 1960s): 8 KiB to 16 KiB words. APL/360 ran
  full timesharing in <128 KiB per user partition.
- **BASIC** (Altair 4K BASIC, 1975): 4 KiB. Microsoft BASIC for
  the IBM PC fit in 32 KiB.
- **OCaml ancestors** (LISP, ML on 1970s machines): 64 KiB to
  256 KiB total memory; not just heap.
- **SNOBOL4** (1967, IBM mainframes): typical jobs in 64 KiB to
  256 KiB.
- **Smalltalk-72/76** (Xerox Alto): 128 KiB to 512 KiB total
  including bitmap display.
- **Pascal** (original CDC 6600 implementation): 60-bit words but
  small core; UCSD Pascal on 64 KiB micros.
- **Macrolisp / Maclisp** (PDP-10): 256 KiB total.

The IBM PC shipped in 1981 with 16 KiB to 256 KiB. Bill Gates'
apocryphal "640 KiB ought to be enough for anybody" reflected the
state of 1985 productivity software, not a per-process heap. By
that ceiling, *whole DOS systems* with multiple resident TSRs ran
in 640 KiB.

## What we have

A 1 MiB COR24 emulator. No graphics card. No clock. Very limited
I/O. Simpler than an IBM PC of 1982. Instead of DOS, a tiny
monitor.

That is not a constrained environment by historical standards. It
is, by 1985 measure, a luxurious one.

## What this means for step 004 and beyond

When a survey finds that an OCaml interpreter wants 256 KiB of
heap, or that PL/SW's compiler image is approaching 1 MiB, or
that tuplet has been doubling its heap reservation in a failed
attempt to avoid OOM, the *prior* should be:

> Something has gone soft. Find it.

Not:

> The COR24 is too small.

The default disposition for any heap > 32 KiB is *suspicious until
justified*. Acceptable categories of justification, in roughly
descending order of merit:

1. **Algorithmic floor** -- a problem that genuinely requires the
   working set (e.g., a 64 KiB hash table the program must build
   to do its job; a bytecode image that is intrinsically that
   size).
2. **Bytecode/data image** -- the heap is mostly read-only data
   that arrived as part of a loaded artifact, not allocations.
3. **GC catching up** -- the heap is sized for the floor *plus*
   slack between collections; legitimate, with a sized-floor
   measurement.
4. **Dead-leak** -- the bad smell. Allocations that were never
   freed because the runtime has no GC, or has a no-op
   `dispose`/`free`. **This is the current state of OCaml/tuplet
   pre-GC.** Not a justification, a defect.
5. **Algorithmic bloat** -- using a 32-bit pointer where a 16-bit
   index would do, boxing primitive integers, etc. Often
   fixable in one pass.

The schema v1.2 `heap_justification.category` enum should match
this list (`algorithmic-floor | bytecode-image | gc-slack |
dead-leak | algorithmic-bloat`), and the validator should treat
`dead-leak` and `algorithmic-bloat` as warnings rather than
acceptable justifications. A scenario whose heap is *only*
justified by `dead-leak` should not pass `sw-launch check
--strict`.

## Rules of thumb (working numbers, refine in step 004 Part 1)

- **Total claimed SRAM <= 1 MiB.** Hard cap, that's all there is.
- **Code + data per layer**: usually < 64 KiB; layered images
  (pvm + interp + app) sum to < 256 KiB total code+data.
- **Heap per scenario**: aim for < 256 KiB; *consider* < 64 KiB
  the comfortable default for any single language layer.
- **Stack per scenario**: aim for < 32 KiB total, including the
  3 KiB EBR; multiple stacks for separate languages each get
  modest reservations.
- **Anything > 0.5 MiB heap or > 0.5 MiB stack**: requires the
  `acknowledge_oversized = true` flag and a written justification
  in TOML.
- **Anything > 32 KiB heap**: requires a
  `heap_justification` block.

These are *defaults*. They can be relaxed per-profile if the
profile's intended use legitimately needs more (REPL with in-line
compilation may legitimately want a larger compile arena). The
relaxation has to be argued, not assumed.

## Implication for the step 004 heap-analysis doc

For each repo with claimed heap > 32 KiB, after categorizing per
the list above, the analysis should:

- Default-assume **dead-leak** until proven otherwise.
- Cite the smallest historical implementation of the same
  language as the *upper* benchmark; if 1985 SNOBOL4 on a 256 KiB
  mainframe could compile and run a working program, our 1 MiB
  COR24 SNOBOL4 should not need anywhere near that. Where it
  does, that's an inversion of expectations and the doc should
  call it out.
- Identify the work that would shrink the heap (GC pass, smaller
  pointer width, freelist, escape analysis, bytecode compaction)
  *before* arguing for a profile budget that accommodates the
  current size.
- Output a **shrinkage backlog** -- per repo, the specific
  changes that would bring the heap claim down to a defensible
  number, ordered by estimated effort.

The point is: schema v1.2 must support large heaps when needed,
but the heap-analysis doc must not normalize the current sizes.
The current sizes are the *evidence of work to do*, not the
*specification for the launcher*.

## A note on tuplet

The user reports the OCaml GC work in flight (`sw-cor24-ocaml`
saga `ocaml-heap-reclaim`) is exactly the kind of fix this
stance expects. Tuplet's failure mode -- doubling the heap
reservation when it ran out, then doubling again -- is a textbook
dead-leak symptom. After GC lands, tuplet's `heap_limit` should
*shrink*, not stay where it is. The launcher schema should be
ready to express the smaller number, and the step 004 analysis
doc should record the pre-GC size as a *historical reference*
rather than as the budget.

## Relation to step 004 prompt

Step 004's prompt currently says "for each repo with claimed heap
> 32 KiB ... why this big, how much is dead, with-GC floor." This
doc tightens the *prior*: the analysis starts from "this is dead
allocation" and the writer's job is to find evidence that says
otherwise, not the reverse. Step 004 should cite this doc
explicitly in the heap-analysis output's framing.
