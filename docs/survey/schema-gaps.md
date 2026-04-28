# Schema gaps observed across the survey

Aggregates the "Schema gaps for this repo" sections from the 13
per-repo surveys. Each gap below cites the repo(s) that need it
and (where helpful) a concrete example. Step 002-revise-schema is
expected to address each gap by adding a TOML field, generalizing
an existing one, or recording a justified out-of-scope decision.

## A. Layer kinds and load methods

A1. **Composite image from N modules linked host-side.**
- `snobol4`: four .s modules concatenated by `link24` into one
  binary loaded at 0x000000.
- `macrolisp`: multi-module demo loads `uart`/`spi`/`i2c`/`gpio`/
  `timer` at fixed `--base-addr 0x1000..0x5000` slots.
- `monitor`: monitor + sws + each program preloaded at distinct
  slot addresses by the same `cor24-run` invocation.
- Suggested: a `composite` layer kind that emits a list of
  `--load-binary` args, with per-module `base_addr`, optionally
  driven by a host-side linker tool (`link24`, `pl24r`, ...).

A2. **Memory image as opaque data side-channel.**
- `tuplet`: `image.bin@0x080000` carrying tuple data consumed by
  the interpreted DSL.
- `snobol4`: optional input data at `@0x090000` whose *presence*
  flips a runtime mode flag (`INP_TTY=0|1`).
- `macrolisp`: snapshot blob `prelude.snap@0x080000`.
- Suggested: extend `kind = "data"` with a `presence_implies`
  hook to declare the runtime mode flag a layer's presence
  toggles, plus byte-level pre-processing (e.g. append ETX).

A3. **Source loaded as a UART preamble** before / instead of
real source.
- `forth-from-forth`, `forth-in-forth`, `forth-on-forthish`: each
  has its own ordered list of `core/*.fth` preamble files.
- Suggested: `kind = "uart-preamble"` with an ordered file list
  and concatenation rules.

A4. **Generated kernel checked into git but rebuildable.**
- `forth-from-forth`: `kernel.s` is committed but reproducible
  via `scripts/build-kernel.sh`.
- Suggested: `regen` field on a layer that points at the rebuild
  script; cache key incorporates the script's output hash.

A5. **Snapshot-then-restore pipeline.**
- `macrolisp`: `snapshot-save.s` writes ASCII hex; host
  `extract-snapshot.py` rehydrates to `prelude.snap`;
  `repl-snapshot.s` loads the snapshot. The launcher must
  understand both the *generate* and *load* phases.
- Suggested: a two-phase scenario where phase 1 emits an
  artifact that phase 2 consumes.

## B. Patches and symbol resolution

B1. **Build-time-resolved sidecar files.**
- `ocaml`, `tuplet`: `<repo>/build/code_ptr_addr.txt` and
  `heap_limit_addr.txt` written at build time, read at run time.
- Suggested: sidecar source kind `value = "sidecar:<path>"` plus
  an upstream-layer reference, so a downstream consumer can
  pull a resolved address without re-deriving it.

B2. **Symbol pulled from listing of an *upstream* layer.**
- Cross-repo: tuplet wants pvm symbols from sw-cor24-ocaml's
  build, not its own.
- Suggested: `patches.target = "<upstream-layer>.<symbol>"`
  where the upstream may be vendored, not local.

B3. **Heap geometry with limit-only patch.**
- `ocaml`, `tuplet`: only the heap *ceiling* is patched; the
  floor is a static label inside the runtime.
- Suggested: `segments.<name>.kind = "heap"`, `grows = "down"`,
  with patches that target only `heap_limit` (no `heap_base` /
  `heap_end`).

B4. **Configurable hardware stack size.**
- `apl` samples: `--stack-kilobytes 8`.
- `script`: bumps stack to 8 KB on demand.
- `monitor`: shared 3 KB EBR for all resident processes.
- Suggested: `targets.<t>.regions.ebr_stack.size_override` and
  per-scenario `run.stack_kilobytes`.

