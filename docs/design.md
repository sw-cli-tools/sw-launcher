# sw-launcher Design

This document fixes the concrete shape of `sw-launch.toml`,
`sw-launch.lock`, the cache key formula, the load plan, and the three
primitive scenario shapes the tool must support on day one.

## Naming and locations

- Repo: `sw-cli-tools/sw-launcher`
- Binary: `sw-launch`
- Per-project config: `sw-launch.toml`
- Per-project lockfile: `sw-launch.lock`
- Per-project local state: `.sw-launch/`
- User cache: `~/.cache/sw-launch/`
- User vendor store: `~/.local/share/sw-launch/`

## Memory layout: the three scenario shapes

The tool must express, at minimum, three primitive shapes. All
addresses below are illustrative -- real values come from the TOML.

### Scenario A -- "assembled program at zero, UART data"

The simplest case. A single hand-written `.s` is assembled and loaded
at address 0; runtime data arrives via UART.

```
COR24 address space
+----------------------------+ 0x000000  <-- entry
| program.bin (assembled .s) |
|                            |
+----------------------------+
| .... unused ....           |
+----------------------------+ 0xFEEC00  stack pointer init
| stack                      |
+----------------------------+ 0xFF0000  LED + switch MMIO
| device registers           |
+----------------------------+

UART input  : "abc!"   (--uart-input)
UART output : captured for expectations
```

TOML sketch:

```toml
[scenarios.echo]
target = "cor24"
layers = ["echo_program", "stdin_data"]
entry  = "0x000000"

[scenarios.echo.run]
timeout_ms = 2000
max_cycles = 200_000
halt_on    = "uart-eot"

[scenarios.echo.expect]
uart_contains = ["abc!"]
exit_code     = 0

[layers.echo_program]
kind     = "assembler"
source   = "local"
input    = "src/echo.s"
tool     = "assembler"
artifact = "echo.bin"
load.method  = "memory"
load.address = "0x000000"

[layers.stdin_data]
kind   = "data"
source = "local"
input  = "tests/echo-input.txt"
load.method    = "uart"
load.max_bytes = 1024
```

### Scenario B -- "runtime + binary blob, with patch"

A COR24 binary loaded at 0 acts as a runtime/VM (e.g. `pvm.s`
assembled to `pvm.bin`). A higher-level binary (e.g. a `.p24` p-code
image) is loaded at a higher address. The runtime needs to know where
the bytecode lives, so a single word in the runtime's data segment
gets patched at load time.

```
+-----------------------------------+ 0x000000  <-- entry (pvm)
| pvm.bin (p-code VM, COR24 native) |
|   ...                             |
|   code_ptr: 0x??????   <-- patched to 0x010000 by --patch
|   ...                             |
+-----------------------------------+ 0x010000
| pcode_app.p24                     |
+-----------------------------------+ 0x020000
| .... unused ....                  |
+-----------------------------------+ 0xFEEC00  stack
+-----------------------------------+ 0xFF0000  MMIO
+-----------------------------------+
```

TOML sketch:

```toml
[scenarios.pcode-hello]
target = "cor24"
layers = ["pcode_vm", "pcode_app"]
entry  = "0x000000"

[scenarios.pcode-hello.run]
timeout_ms = 5000
max_cycles = 5_000_000
halt_on    = "monitor-exit"

[scenarios.pcode-hello.expect]
uart_contains = ["HELLO"]
exit_code     = 0

[layers.pcode_vm]
kind     = "assembler"
source   = "vendor:sw-pcode@v0.1.0"
input    = "vendor/sw-pcode/v0.1.0/bin/pvm.s"
tool     = "assembler"
artifact = "pvm.bin"
load.method  = "memory"
load.address = "0x000000"
exports.symbol = "code_ptr"      # listing-extracted address

[layers.pcode_app]
kind     = "pcode"
source   = "local"
input    = "examples/hello.spc"
tool     = "pcode_assembler"
artifact = "hello.p24"
load.method  = "memory"
load.address = "0x010000"
patches = [
  { target = "pcode_vm.code_ptr", value = "0x010000" }
]
```

The `exports.symbol` mechanism tells `sw-launch` to grep the layer's
listing for `code_ptr:` and remember its address; `patches` then
expand to `--patch <addr>=<value>` flags for `cor24-run`.

### Memory layout for Scenarios B and C, with stacks and heaps

Scenario B in practice looks like this once segments are honored:

