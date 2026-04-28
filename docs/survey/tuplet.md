# Survey: tuplet (sw-vibe-coding/tuplet)

The canonical *failing* case. Tuplet is a tuple-first DSL whose
demos are run as OCaml programs interpreted on top of pvm + ocaml
interpreter, with a binary "memory image" preloaded at high SRAM as
a side channel for runtime data. The single non-trivial run script,
`scripts/run-ml-memory.sh`, is the most complex orchestration in
the corpus: four memory loads (counting the input image), two
build-time-resolved patches, UART carrying source plus an EOT, and
a hand-tuned heap geometry that puts the OCaml heap *between* the
two preloaded binaries. The user reports it currently fails. The
schema design must (a) express this layout precisely and (b) catch
the most likely failure mode before the emulator runs.

## 1. Run scripts / entry points

- `scripts/run-ml-memory.sh` -- main path. Takes one or more `.ml`
  files plus a binary `image.bin`. Concatenates per-file module
  headers, normalizes via the same `source_for_repl` awk pass as
  sw-cor24-ocaml, then invokes:

  ```
  cor24-run \
    --load-binary <ocaml-build>/pvm.bin@0 \
    --load-binary <ocaml-build>/ocaml.p24m@0x040000 \
    --load-binary <input_image>@0x080000 \
    --patch 0x<code_ptr>=0x040000 \
    --patch 0x<heap_limit>=0x03F000 \
    --entry 0 \
    -u "<concatenated source>\x04" \
    --speed 0 -n 3000000000
  ```

  Source: `scripts/run-ml-memory.sh:106-117`.

- `scripts/run-ml.sh` -- simpler variant without the binary memory
  image; falls back to the OCaml repo's own runner.
- `scripts/run-lex-parse-fixture.sh`,
  `scripts/run-lexer-fixture.sh` -- micro-tests of the lex/parse
  pipeline against fixture .ml files.
- `scripts/repro-ocaml-issue27.sh`, `scripts/repro-ocaml-issue28.sh`
  -- targeted reproductions of upstream OCaml interpreter bugs.

## 2. Memory loads

| layer            | path                                          | address    | source                | rationale                                  |
|------------------|-----------------------------------------------|-----------:|-----------------------|--------------------------------------------|
| pcode_vm         | `<ocaml>/build/pvm.bin`                       | `0x000000` | vendored from sw-cor24-pcode | COR24-native p-code VM at boot      |
| ocaml_interpreter| `<ocaml>/build/ocaml.p24m`                    | `0x040000` | built by sw-cor24-ocaml      | Pascal-compiled OCaml interp        |
| input_image      | tuplet-supplied `image.bin` (copied + EOT)    | `0x080000` | local (caller arg)           | runtime data side-channel           |

The shell script `cp`s the image to `/tmp/tuplet-memory-input.XXXXXX`,
appends a single `0x03` byte (ETX) before the `--load-binary`, so
the *delivered* image is one byte longer than the source file.
Source: `scripts/run-ml-memory.sh:18-21`.

## 3. Patches

| target address           | value     | source of symbol                          | rationale                              |
|--------------------------|-----------|-------------------------------------------|----------------------------------------|
| `0x<code_ptr>` (build-resolved) | `0x040000` | `<ocaml>/build/code_ptr_addr.txt` | tells pvm where ocaml.p24m starts      |
| `0x<heap_limit>` (build-resolved) | `0x03F000` | `<ocaml>/build/heap_limit_addr.txt` | OCaml heap upper bound; sits *just below* ocaml.p24m at 0x040000 |

Both addresses are looked up at run time by the script, not pinned
in the script source. Source: `scripts/run-ml-memory.sh:90-91`.

## 4. UART payload structure

For one `.ml` file: `source_for_repl <file>` output, then a single
`0x04` byte (EOT). Source: `scripts/run-ml-memory.sh:93-104`.

For multiple `.ml` files: each file is preceded by an injected
`let __module = "Name"` line (with `Name` derived from the
basename, first letter uppercased), then the normalized source.
Backslashes in the assembled string are doubled to survive the
shell pass-through. Source: `scripts/run-ml-memory.sh:97-103`.

The post-EOT runtime data path that sw-cor24-ocaml uses (via
`OCAML_STDIN`) is **not** used here; tuplet's runtime data goes
through `--load-binary` at `0x080000`, not through UART after the
EOT.

## 5. Heaps and stacks

Three regions, each with a different model:

- **pvm internal** (`eval_stack`, `call_stack`, `heap_seg`) --
  embedded inside `pvm.bin`, reserved at fixed offsets, never
  patched. Resolved offsets visible in `pvm.lst` at the OCaml
  repo's vendored pvm v0.1.0. Used by pvm itself for its own
  bookkeeping; no patch from tuplet's side.