## C. Build pipelines

C1. **Compile-by-running-the-emulator.**
- `basic`, `plsw`: the compiler is itself a COR24 program; the
  build invokes `cor24-run` with the compiler image and source
  over UART, captures the output (between markers), and produces
  the next stage's input.
- Suggested: a `tool.kind = "emulator-hosted"` invocation type
  with explicit input/output framing markers.

C2. **Sed/awk post-processing on intermediates.**
- `basic`: sed-rewriting on `.spc` intermediate.
- `plsw`: regex extraction between marker lines.
- `forth`: cropping output between `!!BEGIN-KERNEL!!` /
  `========` markers.
- Suggested: `tool.post_process` field with a small allow-list
  of named transforms (regex extract between markers, strip
  prefix lines, ASCII filter).

C3. **Multi-stage pipeline per program.**
- `plsw`: `<name>-combined.plsw` -> `<name>.s` -> `<name>-dump.txt`.
- `pascal` multi-unit: `.spi` files prepended with markers,
  re-fed via UART.
- Suggested: chained tool invocations expressed as a layer DAG;
  intermediate artifacts are first-class layers with caching.

C4. **Test-transcript splice between sentinel lines.**
- `smalltalk`: optional canned transcript spliced into the BASIC
  output between sentinels.
- Suggested: a `splice` post-process step on text outputs.

## D. UART payload composition

D1. **EOT then runtime-data semantics.**
- `ocaml`: source + 0x04 + OCAML\_STDIN appended.
- `tuplet`: source + 0x03 / 0x04 (ETX in input image, EOT in
  UART). Two separate channels.
- Suggested: ordered list of `uart` layers each with its own
  `terminator` byte; concatenation order is declarative.

D2. **No-terminator policies.**
- `forth`: literal `\n\r\t\xNN` are *interpreted* by cor24-run
  pre-loading; UART must avoid them.
- `apl`: literal `\n` between lines, no EOT; in-band terminator
  is `)OFF`.
- Suggested: `uart.encoding = "raw" | "escape-interpreted"` and
  `uart.in_band_terminator` for languages with their own EOF.

D3. **Module-header injection per source file.**
- `ocaml`, `tuplet`: each `.ml` is prefixed with
  `let __module = "Name"`.
- `pascal` multi-unit: each `.spi` wrapped in
  `;--- SPI <name> ---` / `;--- END SPI ---`.
- Suggested: `uart.per_file.prelude_template` with `{name}`,
  `{stem}`, `{module}` placeholders.

D4. **Source pre-normalization (logical-line folding).**
- `ocaml`, `tuplet`: `source_for_repl` awk pass folds
  continuation / `|` arms / `and`-bindings into single logical
  lines.
- Suggested: a small set of named normalizers (`logical-line-fold`,
  `strip-comments`, `strip-prefix`) with parameters.

D5. **Distinguishing "command text" UART from "source code"
UART from "raw bytes" UART.**
- `script` (sws): commands.
- All language repos: source code.
- `tuplet`, `snobol4`: raw bytes in a loaded binary.
- Suggested: `uart.role = "command" | "source" | "data"` for
  human-readable diagnostics and to disable source normalizers
  on command channels.

## E. Multiple run modes per repo

E1. **Repo-level run profiles.**
- `snobol4`: `run-snobol4.sh` (batch, dump+awk) vs
  `run-snobol4-tty.sh` (terminal, no data load).
- `pascal`: single-unit (`pl24r`-only) vs multi-unit (`p24-load`).
- `ocaml`: `cor24-run` (regression) vs `pv24t` (live keystrokes).
- `smalltalk`: `run-bare.sh` / `run-st.sh` / `run.sh`.
- Suggested: `[scenarios.<name>.profiles.<profile>]` overrides a
  base scenario's `run.*` and load list.