```
+----------------------------------------+ 0x000000  <-- entry
| pvm.bin code segment                   |  ~16 KiB
+----------------------------------------+ ~0x004000
| pvm.bin data + embedded segments       |
|   eval_stack  (1 KiB, embedded)        |
|   call_stack  (2 KiB, embedded)        |
|   heap_seg    (8 KiB, embedded)        |
+----------------------------------------+ 0x010000
| pcode app .p24                         |
+----------------------------------------+
| ... free SRAM ...                      |
+----------------------------------------+ 0xFEEC00  EBR (HW stack, do not touch)
+----------------------------------------+ 0xFF0000  MMIO
```

Scenario C adds an interpreter with its own non-embedded heap and
stacks, *patched* into runtime symbols:

```
+----------------------------------------+ 0x000000  <-- entry
| pvm.bin (with embedded VM-internal     |
|   eval/call stacks + small heap)       |
+----------------------------------------+ 0x010000
| ocaml.p24m                             |
+----------------------------------------+ 0x080000
| ocaml interp value heap (256 KiB)      |  <- segment, patched into
|                                        |     ocaml_interp.heap_base / _end
+----------------------------------------+ 0x0C0000
| ocaml interp eval stack (32 KiB)       |  <- patched into esp_init
+----------------------------------------+ 0x0E0000
| dsl_app.heap (64 KiB)                  |  <- patched into dsl_app.heap_base
+----------------------------------------+ 0x0F0000  free
+----------------------------------------+ 0xFEEC00  EBR
+----------------------------------------+ 0xFF0000  MMIO
```

The validator must understand both shapes, count both kinds of
segments toward the SRAM budget, and reject the most common failure
mode: an interpreter heap colliding with a DSL heap because both
upper layers were authored against an older budget.

### Scenario C -- "nested interpreter with source + data"

A COR24 runtime hosts a p-code app that is itself an interpreter
(e.g. `ocaml.p24m` -- the OCaml interpreter compiled from Pascal to
p-code). That interpreter consumes source code (e.g. `demo.ml`) and
runtime data (e.g. UART chars).

```
+----------------------------------------+ 0x000000  <-- entry
| pvm.bin (COR24 native p-code VM)       |
|   code_ptr -> 0x010000 (patched)       |
+----------------------------------------+ 0x010000
| ocaml.p24m (Pascal -> p-code OCaml)    |
+----------------------------------------+ 0x020000  ... varies
| optional pre-loaded source/data        |
+----------------------------------------+ 0xFEEC00  stack
+----------------------------------------+ 0xFF0000  MMIO
+----------------------------------------+

UART  : OCaml source + EOT + runtime data  (--uart-input)
```

TOML sketch:

```toml
[scenarios.ocaml-newlang-demo]
target = "cor24"
layers = ["pcode_vm", "ocaml_interp", "ocaml_source", "demo_input"]
entry  = "0x000000"

[scenarios.ocaml-newlang-demo.run]
timeout_ms = 60_000
max_cycles = 3_000_000_000
halt_on    = "monitor-exit"

[scenarios.ocaml-newlang-demo.expect]
uart_contains = ["NEWLANG READY", "demo complete"]
exit_code     = 0

[layers.pcode_vm]      # same as scenario B
# ...

[layers.ocaml_interp]
kind     = "pcode-image"
source   = "vendor:sw-cor24-ocaml@v0.1.0"
input    = "vendor/sw-cor24-ocaml/v0.1.0/bin/ocaml.p24m"
artifact = "ocaml.p24m"
load.method  = "memory"
load.address = "0x010000"
patches = [
  { target = "pcode_vm.code_ptr", value = "0x010000" }
]

[layers.ocaml_source]
kind   = "text"
source = "local"
input  = "examples/newlang.ml"
load.method     = "uart"
load.terminator = "EOT"
load.max_bytes  = 16_384

[layers.demo_input]
kind   = "text"
source = "local"
input  = "examples/demo-in.txt"
load.method     = "uart"
load.terminator = "none"   # appended after EOT
load.max_bytes  = 4_096
```

This is the shape we observe in
`sw-cor24-ocaml/scripts/run-ocaml.sh`: an environment variable
`OCAML_STDIN` is appended to the UART input after the source's EOT
terminator. `sw-launch` makes that explicit and ordered.

## Memory segments per layer

A layer is not just a code blob at an address. Each layer is a composite
of segments, any of which may be:

- **embedded in the artifact** (e.g. `pvm.bin` ships with statically
  reserved `eval_stack`, `call_stack`, and `heap_seg` regions in its
  own data segment), or