- **ocaml value heap** -- one-sided geometry. Only the *limit*
  (top) is patched, to `0x03F000`. There is no patch for a heap
  base; the OCaml runtime treats the region between its own static
  bottom and `heap_limit` as available. The 4 KB gap between
  `0x03F000` and `0x040000` (where ocaml.p24m loads) is a
  deliberate guard to keep the heap from colliding with the
  interpreter image. Grows down toward the pvm at 0x000000.
  Source: `scripts/run-ml-memory.sh:91, 111`.

- **input image** -- not a runtime-managed region, just a static
  blob of bytes loaded at `0x080000`. Read by interpreted OCaml
  code via memory-mapped reads (the DSL builds tuples from it).

## 6. Build artifacts vs vendored

What tuplet builds locally:
- The DSL compiler / lexer / parser tests in `src/` (these are
  Rust binaries on the host; not loaded into emulator memory).
- The optional `image.bin` per demo (varies; the script accepts
  any binary file).

What tuplet vendors / consumes:
- `<ocaml-repo>/build/pvm.bin` -- by relative path, no version pin.
- `<ocaml-repo>/build/ocaml.p24m` -- by relative path, no version
  pin.
- `<ocaml-repo>/build/code_ptr_addr.txt`,
  `<ocaml-repo>/build/heap_limit_addr.txt` -- read at run time.
- `<ocaml-repo>/vendor/sw-em24/<version>/bin/cor24-run` -- by
  vendored sw-em24 path of the OCaml repo, with fallback to
  `command -v cor24-run`.

The version of the OCaml interpreter, the pvm, and cor24-run are
all *transitively* pinned via the OCaml repo's `vendor/active.env`,
but tuplet itself records nothing about which versions of those
upstream artifacts its demos were validated against. If someone
rebuilds the OCaml repo with a different `code_ptr` offset
(`pvm.s` symbol drift), `code_ptr_addr.txt` updates, the run
script picks it up, and the demo still works -- but if the OCaml
heap geometry changes (e.g. a different `heap_limit` placement
strategy), tuplet's pinned `0x03F000` could land *inside* the new
ocaml.p24m and silently corrupt it.

Source: `scripts/run-ml-memory.sh:23-28`.

## 7. Known limits

- `MAX_INSTRS` defaults to `3_000_000_000` (line 17, override via
  trailing numeric arg). No timeout flag passed to cor24-run; only
  the cycle budget terminates a runaway run.
- No explicit max source size, no max image size. Both are bounded
  only by the gap between `0x03F000` (heap top) and `0x040000`
  (ocaml.p24m start) and by the gap between `0x080000` (image base)
  and the EBR stack at `0xFEEC00`.
- The script doubles backslashes (`scripts/run-ml-memory.sh:103`)
  to survive shell quoting; UART payload size is not validated.

## Schema gaps for this repo

- **`heap.grows = "down"` with `patches.heap_limit_only` semantics**
  -- the OCaml heap's only patched symbol is its *limit*, not its
  base or end; the runtime infers the floor from a static label.
  Schema must allow declaring this without lying.

- **Build-time-resolved patch targets** -- both `code_ptr` and
  `heap_limit` come from text files
  (`<repo>/build/<symbol>_addr.txt`) written at build time. The
  schema's `exports.symbols + listing parse` model in design.md
  covers the *production* side; tuplet is a *consumer* of those
  resolutions and needs a way to say "look up symbol X in upstream
  layer's resolved-address sidecar." Either (a) the schema models
  the sidecar directly, or (b) the schema models cross-layer symbol
  resolution and tuplet declares pvm as an upstream layer it pulls
  symbols from.

- **Cross-repo vendored layer with transitive version pinning** --
  tuplet pulls pvm.bin / ocaml.p24m from a sibling repo with no
  explicit pin in tuplet itself; pinning is transitive via the
  sibling's `vendor/active.env`. Schema must support this without
  forcing tuplet to duplicate every pin.

- **Side-channel binary input via --load-binary** -- the input
  image at `0x080000` is data, not code, and is consumed by
  interpreted OCaml. Schema's `kind = "data"` covers this; we
  also need a way to declare the interpreted-language access
  contract (what address layout the DSL expects).

- **Pre-EOT bytes appended to a loaded binary** -- the script
  `cp`s the image and appends `0x03` (ETX) before loading. Schema
  needs either (a) a "pre-process input" hook with bounded
  semantics, or (b) explicit byte-stream composition for `data`
  layers (concatenate file + literal bytes).

- **No top-level config or recorded validation versions** -- the
  repo has no equivalent of `sw-launch.toml`. Adopting it requires
  expressing every layer + patch + UART chunk above. The whole
  point of the tool.

The likely failure mode (covered in
`docs/survey/tuplet-failure-hypothesis.md`) is that without the
4 KB guard or a heap-collision check, an OCaml program that runs
its heap up against `0x040000` overwrites the ocaml.p24m image and
manifests as a TRAP or silent miscompute. The schema needs an
overlap rule (E0003 segment-aware) plus a "guard region" concept
to express the deliberate 4 KB gap.