E2. **Filename-pattern emulator-flag overrides.**
- `apl`: presence of a `.cor24` file name flips
  `--load-binary <file>@0x080000` and `--patch ...=0x080000` on.
- Suggested: `[scenarios.<name>.conditional_loads]` with simple
  predicates (file present / absent, env var set, profile name
  matches).

E3. **Interactive vs batch modes from one binary.**
- `monitor`, `script`, `yocto-ed`: same image, different runtime
  mode; the launcher sets `--terminal` or pipes UART based on
  intent.
- Suggested: `run.mode = "batch" | "terminal" | "echo-line"`
  controlling cor24-run flags.

## F. Resident-process / shell models

F1. **Service vector + trampoline transfer of control.**
- `monitor`: programs invoked via `mon_invoke_program`; non-local
  return via `svc_exit_impl` (longjmp-style).
- Suggested: scenario can declare a `resident_process` flag; the
  launcher does not try to "run to completion" -- the user (or
  test driver) interacts via UART.

F2. **Shared-memory protocol regions.**
- `script` (sws): `RUN_CMD_BUF=0x0F0000`, `RUN_OUT_BUF=0x0F0400`,
  `RUN_ENTRY_0=0x0FFE00`.
- `yocto-ed`: `SYE_CMD_ADDR=0x0F0000`, post-quit buffer at
  `0x0F0400`.
- Monitor: `mon_run_request[32]` shared buffer.
- Suggested: `[layers.<n>.shared_regions]` mapping symbolic names
  to address ranges, with overlap-checked validation as for any
  other segment.

F3. **Pre-staged UART command stream.**
- `yocto-ed`: canned keystrokes at `0x0F0000` consumed before
  real UART.
- Suggested: a special `kind = "uart-prebuffer"` that loads at a
  fixed memory address rather than via cor24-run's UART path.

## G. Vendoring and tool resolution

G1. **Three vendored toolchains in one repo.**
- `plsw`: `sw-asx24`, `sw-cx24`, `sw-em24` each with
  `active.env`-pinned versions (currently TBD-stubbed).
- Suggested: top-level `[tools.<n>]` entries with `vendor:` source
  resolution; multiple tool slots are not exotic.

G2. **No-pin-from-PATH stance.**
- `apl`, `forth`, `macrolisp`, `monitor`, `script`, `yocto-ed`:
  no version pinning at all.
- Suggested: `sw-launch doctor` records the *observed* version of
  each PATH-resolved tool and surfaces it; lockfile records it
  even when not explicitly pinned, so drift is visible.

G3. **Sibling-repo paths with no pin.**
- `basic`, `pascal`, `smalltalk`, `snobol4` (HOME-relative).
- `tuplet`: transitively pinned via sibling's vendor.
- Suggested: `source = "sibling:<path>"` as a first-class
  resolution kind; lockfile records the sibling's HEAD SHA at
  pin time.

## H. Validation rules to add

H1. **Guard region between layers.** Tuplet needs to declare the
4 KB gap between `0x03F000` and `0x040000` as deliberate; an
overlap rule alone can't tell guard from accident.
- Suggested: new error code for "no guard between adjacent layers
  > N bytes apart could fit a future layer"; warn-not-error.

H2. **Cycle / timeout budget sanity.** Some scenarios run 1000x
the median; flag (warn, don't fail) outliers.

H3. **Shared-memory protocol overlap.** `script` and `yocto-ed`
both use `0x0F0000` as their command region; fine for them
individually but a launcher composing them in one scenario must
catch the conflict.

## I. Out-of-scope candidates

The following gaps were observed but are explicitly not in scope
for the first launcher schema (revisit when concrete demand
appears):

- I1. Filesystem stubs (`fs_read_file` in sws). The launcher
  doesn't need a virtual filesystem.
- I2. Mark-compact GC scheduling annotations. The runtime owns
  this; the launcher only owns the heap region.
- I3. Diff-based snapshot updates. Future Phase 4+ if at all.