- **reserved by the loader** at a configured address with a configured
  size (e.g. an OCaml interpreter that needs 256 KB of heap above
  whatever the VM already reserved), or
- **patched into the runtime** so that it knows where to find its
  stack pointer / heap top / call-frame base.

The COR24 hardware stack lives in 3 KB EBR at `0xFEEC00..0xFEF7FF`. That
is large enough for `pvm.s` itself to use sparingly via `sp`, but
*nothing* substantial -- not the p-code VM's eval stack, not an OCaml
interpreter's heap, not a DSL on top of OCaml -- fits in 3 KB. Real
applications use *high-SRAM* memory regions for their working stacks
and heaps, configured per layer and patched in at load time.

The schema models this with a `segments` table per layer:

```toml
[layers.pcode_vm]
kind     = "assembler"
source   = "vendor:sw-pcode@v0.1.0"
input    = "vendor/sw-pcode/v0.1.0/bin/pvm.s"
tool     = "assembler"
artifact = "pvm.bin"
load.method  = "memory"
load.address = "0x000000"
exports.symbols = ["code_ptr", "esp_init", "csp_init", "hp_init"]

# Segments embedded inside pvm.bin (declared so overlap-check sees them)
[layers.pcode_vm.segments.code]
kind   = "code"
size   = "auto"                 # learned from artifact length

[layers.pcode_vm.segments.data]
kind   = "data"
size   = "auto"

# pvm.s reserves these at fixed offsets inside its image. The loader
# does NOT need to allocate space for them -- they are part of the
# artifact -- but it does need to know they exist for overlap checks.
[layers.pcode_vm.segments.eval_stack]
kind     = "stack"
embedded = true
symbol   = "eval_stack"         # resolved from listing
size     = "0x000400"           # 1 KiB (matches pvm.s)

[layers.pcode_vm.segments.call_stack]
kind     = "stack"
embedded = true
symbol   = "call_stack"
size     = "0x000800"

[layers.pcode_vm.segments.heap]
kind     = "heap"
embedded = true
symbol   = "heap_seg"
size     = "0x002000"

# An interpreter layered on top -- its heap and stacks live in
# *separately reserved* high-SRAM regions, and the runtime gets
# patched to point at them.
[layers.ocaml_interp.segments.value_heap]
kind          = "heap"
embedded      = false
load.address  = "0x080000"
size          = "0x040000"      # 256 KiB OCaml value heap
patches = [
  { target = "ocaml_interp.heap_base", value = "self.address" },
  { target = "ocaml_interp.heap_end",  value = "self.end" },
]

[layers.ocaml_interp.segments.eval_stack]
kind          = "stack"
embedded      = false
load.address  = "0x0C0000"
size          = "0x008000"      # 32 KiB
grows         = "up"
patches = [
  { target = "ocaml_interp.esp_init", value = "self.address" },
]

# An app on top of the interpreter (e.g. a DSL) can declare its own.
[layers.dsl_app.segments.dsl_heap]
kind          = "heap"
embedded      = false
load.address  = "0x0E0000"
size          = "0x010000"
patches = [
  { target = "dsl_app.heap_base", value = "self.address" },
]
```

Segment kinds the validator recognizes:

| kind    | meaning                                    | typical patches |
|---------|--------------------------------------------|----------------|
| `code`  | executable bytes; immutable at load time   | none           |
| `data`  | static read-only or initialized data       | none           |
| `bss`   | zero-initialized; reserved, not loaded     | none           |
| `heap`  | dynamic allocation region                  | `heap_base`, `heap_end`, or `heap_limit` |
| `stack` | call/eval stack region                     | `sp_init` or named symbol |

Heaps may grow up (`grows = "up"`, default) or down (`grows = "down"`).
A down-growing heap typically only patches a single `heap_limit`
symbol, not `heap_base` -- this is how the OCaml interpreter +
tuplet stack is wired today (`heap_limit = 0x03F000` immediately
below `ocaml.p24m@0x040000`). The schema accepts either pattern:

```toml
[layers.ocaml_interp.segments.value_heap]
kind          = "heap"
embedded      = false
grows         = "down"
load.address  = "0x010000"      # bottom of heap
size          = "0x02F000"      # ends at 0x03F000
patches = [
  { target = "ocaml_interp.heap_limit", value = "self.end" },
]
```
| `mmio`  | device aperture (declared, not loaded)     | none           |

Whether a segment is `embedded = true` or not changes how it is
counted:

- **Embedded** segments live inside the layer's artifact. The loader
  doesn't allocate them again, but the validator records their absolute
  range (`layer.load.address + symbol_offset`) for the global
  overlap check.
