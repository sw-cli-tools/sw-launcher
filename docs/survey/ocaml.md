# Survey: sw-cor24-ocaml

The OCaml interpreter is a tree-walk evaluator written in Pascal
(`src/ocaml.pas`) compiled to a single linked .p24 module. At runtime
the COR24 emulator loads the prebuilt PVM native binary at address 0
and the interpreter image at 0x040000, then patches two PVM symbols
(`code_ptr`, `heap_limit`) so the VM enters the .p24m image and bounds
its heap below it. The OCaml source program is shipped to the
interpreter through the UART input, terminated by a 0x04 (EOT) byte;
runtime stdin for `getc`/`read_line` is appended after the EOT via the
`OCAML_STDIN` environment variable. This is the canonical
"nested-interpreter on the COR24 emulator" pattern -- AOT-compiled VM
hosting a Pascal-compiled interpreter executing OCaml source.

## 1. Run scripts

| Script | Purpose | Runner | Image used |
|---|---|---|---|
| `scripts/build.sh` | Compile `src/ocaml.pas` -> `build/ocaml.p24` and `build/ocaml.p24m`; assemble `pvm.s` -> `build/pvm.bin` and `build/pvm.lst`; resolve `code_ptr` and `heap_limit` addrs into `build/code_ptr_addr.txt` / `build/heap_limit_addr.txt` (`scripts/build.sh:64-82`) | `cor24-run --assemble` for PVM, `cor24-run --run p24p.s` for Pascal compile | -- |
| `scripts/run-ocaml.sh` | Run a `.ml` source on the emulator (regression / batch demos) | `cor24-run --load-binary ... --patch ...` (`scripts/run-ocaml.sh:150-154`) | `pvm.bin` + `ocaml.p24m` |
| `scripts/run-ocaml-interactive.sh` | Run a `.ml` source on the host p-code interpreter so live keystrokes feed UART (`scripts/run-ocaml-interactive.sh:33`) | `pv24t` (host binary, NOT emulator) | `ocaml.p24` |
| `scripts/repl.sh` | Interactive terminal REPL on emulator (`scripts/repl.sh:31-35`) | `cor24-run --terminal` | `pvm.bin` + `ocaml.p24m` |
| `scripts/run-pascal.sh` | Smoke test: run a `.pas` through the full toolchain end-to-end (`scripts/run-pascal.sh:84-87`) | `cor24-run --run pvm.s` (older inline pattern) | `pvm.s` (assembled inline) + relocated `.bin@0x010000` + `code_ptr.bin` |
| `scripts/vendor-fetch.sh` | Materialize vendored toolchain artifacts from local upstream repos | n/a | -- |
| `scripts/relocate_p24.py` | Used only by `run-pascal.sh` -- rewrites absolute data refs in a `.p24` body for non-zero load addr (only the older inline path needs this; the new `run-ocaml.sh` path uses `p24-load --load-addr` instead, in build.sh:64) | n/a | -- |

The `justfile` only orchestrates these scripts (`justfile:30-97`).

## 2. Memory loads

`scripts/run-ocaml.sh:150-154` issues exactly two `--load-binary`
arguments to `cor24-run`:

```
--load-binary build/pvm.bin@0
--load-binary build/ocaml.p24m@0x040000
```

`pvm.bin` is the AOT-assembled PVM (native COR24 code). `ocaml.p24m` is
the linked, relocated multi-unit p-code image of the interpreter +
Pascal runtime (produced by `p24-load --load-addr 0x040000` in
`scripts/build.sh:64`). The interpreter image base 0x040000 is fixed in
both `build.sh` (relocate) and `run-ocaml.sh` / `repl.sh` (load-binary
+ patch).

`scripts/run-pascal.sh:84-87` uses a different pattern (inline assemble
PVM and load `.bin@0x010000` + tiny `code_ptr.bin@<addr>`) but that is
only the Pascal smoke path, not the OCaml run path.

## 3. Patches

Two `--patch` operations at `scripts/run-ocaml.sh:152-153` and
`scripts/repl.sh:33-34`:

```
--patch 0x${CODE_PTR}=0x040000
--patch 0x${HEAP_LIMIT}=0x03F000
```

Both addresses are read at runtime from build artifacts:

- `build/code_ptr_addr.txt` -> currently `1279` (i.e. patch the word at
  0x001279 to 0x040000)
- `build/heap_limit_addr.txt` -> currently `127D` (i.e. patch the word
  at 0x00127D to 0x03F000)

These are extracted in `scripts/build.sh:70-81` by greping the PVM
listing for `code_ptr:` and `heap_limit:` and capturing the next line's
address. This is the canonical **build-time-resolved patch address**
pattern. Anything else hard-coded would drift if `pvm.s` were edited.

`code_ptr` redirects PVM execution from its built-in `code_seg` stub
(`vendor/sw-pcode/v0.1.0/bin/pvm.s:3567-3572`) to the loaded image at
0x040000. `heap_limit` (default 0x00F000 in pvm.s:3589) is moved up to
0x03F000 to give the heap nearly all of the 0x000000..0x040000 window
below the image, with a 4 KB guard gap right under the image.

