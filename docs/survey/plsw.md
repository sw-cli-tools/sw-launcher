# Survey: sw-cor24-plsw

PL/SW (Programming Language for Software Wrighter) is a freestanding,
PL/I-inspired systems language transpiling to COR24 24-bit assembler.
The compiler itself is written in C and is built by the vendored sw-cx24
cross compiler into a single COR24 assembly file (build/plsw.s, ~356K
lines), which then runs *on the COR24 emulator (sw-em24)*. End-user
.plsw + .msw source is fed to that running compiler over UART using a
custom FILE:/SOURCE: protocol; the compiler's stdout (also UART) emits
COR24 .s assembly, which is captured to disk and re-run through the
emulator to execute the user program. This repo therefore exercises a
two-stage emulator pipeline and pins three vendored toolchains
(sw-cx24, sw-asx24, sw-em24) at v0.1.0 each.

## 1. Run scripts

The justfile (justfile:1-46) is the primary entry point. It defines:
- `build` (justfile:9-10): `tc24r src/main.c -o build/plsw.s -I $HOME/github/sw-vibe-coding/tc24r/include -I src` -- the compiler still calls the system `tc24r` directly, not the vendored sw-cx24.
- `run` (justfile:13-14), `run-input` (justfile:17-18), `test` (justfile:21-22): launch `cor24-run --run build/plsw.s` interactively or with cycle limits.
- `pipeline <args>` (justfile:27-28) -> scripts/pipeline.sh
- `pipeline-dump <args>` (justfile:32-33) -> scripts/pipeline-dump.sh
- `hello-macro` (justfile:36-37): pipeline-dump examples/greet.msw examples/hello_macro.plsw
- `chain` (justfile:40-41): pipeline-dump include/{cvt,ascb,asxb,tcb}.msw examples/chain.plsw
- `clean` (justfile:44-45): `rm -f build/*.s build/*-combined.plsw build/*-dump.txt`

scripts/ contents (3 files):
- pipeline.sh (113 lines): build+run, capture UART, no dump
- pipeline-dump.sh (126 lines): build+run with `--dump`, save .s, dump.txt, -combined.plsw
- vendor-fetch.sh (363 lines): verify/--record vendored binaries against version.json sha256

There is no top-level demo.sh.

## 2. Memory loads

There is no static memory map file. The runtime layout is implicit and
documented in docs/architecture.md:104-135:

| Region        | Range                | Purpose                          |
|---------------|----------------------|----------------------------------|
| .text + .data | 0x000000-0x0FFFFF    | 1 MB SRAM, code then static data |
| (unmapped)    | 0x100000-0xFEDDFF    | 14.9 MB hole                     |
| Stack (EBR)   | 0xFEE000-0xFEFFFF    | 8 KB, grows down from 0xFEEC00   |
| LED           | 0xFF0000             | active-low MMIO                  |
| UART data     | 0xFF0100             | TX/RX                            |
| UART status   | 0xFF0101             | bit7=TX-ready, bit0=RX-ready     |

cor24-run is invoked with no `--load` arguments; the assembler resolves
all addresses at assemble time. Every observed run shows initial sp =
0xFEEC00 (e.g. build/hello-dump.txt:35, build/chain-dump.txt:14).

## 3. Patches

No binary patches, overlays, or post-link rewrites. Every artifact is
produced from source through assembly only. The `pipeline-dump.sh:62-74`
"-combined.plsw" output is just a concatenation with realpath comments
for diagnostic readability -- not a patch.

## 4. UART payload structure

The compiler reads its source program over UART using a small framing
protocol implemented in pipeline.sh:43-62 and pipeline-dump.sh:42-58:

```
'c\n'                               <-- compile command
[ optional, when .msw files given:
  for each macro file m:
    'FILE:<basename(m)>\n'
    <file contents, lines joined with '\n'>
    '\x1E'                          <-- record separator = end of file
  'SOURCE:\n'                       <-- begin main translation unit
]
<.plsw contents, lines joined with '\n'>
'\x04'                              <-- EOT, end of input
```

The compiler echoes the assembly inside two literal markers
`--- generated assembly ---` ... `--- end assembly ---`
(pipeline.sh:79-81); the host extracts everything between them.
Compile budget: -n 200000000, -t 120s. Run budget: -n 50000000,
-t 30s (pipeline-dump.sh:78,112).

## 5. Heaps and stacks

- Stack: 8 KB EBR at 0xFEE000-0xFEFFFF, sp init 0xFEEC00. Calling
  convention pushes fp/r2/r1 and sets fp=sp on entry; first arg at
  fp+9 (CLAUDE.md:168-171, build/hello.s:11-15).
- Heap: none in the runtime. Programs that need dynamic memory roll
  their own bump arena -- e.g. examples/chain.plsw has
  `DCL ARENA(512) BYTE` + `ARENA_POS` and an `ALLOC` PROC
  (build/chain-combined.plsw:71-79).
