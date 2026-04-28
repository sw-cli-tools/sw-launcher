# Survey index: how COR24 language repos run today

Thirteen repos surveyed. Working language repos:
[apl](apl.md), [basic](basic.md), [forth](forth.md),
[macrolisp](macrolisp.md), [ocaml](ocaml.md),
[pascal](pascal.md), [plsw](plsw.md),
[smalltalk](smalltalk.md), [snobol4](snobol4.md). Resident
shell / editor approach:
[monitor](monitor.md), [script](script.md),
[yocto-ed](yocto-ed.md). Failing canonical case:
[tuplet](tuplet.md).

The schema gaps each repo surfaced are aggregated in
[schema-gaps.md](schema-gaps.md). The tuplet failure analysis is
in [tuplet-failure-hypothesis.md](tuplet-failure-hypothesis.md).
The cross-cutting "monitor-based edit-and-run for other languages"
analysis is in [monitor-shell-feasibility.md](monitor-shell-feasibility.md).

## Comparison table

The columns are: layers loaded by `--load-binary`; `--patch` count
per typical scenario; whether UART carries source code, runtime
data, both, or neither; whether the heap is **emb**edded in an
artifact or **res**erved by the launcher; same for the stack;
heap growth direction; rough total SRAM used (code + reserved
regions). N/A means the repo does not exhibit that pattern.

| repo        | loads | patches | UART src | UART data | heap   | stack  | heap grows | approx SRAM |
|-------------|------:|--------:|----------|-----------|--------|--------|------------|-------------|
| apl         | 0-1   | 0-1     | yes      | no        | emb    | hw EBR | up         | < 100 KiB   |
| basic       | 1     | 0       | yes      | no        | emb    | hw EBR | up         | ~64 KiB     |
| forth       | 0     | 0       | yes      | no        | emb    | hw EBR | up (HERE)  | ~256 KiB    |
| macrolisp   | 0-1   | 0       | yes      | no (snap) | emb    | hw EBR | up         | ~512 KiB    |
| ocaml       | 2     | 2       | yes      | yes (post-EOT) | emb+res | emb (in pvm) | down (limit only) | ~512 KiB |
| pascal      | 1-2   | 1       | yes      | no        | emb    | emb    | up         | ~64 KiB     |
| plsw        | 0     | 0       | yes      | no        | emb    | hw EBR | up         | ~1 MiB      |
| smalltalk   | 0     | 0       | hands off | hands off | (delegated to basic) | -- | -- | -- |
| snobol4     | 1-3   | 0       | yes      | yes (mode-flag) | emb | emb | up | ~128 KiB |
| monitor     | many  | 0       | no       | no        | emb    | hw EBR | up         | ~64 KiB     |
| script (sws)| 0+1   | 0+1     | commands | no        | emb    | hw EBR | up         | ~64 KiB     |
| yocto-ed    | 1     | 0       | edit cmds | no       | emb    | hw EBR | up         | ~64 KiB     |
| tuplet      | 3     | 2       | yes      | yes (img@0x080000) | res | emb (in pvm) | down (limit only) | ~768 KiB |

## Patterns observed

Five distinct shapes emerged:

1. **Single-image-at-zero**, UART-fed source. Heap and stack are
   embedded inside the image. Examples: apl, basic, forth, plsw,
   smalltalk (delegated). The simplest case; `sw-launch` Scenario A.

2. **Runtime + image + patch**. A native COR24 runtime at 0 plus
   one or more p-code images at higher addresses, with a
   `code_ptr`-style patch tying them together. Examples: pascal
   (single-unit), pascal (multi-unit), the OCaml/tuplet pattern
   without the heap patch. `sw-launch` Scenario B.

3. **Nested interpreter with heap-limit patch and UART-after-EOT
   data**. Pattern adds a *second* patch (heap limit), and the
   UART payload is composed of `<source> + EOT + <runtime data>`.
   Examples: ocaml, tuplet (the runtime-data path moves to a
   loaded binary at 0x080000 instead of post-EOT UART).
   `sw-launch` Scenario C.

4. **Multi-module composite image**. The launcher loads N
   independently assembled modules at contiguous bases (snobol4)
   or at fixed slot addresses (macrolisp's multi-module demo;
   monitor's program registry). Linking happens host-side in a
   `link24`-style step, not via patches. Schema needs a "composite
   image" layer kind that emits multiple loads from one logical
   layer.

5. **Resident shell + paste-and-go**. Monitor at 0, sws shell at
   0x20000, programs at fixed slot addresses, all preloaded
   together; transfer of control happens *inside* the emulator
   via a service-vector / trampoline (mon\_invoke\_program),
   never returns to the host runner. The host runner kicks the
   emulator off and reads UART; the user then drives the rest of
   the session interactively. Examples: monitor, script (sws),
   yocto-ed (edit-only variant).

## Two axes, not one

Every survey points at a 2D taxonomy:

- **Build axis**: is the image (a) hand-written assembly, (b)
  compiled from a higher-level language, (c) a snapshot rehydrated
  by host tooling, or (d) a composite of N modules linked
  host-side?
- **Run axis**: is the run (a) one-shot batch (kick off and check
  UART), (b) interactive REPL through UART, (c) interactive shell
  with a resident process model, or (d) edit-then-run via a
  resident editor?

The current `docs/design.md` schema covers (build a, b) cleanly
and (run a) cleanly. The other build modes need explicit
support; the other run modes require both a config-time
description and a runtime mode flag in the launcher.

## Vendoring posture is a mess

| repo        | vendor model | version pin |
|-------------|--------------|-------------|
| apl         | none         | tools from PATH only |
| basic       | none         | sibling-repo `../sw-cor24-*` paths |
| forth       | none         | tools from PATH only |
| macrolisp   | none         | tools from PATH only |
| ocaml       | `vendor/<tool>/<ver>/` with `active.env` | exact, with commit SHAs |
| pascal      | none         | sibling-repo paths |
| plsw        | `vendor/<tool>/<ver>/` with `active.env`, fields TBD | declared, not enforced |
| smalltalk   | none         | sibling-repo paths |
| snobol4     | none         | hard-coded HOME paths |
| monitor     | none         | tools from PATH only |
| script      | none         | none |
| yocto-ed    | none         | none |
| tuplet      | transitive via ocaml's vendor/ | inherited, undeclared |

`sw-launch` should make the OCaml-style vendored model the
default. Repos with no current pinning should be upgraded as part
of adopting `sw-launch`.

## Cycle and timeout budgets vary by 3 orders of magnitude

Smallest budget: forth selftest `-n 5_000_000`. Largest:
forth-on-forthish `-n 3_000_000_000`, ocaml/tuplet
`-n 3_000_000_000`. plsw uses both a cycle budget *and* a wall
timeout (`-t 30s`/`-t 120s`). Schema should accept both, default
to a sensible bound, and warn (not error) when a scenario sets a
budget more than 100x the median for its target.