## 4. UART payload structure

`scripts/run-ocaml.sh:148`:

```
UART_INPUT="${ML_INPUT}"$'\x04'"${OCAML_STDIN:-}"
```

So the UART preload is laid out as:

```
[ source text (one or more .ml files joined) ] 0x04 [ OCAML_STDIN bytes ]
```

For multi-file modules, `run-ocaml.sh:136-145` prefixes each source
with a `let __module = "Name"` line and concatenates. The
`source_for_repl` awk pass (lines 56-122) folds continuation lines so
the REPL sees one logical statement per line.

`0x04` is the EOT terminator the lexer's `lex_init` looks for. After
EOT, residual UART bytes feed `getc`/`read_line` at runtime
(`docs/stdin-and-getc.md:21-32`). The Pascal runtime's one-byte
lookahead means the very first runtime read sees the trailing 0x04 and
must skip it (`docs/stdin-and-getc.md:35-50`).

`scripts/run-ocaml-interactive.sh:33` builds a similar payload but
passes it to `pv24t -i` (host p-code interpreter) so stdin stays bound
to the live terminal:

```
"$PV24T" "$BUILD_DIR/ocaml.p24" -n 0 -i "$(cat "$ML")$(printf '\x04')"
```

## 5. Heaps and stacks

The interpreter does **not** carry its own heap base/end/limit globals.
All heap state lives in PVM and is patched by run-ocaml.sh. Inside
`pvm.s`:

- `code_ptr` (pvm.s:3571, listed at 0x001279) -- patchable code base.
- `heap_limit` (pvm.s:3589, listed at 0x00127D) -- patchable allocation
  ceiling. Default 0x00F000; OCaml run patches to 0x03F000.
- `globals_seg` (pvm.s:3614) -- 1536-byte fixed globals area, listed at
  0x001290. Unused for .p24m images (which carry their own globals).
- `call_stack` (pvm.s:3668, listed at 0x001890) -- 4096 bytes,
  embedded. Bounds-checked by op_call/op_calln/op_enter/op_xcall.
  pvm.s:3666 explicitly sizes it for OCaml `eval_expr` (~57 B/frame ->
  ~72 frames headroom).
- `eval_stack` (pvm.s:3800, listed at 0x002890) -- 1536 bytes,
  embedded.
- `heap_seg` (pvm.s:2712 in listing, at 0x002E90) -- start of the
  arena. The heap grows upward from `heap_seg` toward `heap_limit`
  (which run-ocaml.sh sets to 0x03F000, just under the image base).

The heap, eval stack, and call stack are all part of PVM's static
layout, embedded in `pvm.bin` at fixed offsets relative to address 0.
The OCaml interpreter calls Pascal `new(...)` (`docs/heap-survey.md:11`)
which lowers to PVM `sys ALLOC`; allocations come out of `heap_seg` and
trip TRAP 5 if `hp >= heap_limit`. There are 44 `new(...)` sites across
four record types (Expr, Pat, Val, EnvEntry) catalogued in
`docs/heap-survey.md:60-141`. A mark/sweep collector with `top_env` as
its only root runs at the top of each REPL transaction
(`docs/gc-design.md:18-42`) using Pascal's `dispose()` which routes
through `sys_free`.

There is **no** patch for "heap_base" or "heap_end" -- the floor is
implicit (`heap_seg`, fixed by the PVM build) and only the ceiling
(`heap_limit`) is patchable. The eval/call stacks have no patch hooks
at all; their addresses come out of the PVM listing but the launcher
never references them.

## 6. Build artifacts vs vendored

Artifacts produced by `scripts/build.sh` into `build/`:

- `ocaml.spc` -- Pascal compiler output (text .module/.endmodule).
- `ocaml_linked.spc` -- after `pl24r` link with `runtime.spc`.
- `ocaml.p24` -- assembled, single linked module, base 0 (used by
  `pv24t` interactive path).
- `ocaml.p24m` -- relocated to 0x040000 via `p24-load --load-addr`
  (used by `cor24-run`).
- `pvm.bin` -- pre-assembled native PVM.
- `pvm.lst` -- listing, mined for symbol addresses.
- `code_ptr_addr.txt` -- `1279` (resolved at build time).
- `heap_limit_addr.txt` -- `127D` (resolved at build time).

Vendored under `vendor/`:

- `vendor/active.env` pins `SW_PASCAL_VERSION=v0.1.0`,
  `SW_PCODE_VERSION=v0.1.0`, `SW_EM24_VERSION=v0.1.0`.
- `vendor/sw-pascal/v0.1.0/bin/` -- `p24p.s` (Pascal compiler, COR24
  asm), `runtime.spc` (Pascal runtime in p-code asm), `runtime.spi`,
  `p24p_rt.p24` (prebuilt runtime unit binary).
- `vendor/sw-pcode/v0.1.0/bin/` -- `pvm.s` (PVM source), `pa24r`
  (assembler, host bin), `pl24r` (linker, host bin), `p24-load`
  (multi-unit relocating loader, host bin).