- **Non-embedded** segments require their own `load.address` and
  `size`, occupy their own range, and may declare `patches` that point
  the runtime at them. They are never written to a file -- the loader
  reserves the range and zero-fills if `kind = "bss" | "stack" |
  "heap"` and `init = "zero"`.

The shorthand `value = "self.address"` / `"self.end"` /
`"self.size"` lets a segment patch a runtime symbol with its own
address-derived value without duplicating the literal hex.

## Memory budgets per target

```toml
[targets.cor24]
kind         = "emulator"
word_bits    = 24
address_bits = 24
endian       = "big"
loader       = "cor24-memory-map"

[targets.cor24.regions]
sram     = { start = "0x000000", end = "0x0FFFFF" }
ebr_stack = { start = "0xFEEC00", end = "0xFEF7FF", role = "hw-stack" }
mmio     = { start = "0xFF0000", end = "0xFFFFFF" }
```

`validate` enforces:

- Every memory-loaded byte and every reserved segment falls inside
  `regions.sram`.
- Nothing user-loaded touches `regions.ebr_stack` (the COR24 hardware
  stack is reserved) or `regions.mmio`.
- Reserved stack/heap segments never overlap each other or any code/
  data segment from any layer.
- Total reserved memory <= `regions.sram.end - regions.sram.start + 1`.

This is what catches the "I forgot the OCaml heap collides with the
DSL heap" class of bugs at `check` time, before the emulator even
starts.

## Layer kinds

| kind          | input(s)              | tool             | artifact      | typical load |
|---------------|-----------------------|------------------|---------------|--------------|
| `assembler`   | `.s`                  | `assembler`      | `.bin`        | memory       |
| `binary`      | `.bin`                | (none, copy)     | `.bin`        | memory       |
| `pcode`       | `.spc`                | `pcode_assembler`| `.p24`        | memory       |
| `pcode-image` | `.p24m` (pre-linked)  | (none, copy)     | `.p24m`       | memory       |
| `text`        | UTF-8 text            | (none)           | bytes         | uart         |
| `data`        | bytes                 | (none)           | bytes         | uart, memory |

## Load methods

```toml
load.method = "memory"
load.address = "0x010000"   # required
```

```toml
load.method = "uart"
load.max_bytes = 4096       # required, hard cap
load.terminator = "EOT" | "none"
```

Future methods (declared but unimplemented in MVP): `card`, `disk`,
`flash`, `serial-loader`.

## Patches

```toml
patches = [
  { target = "pcode_vm.code_ptr", value = "0x010000" },
  { target = "0x000A12",          value = "0x000042" },
]
```

`target` is either `<layer>.<symbol>` (resolved by reading the layer's
listing/map file) or a literal hex address. `value` is hex.

## Validation rules

The `validate` module enforces (each diagnostic has a stable error
code; numbering is final once assigned):

1. (E0001) Every scenario lists at least one layer.
2. (E0002) Every layer referenced by a scenario exists.
3. (E0003) No two memory ranges overlap. This includes embedded
   segments resolved through layer listings and non-embedded
   reserved segments. The check runs across *every* segment of
   *every* layer in the scenario.
4. (E0004) Every UART layer has `max_bytes` and the input file is no
   larger than that.
5. (E0005) Layer kind and load method are compatible (e.g. `pcode`
   cannot `load.method = "uart"`).
6. (E0006) Every patch resolves. Symbolic targets must come from a
   layer that has an `exports.symbols` entry, the segment must exist,
   and the symbol must be earlier in topo order than the patcher.
7. (E0007) The layer DAG is acyclic.
8. (E0008) Every scenario has `run.halt_on` or `run.max_cycles`
   (preferably both).
9. (E0009) Every tool referenced by a layer exists in `[tools.*]`.
10. (E0010) `sw-launch.lock` is up to date if it exists; otherwise
    `--update-lock` is required.
11. (E0011) Every reserved segment (`stack`, `heap`, `bss`) lies
    entirely within `targets.<t>.regions.sram` and never touches
    `regions.ebr_stack` or `regions.mmio`.
12. (E0012) Sum of all reserved + loaded bytes does not exceed
    `regions.sram` size.
13. (E0013) Every `stack`/`heap` segment has a non-zero `size`.
14. (E0014) `embedded = true` segments must declare a `symbol`
    that resolves in the producing layer's listing.
15. (E0015) `embedded = false` segments must declare both
    `load.address` and `size`.
16. (E0016) Patches with `value = "self.address" | "self.end" |
    "self.size"` are only valid inside a segment block.