- Compiler internals: AST node pool capped at 256, ~64 symbols/scope
  (docs/architecture.md:194-199).

## 6. Build artifacts vs vendored

Compile pipeline produces, per source program (under build/):

| Artifact                     | Source             | Producer                  |
|------------------------------|--------------------|---------------------------|
| `<name>-combined.plsw`       | concatenation      | pipeline-dump.sh:62-74    |
| `<name>.s` (or `out.s`)      | UART output of compiler running on emulator | pipeline.sh:79-93, pipeline-dump.sh:88-103 |
| `<name>-dump.txt`            | second emulator run with `--dump` | pipeline-dump.sh:111-115 |
| `plsw.s` (compiler itself)   | tc24r src/main.c   | justfile:9-10             |

Observed in build/ at survey time: hello, hello_macro, chain, define
each have the full triple; out.s and run-dump.txt are the
last-run scratch outputs from `pipeline-dump.sh`. plsw.s is 356,547
lines (the C-compiled compiler).

Vendored toolchains (vendor/active.env:9-11 pins all three at v0.1.0):

| Tool      | Version | Provides                       | version.json |
|-----------|---------|--------------------------------|--------------|
| sw-cx24   | v0.1.0  | C cross-compiler (formerly tc24r) | vendor/sw-cx24/v0.1.0/version.json |
| sw-asx24  | v0.1.0  | COR24 cross-assembler (assembler half of cor24-run) | vendor/sw-asx24/v0.1.0/version.json |
| sw-em24   | v0.1.0  | COR24 emulator (runtime half of cor24-run) | vendor/sw-em24/v0.1.0/version.json |

All three manifests have repo/commit/build_cmd/binary_src/sha256
recorded as `"TBD"` (vendor/sw-cx24/v0.1.0/version.json:6-19 and
peers). `bin/` directories exist but are empty -- vendor-fetch.sh has
not yet been pointed at materialized upstreams. The justfile and shell
scripts still reach for system `tc24r` and `cor24-run`; the vendored
tree is staged for cutover but not wired in. docs/vendor-plan.md:30-40
notes the future split: cor24-run is being decomposed into sw-asx24 +
sw-em24, and tc24r becomes sw-cx24.

## 7. Known limits

- Single translation unit; no separate compilation
  (docs/architecture.md:194-195).
- 3 GP registers (r0/r1/r2) drive aggressive spilling
  (docs/architecture.md:196).
- AST pool 256 nodes, ~64 symbols/scope
  (docs/architecture.md:198-199).
- No floats, no FPU.
- 1 MB SRAM hard cap; 8 KB stack hard cap; ~14.9 MB unmapped hole.
- Compiler is bottlenecked by emulator throughput: 200M-instruction
  cap, 120s wall (pipeline.sh:67); chain.plsw uses 316 instructions
  to run but the compile run consumes vastly more.
- vendored toolchains are placeholders: TBD commit/sha and empty
  bin/ dirs (vendor/sw-*/v0.1.0/version.json all line ranges 6-19).

## Schema gaps for this repo

A sw-launcher schema must capture:

1. Two-stage emulator invocation: stage A runs the compiler image
   (build/plsw.s) on sw-em24 with a UART input payload; stage B runs
   the *captured* stdout-as-assembly on sw-em24 again. Most schemas
   assume one program per launch.
2. UART input is structured: a leading `'c\n'` command, optional
   FILE:/SOURCE: framing with `\x1E` record separators, trailing
   `\x04` EOT. The schema needs first-class "stdin payload composed
   from N input files plus framing literals" support.
3. Output is parsed out of UART by line markers
   (`--- generated assembly ---` ... `--- end assembly ---`); schema
   must express "extract region between markers, write to file".
4. Multiple budgets per stage: `-n` instruction count and `-t`
   wall-clock seconds differ between compile (200M / 120s) and run
   (10M-50M / 30s).
5. THREE coexisting vendored toolchains under vendor/<tool>/<version>
   with a single active.env pinning each version, and a
   version.json carrying repo/commit/build_cmd/binary_src/sha256 per
   platform. Today the justfile still calls system tc24r/cor24-run
   instead of the vendored binaries -- the launcher schema should
   distinguish "declared vendor pin" from "actually-invoked binary".
6. Multi-stage compile artifacts: `-combined.plsw` (preprocessor-like
   concatenation), `.s` (transpiler output), `-dump.txt` (emulator
   memory dump). Naming derives from the .plsw basename, but the
   non-dump pipeline writes to a literal `out.s` instead.
7. No memory-map file and no patches; layout is hard-coded in the
   emulator/assembler. Schema should allow an empty "loads" /
   "patches" set rather than requiring them.
8. Heaps are user-space (in-program arenas), not part of the runtime;
   the schema should not assume a system-provided malloc region.
9. Macro/include set is positional in the script invocation but
   resolved by basename via FILE:/SOURCE: -- the launcher needs to
   model "ordered include list with logical-name lookup", not just a
   `-I` directory list.