- `vendor/sw-em24/v0.1.0/bin/` -- `cor24-run` (emulator + assembler,
  host bin; falls back to system PATH).

`version.json` files cite local upstream paths and pinned commits, e.g.
`sw-pcode` commit `cd8a6a7a...` (vendor/sw-pcode/v0.1.0/version.json:7),
`sw-pascal` commit `5456588e...`,
`sw-em24` commit `e7c79012...`. None of the vendored versions are
checksummed yet (`sha256: TBD`).

## 7. Known limits

From `docs/ocaml-bring-up-issues.md`: the current production path
(build.sh + run-ocaml.sh) uses unit-mode link via `p24-load`, but the
older `run-pascal.sh` path is the only one fully proven for arbitrary
UART input -- there is a documented io_init/lookahead interaction
between unit-mode globals and the p24p runtime (lines 116-145).

`docs/heap-survey.md:204-207`: there are no manual mark/release or
arena patterns; allocation is `new(...)` then leak. A mark/sweep
collector exists (`docs/gc-design.md`) but only the top-of-REPL trigger
is implemented; mid-eval collection would require root-stack
instrumentation that is not present.

`docs/gc-design.md:121-129`: the Pascal compiler caps procedures at
128 (`MAX_PROCS = 128`); `src/ocaml.pas` is at 127. That bounds how
much the GC can grow without inlining.

`docs/stdin-and-getc.md:163-164`: `read_line`-based loops cost call
frames that risk TRAP 2 (stack overflow) past ~6 levels deep, because
`call_stack` is only 4 KB.

Demos that need live keystrokes (`demo-echo-loop`, `demo-guess`,
`demo-adventure`) bypass the emulator entirely and run on `pv24t` via
`run-ocaml-interactive.sh`; they are not exercised by the
emulator+patch path.

## Schema gaps for this repo

A schema that captures only "image + load-addr + entry" loses the
following load-bearing facts about how this repo runs:

1. **Two-binary nested load.** The launcher must express loading both
   the AOT VM (`pvm.bin@0`) and the interpreter image
   (`ocaml.p24m@0x040000`) in one invocation. A "single image" model
   does not fit.

2. **Build-time-resolved patches with named symbols.** The patch
   addresses (`code_ptr`, `heap_limit`) are not literals -- they are
   resolved from `pvm.lst` at build time and dropped into
   `build/code_ptr_addr.txt` and `build/heap_limit_addr.txt`. The
   schema needs: "patch <symbol-resolved-from-build-artifact-file> =
   <value>", not a hard-coded address. There are exactly two such
   patches today; the value of one (0x040000) equals the second
   image's load-addr, the value of the other (0x03F000) is
   image-load-addr minus a fixed 4 KB guard. Both relationships should
   be expressible.

3. **UART payload composition.** The launcher drives UART preload as
   `<source-bytes> 0x04 <stdin-bytes>`. The schema needs to express:
   (a) source ingestion via UART terminated by EOT 0x04, (b) optional
   appended stdin, (c) source pre-processing (the `source_for_repl`
   awk pass that folds continuation lines), and (d) multi-file module
   composition (per-file `let __module = "Name"` headers joined by
   newlines).

4. **Two run modes for one repo.** The same interpreter is launched
   two different ways: `cor24-run` for canned/regression demos and
   `pv24t` for live-keystroke demos. The launcher schema must allow
   one repo to declare more than one runner profile and select at
   invoke time. This repo's `ocaml.p24` (base 0) and `ocaml.p24m`
   (base 0x040000) are produced from the same Pascal sources but
   target different runners.

5. **Heap geometry is one-sided.** Only the heap *ceiling* is
   patchable here; the floor is implicit at PVM's `heap_seg` (fixed at
   0x002E90 in this build). No `heap_base`/`heap_end` patches exist.
   eval_stack and call_stack are statically embedded at fixed offsets
   in `pvm.bin` and are not patched at all. The schema must allow
   "patches a subset of memory-region anchors, leaves the rest
   implicit/embedded."

6. **Vendored toolchain pin via env file.** `vendor/active.env` is the
   single source of truth for tool versions. The schema's manifest
   needs to express dependence on multiple vendored upstreams
   (`sw-pascal`, `sw-pcode`, `sw-em24`) each pinned independently,
   with paths under `vendor/<tool>/<version>/bin/`.

7. **Build artifacts contain symbol resolutions.** A clean schema must
   model `build/<symbol>_addr.txt` as a first-class concept produced
   by the build step and consumed by the run step; otherwise the
   patch addresses look like magic numbers.

8. **`-n max_instructions` and `--speed 0` are part of the run
   contract.** `run-ocaml.sh:154` defaults to `-n 3000000000` (and
   accepts an override last-arg); REPL uses `-n -1 -t 3600 --terminal`
   (`scripts/repl.sh:35`). The launcher schema must distinguish
   bounded-batch vs unbounded-interactive and allow per-profile
   `--terminal`/`--speed`/`-n`/`-t` parameterization.