## Cache key

For each layer:

```
layer_key = sha256(
    schema_version
  | normalize_toml(layer_config)
  | hash_each(input_files)
  | tool_version_hash
  | dependency_layer_hashes (in topo order)
  | load_address_or_uart_marker
)
```

`tool_version_hash` is the SHA256 of the tool binary's `--version`
output combined with the binary's own SHA. We do not run tools to
compute keys -- we run `--version` once per session and cache it in
memory.

`normalize_toml` is a deterministic re-serialization (sorted keys,
canonical numeric form) so trivial reformatting doesn't bust the
cache.

## Lockfile format

```toml
schema_version = 1
generated_at   = "2026-04-28T15:14:00Z"

[[locked]]
package = "sw-pcode"
version = "v0.1.0"
source  = { kind = "local", path = "../sw-cor24-pcode" }
commit  = "cd8a6a7a634b7c76198f1f70d3ff11e8550db492"
artifacts = [
  { name = "pvm.s",  sha256 = "..." },
  { name = "pa24r",  sha256 = "..." },
  { name = "pl24r",  sha256 = "..." },
]

[[locked]]
package = "sw-cor24-ocaml"
version = "v0.1.0"
source  = { kind = "local", path = "../sw-cor24-ocaml" }
commit  = "..."
artifacts = [
  { name = "ocaml.p24m", sha256 = "..." },
]
```

`sw-launch vendor sync` writes this. `sw-launch run` refuses to
proceed if any TOML-declared dependency is missing from the lockfile
or has a different commit, unless `--update-lock` is set.

## Expectations

```toml
[scenarios.<name>.expect]
uart_contains    = ["expected substring", ...]
uart_regex       = ["^READY$", ...]
uart_not_contains = ["TRAP", "FAIL"]
exit_code        = 0
stdout_lines_eq  = ["one", "two"]    # exact line-by-line match
```

Run the full match set; report each failure with the captured value
side-by-side with the expectation. Successful runs may also emit a
JSON report at `--report-json <path>` for machine consumption.

## Profiles

```toml
[profiles.demo]
speed         = 0
terminal      = true
trace_loads   = false

[profiles.test]
speed         = 0
terminal      = false
trace_loads   = true
fail_fast     = true
```

`sw-launch run --profile test <scenario>` overrides any
scenario-level run config with the profile's. Default profile is
`demo`.

## CLI surface (final shape)

```
sw-launch <subcommand> [flags] [scenario]

  run     <scenario>                 Build (with cache) and execute, check expectations.
  build   <scenario>                 Build all layers; do not execute.
  check   <scenario>                 Validate config + lock; no tools run.
  graph   <scenario>                 Print layer DAG (text or --json).
  cache   list                       List cached artifacts.
  cache   explain <scenario>         Show cache hit/miss for each layer.
  cache   clean [--scenario <name>]  Drop cache entries.
  vendor  sync                       Resolve and pin all dependencies.
  vendor  status                     Compare TOML vs lock vs vendor store.
  doctor                             Verify host tools (cor24-run, pa24r, pl24r) found.

Flags:
  -c, --config <path>          Default: ./sw-launch.toml
      --profile <name>         Default: demo
      --no-cache               Force rebuild
      --rebuild <layer>        Rebuild a specific layer (cache other layers)
      --update-lock            Allow lockfile mutation
      --explain                Verbose action plan
      --dry-run                Print plan without invoking tools or emulator
      --trace-loads            Log each --load-binary / --uart-input
      --report-json <path>     Write structured run report
      --timeout <ms>           Override scenario timeout
```

## Error code stability

Every diagnostic uses a stable identifier (`E0001`..`E00NN`). New
errors get new numbers; existing numbers never change meaning. Agents
can match on these without scraping prose.

## Test strategy summary

- Unit tests live next to the module under test in `#[cfg(test)] mod
  tests`.
- Integration tests under `tests/` exercise full scenarios using a
  recording fake for the assembler/p-code-asm and either a recording
  fake or the real `cor24-run` for the emulator.
- Scenario A/B/C each get a dedicated integration test file with a
  golden TOML, fixture inputs, and a snapshot of the load plan.
- Validator rules each get a negative test that asserts the exact
  error code and span.
- Cache determinism test: run twice, second run must be 100% hits.
- `cargo clippy --all-targets --all-features -- -D warnings`,
  `cargo fmt --check`, `cargo test`, and `sw-checklist` are mandatory
  pre-commit gates. Each agentrail step ends with the gate green.
