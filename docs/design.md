# sw-launcher Design

This document fixes the concrete shape of `sw-launch.toml`,
`sw-launch.lock`, the cache key formula, the load plan, and the
five primitive scenario shapes the tool must support on day one.

This is **schema v1.2**, revised after `docs/memory-stance.md`
and `docs/heap-analysis.md`. v1.0 treated layers as opaque
artifacts at absolute addresses. v1.1 introduced a *fixed* 8 x
128 KiB partition grid. v1.2 replaces the fixed grid with
**named memory profiles** so each use case (compiled-app,
interpreter-only, repl-inline-compile, compiler-image,
resident-shell) declares its own partition shape, with
per-profile heap and stack budgets.

The v1.2 stance: 1 MiB of COR24 SRAM is luxurious by historical
standards (1980s machines did real work in 4-256 KiB), so any
heap > 32 KiB is suspicious until justified, and the schema
makes the justification explicit (`heap_justification` block).

Every schema change is justified by a specific entry in
`docs/survey/schema-gaps.md` or by `docs/heap-analysis.md`; gap
IDs and analysis-section references are cited inline so the
trace from observation to design is auditable.

## Naming and locations

- Repo: `sw-cli-tools/sw-launcher`
- Binary: `sw-launch`
- Per-project config: `sw-launch.toml`
- Per-project lockfile: `sw-launch.lock`
- Per-project local state: `.sw-launch/`
- User cache: `~/.cache/sw-launch/`
- User vendor store: `~/.local/share/sw-launch/`

## Addressing model: memory profiles

(v1.2; replaces v1.1's fixed grid. See `docs/memory-stance.md`
and `docs/heap-analysis.md`.)

Each scenario picks a **named memory profile** that declares the
partition shape suited to its workload. Profiles encode the
working hypothesis from `docs/heap-analysis.md`:

- A **compiled-app** running an interpreter has different needs
  than a **REPL with in-line compilation**.
- A **resident-shell** scenario carves SRAM into many small
  program slots.
- A **compiler-image** scenario gives most of SRAM to a single
  large image with a small heap on the side.
- An **interpreter-only** scenario is the middle ground.

A profile is *not* a fixed grid. It is an ordered list of
partitions, each with its own size and its own list of named
regions; partitions can have any number of regions of any size,
constrained only by the partition's size and the target's SRAM
bounds.

### Required default profiles

The schema reserves five profile names that ship with the
launcher and have well-known semantics. A consuming repo can
override their `partitions` and `budget` blocks but cannot
rename them.

| profile name           | description                                                |
|------------------------|-----------------------------------------------------------|
| `compiled-app`         | Single image at 0; small heap; small stack.               |
| `interpreter-only`     | Runtime + interpreted source; modest heap.                |
| `repl-inline-compile`  | Runtime + interpreter + compiler in image; large heap.    |
| `compiler-image`       | Monolithic compiler-as-COR24-program (plsw); large code.  |
| `resident-shell`       | Monitor + shell + N program slots; multiple small images. |

Each ships with default partition shapes and budgets sized per
the analysis in `docs/heap-analysis.md`.

### Profile syntax

```toml
[memory_profiles.compiled-app]
description = "Single image at 0; small heap; small stack."

[[memory_profiles.compiled-app.partitions]]
name = "code"
base = "0x000000"
size = "0x010000"            # 64 KiB
regions = [
  { name = "code",   kind = "code",  size = "auto" },
  { name = "static", kind = "data",  size = "auto" },
]

[[memory_profiles.compiled-app.partitions]]
name = "heap"
base = "0x010000"
size = "0x008000"            # 32 KiB
regions = [
  { name = "heap", kind = "heap", size = "0x008000" },
]

[[memory_profiles.compiled-app.partitions]]
name = "stack"
base = "0x018000"
size = "0x002000"            # 8 KiB
regions = [
  { name = "stack", kind = "stack", size = "0x002000" },
]

[memory_profiles.compiled-app.budget]
code_max  = "0x008000"       # 32 KiB
heap_max  = "0x004000"       # 16 KiB
stack_max = "0x002000"       # 8 KiB
total_max = "0x010000"       # 64 KiB
justification_required = true
```

### Default profile budgets (from heap-analysis.md)

| profile               | code+data | heap     | stack   | total    |
|-----------------------|-----------|----------|---------|----------|
| `compiled-app`        | <= 32 KiB | <= 16 KiB| <= 8 KiB| <= 64 KiB|
| `interpreter-only`    | <= 64 KiB | <= 64 KiB| <= 16 KiB| <= 160 KiB |
| `repl-inline-compile` | <= 128 KiB| <= 256 KiB| <= 32 KiB| <= 448 KiB |
| `compiler-image`      | <= 256 KiB| <= 64 KiB| <= 32 KiB| <= 384 KiB |
| `resident-shell`      | <= 64 KiB per slot, up to 8 slots | per-program | shared 8 KiB | <= 512 KiB total |

### How a scenario picks a profile

```toml
[scenarios.apl-batch]
target = "cor24"
memory_profile = "interpreter-only"
layers = ["apl_interp", "apl_source"]
entry  = "0x000000"
```

A scenario's `memory_profile` is a hard contract. Every layer in
the scenario must claim within the named profile's partitions
unless the layer explicitly opts out via `absolute_addresses =
true`.

### How a layer claims regions

Layers cite the *profile's* partitions and regions by name, not
by absolute address:

```toml
[layers.apl_interp.segments.code]
kind   = "code"
claims = [
  { partition = "code", region = "code" },
]

[layers.apl_interp.segments.heap]
kind   = "heap"
claims = [
  { partition = "heap", region = "heap" },
]
```

The launcher resolves names to absolute ranges using the
scenario's selected profile. Renaming a partition in the
profile renames it everywhere consistently.

### Multi-heap and multi-stack profiles

A profile may declare multiple `kind = "heap"` regions of
different sizes, and likewise multiple stacks. Useful when:

- A scenario has a runtime heap *and* a separate program heap
  (e.g., pvm's internal heap_seg plus the OCaml interpreter
  heap above it).
- A scenario has both a call stack (large) and an eval stack
  (small) -- the p-code VM's standard configuration.

Layers reference each by `(partition, region)` name; the
validator confirms each named region exists and matches `kind`.

### Heap justification (required for > 32 KiB heap)

Any layer claiming a heap region above 32 KiB total must declare
a `heap_justification` block:

```toml
[layers.ocaml_interp.heap_justification]
category = "gc-slack"
note     = "Mark/sweep GC; sized for working set + 2x slack."
measured_floor_kib = 64
tracking_issue     = "sw-cor24-ocaml#28"
```

Categories (from `docs/memory-stance.md`):

| category             | accepted? | meaning                                              |
|----------------------|-----------|------------------------------------------------------|
| `algorithmic-floor`  | yes       | Working set genuinely requires this size.            |
| `bytecode-image`     | yes       | Heap is mostly read-only data, not allocations.      |
| `gc-slack`           | yes (with measured_floor_kib) | Sized for floor + slack between collections. |
| `dead-leak`          | warn; rejected by `--strict` | Allocations that never get freed. |
| `algorithmic-bloat`  | warn; rejected by `--strict` | Pointer width, boxing, dispatch tables, etc. |

Without a `heap_justification` block, claims > 32 KiB are E0030.

### Absolute-address opt-out

A layer that doesn't fit any profile (plsw's monolithic compiler
is ~1 MiB; macrolisp's mid-partition slot addresses) opts out:

```toml
[layers.plsw_compiler]
kind = "binary"
absolute_addresses = true
load.method  = "memory"
load.address = "0x000000"
size         = "0x100000"
acknowledge_oversized = true
```

`absolute_addresses = true` means: ignore the profile for this
layer; the loader places the artifact at `load.address` and the
validator checks only SRAM / EBR / MMIO bounds plus per-byte
overlap with other layers. `acknowledge_oversized = true` is
required when the layer's reservation pushes the scenario above
the profile's `total_max` *or* above the 1 MiB rule of thumb.

### Adjacent EBR + MMIO

The COR24 hardware stack at `0xFEEC00..0xFEF7FF` and MMIO at
`0xFF0000..0xFFFFFF` are not part of any profile. They are
declared in `[targets.cor24.regions]` and treated as off-limits.

### Why named profiles instead of a fixed grid

Three reasons, each grounded in the analysis:

- **A REPL needs more heap than a compiled app.** A fixed
  32 KiB heap region forces unrelated workloads to share a
  budget. v1.2 lets each profile size its heap to its
  documented purpose.
- **Bloat is debt, not a constant.** The historical record
  (per `docs/memory-stance.md`) says these languages fit in
  4-256 KiB total in the 1980s. v1.2 budgets are tight enough
  to flag bloat; v1.1's 32 KiB heap regions were tight by
  accident.
- **Per-profile validation is more useful.** The diagnostic
  "scenario `ocaml-newlang-demo` heap claims 252 KiB exceeds
  `repl-inline-compile.budget.heap_max = 256 KiB`; consider
  the OCaml GC work in `sw-cor24-ocaml#28`" is more actionable
  than "heap claims overlap region cell."

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

### Scenario D -- "composite image from N modules" (gap A1)

Several repos compose one runtime image from N independently-
assembled modules. snobol4 builds `snobol4.bin` from four
`sno_*.s` modules linked by `link24`. macrolisp's multi-module
demo loads five `.s` blobs at fixed slot addresses.

```
+----------------------------------------+ 0x000000  <-- entry
| sno_main + sno_util + sno_lex + sno_exec|
| (linked composite)                     |
+----------------------------------------+ ~0x010000
| ... free SRAM ...                      |
+----------------------------------------+ 0x080000
| user source                            |
+----------------------------------------+ 0x090000
| optional input data                    |
+----------------------------------------+ 0xFEEC00  EBR
+----------------------------------------+ 0xFF0000  MMIO
```

```toml
[scenarios.snobol-hello]
target = "cor24"
layers = ["snobol4_image", "user_source"]
entry  = "0x000000"

[scenarios.snobol-hello.run]
mode       = "batch"
max_cycles = 10_000_000
timeout_ms = 30_000
halt_on    = "monitor-exit"

[scenarios.snobol-hello.expect]
uart_contains = ["HELLO"]

[layers.snobol4_image]
kind   = "composite"
linker = "link24"
modules = [
  { name = "sno_main", input = "build/sno_main.s" },
  { name = "sno_util", input = "build/sno_util.s" },
  { name = "sno_lex",  input = "build/sno_lex.s"  },
  { name = "sno_exec", input = "build/sno_exec.s" },
]
artifact = "build/snobol4.bin"
absolute_addresses = true
load.method  = "memory"
load.address = "0x000000"

[layers.user_source]
kind   = "data"
input  = "examples/hello.sno"
load.method  = "memory"
load.address = "0x080000"
size         = "auto"
```

The optional input-data slot is a *conditional* layer (gap E2):

```toml
[[scenarios.snobol-hello.conditional_loads]]
when      = { file_present = "examples/hello.dat" }
add_layer = "user_data"

[layers.user_data]
kind   = "data"
input  = "examples/hello.dat"
load.method  = "memory"
load.address = "0x090000"
```

If the data file is present, the layer is included; runtime
selects "data mode" at the binary's entry probe (gap A2). If not,
the layer is skipped and runtime falls into "TTY mode."

### Scenario E -- "resident shell + program slots" (gap F1, F2)

A monitor or sws-style shell stays loaded; user-typed program
names invoke pre-loaded program binaries via a trampoline.
sw-launch kicks off the emulator, then hands control to a
TUI-driving harness (or to a test driver feeding canned UART).

```
+----------------------------------------+ 0x000000  <-- entry
| monitor + service vector                |
+----------------------------------------+ 0x002000
| program slot 0                          |
+----------------------------------------+ 0x020000
| sws shell                               |
+----------------------------------------+ 0x040000
| program slot 1                          |
+----------------------------------------+
| ... free SRAM ...                      |
+----------------------------------------+ 0x0F0000
| shared regions (run_cmd, run_out, entry)|
+----------------------------------------+ 0xFEEC00  EBR
+----------------------------------------+ 0xFF0000  MMIO

UART  : interactive (mode = "resident")
```

```toml
[scenarios.monitor-with-forth]
target = "cor24"
layers = ["monitor", "sws", "forth_kernel"]
entry  = "0x000000"

[scenarios.monitor-with-forth.run]
mode    = "resident"
halt_on = "user-quit"

# expectations are streaming for resident mode
[scenarios.monitor-with-forth.expect]
uart_contains = ["mon> "]

[[scenarios.monitor-with-forth.programs]]
slot  = "forth"
layer = "forth_kernel"
entry = "self.address"

[layers.monitor]
kind = "binary"
absolute_addresses = true
input = "build/monitor.bin"
load.method  = "memory"
load.address = "0x000000"

[layers.monitor.shared_regions]
run_request = { start = "0x0F0000", size = "0x000020", role = "input" }

[layers.sws]
kind = "binary"
absolute_addresses = true
input = "build/sws.bin"
load.method  = "memory"
load.address = "0x020000"

[layers.sws.shared_regions]
run_cmd_buf = { start = "0x0F0000", size = "0x000400", role = "input"  }
run_out_buf = { start = "0x0F0400", size = "0x001000", role = "output" }
run_entry_0 = { start = "0x0FFE00", size = "0x000003", role = "control" }
cooperates_with = ["monitor"]   # silences E0020 for run_request overlap

[layers.forth_kernel]
kind = "assembler"
input = "forth.s"
tool  = "assembler"
artifact = "forth.bin"
absolute_addresses = true
load.method  = "memory"
load.address = "0x040000"
```

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

Reserved (non-embedded) segments can be located three ways, in
decreasing order of preference:

1. **Partition claim** (default): `claims = [{ partition = N,
   region = "..."}, ...]`. The launcher computes the absolute
   range. Multi-region claims are allowed and contiguous claims
   are encouraged (E0022 warns on non-contiguous).
2. **Absolute address**: `load.address = "0x..."` and `size =
   "0x..."`. Required for the layer to also set
   `absolute_addresses = true`.
3. **Inferred from another layer's segment**: `claims = [{
   layer = "<other>", segment = "<sname>", role = "after" |
   "before" }]`. The launcher places this segment immediately
   above (or below) the referenced one; useful for guard
   regions and adjacency-encoded heaps.

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

# v1.2: no fixed partition grid here. Memory layout is declared
# in [memory_profiles.<name>] blocks, and each scenario picks one.
# See "Addressing model: memory profiles" above.

# Optional override for the COR24 hardware stack pointer init.
# (Gap B4: apl uses --stack-kilobytes 8; plsw effectively uses 8 KiB.)
[targets.cor24.run_defaults]
stack_kilobytes = 3         # default; per-scenario `run.stack_kilobytes` overrides
```

`validate` enforces (codes are stable; full list in "Validation
rules"):

- Every loaded byte and every claimed region falls inside
  `regions.sram` (E0011).
- Nothing user-loaded touches `regions.ebr_stack` or
  `regions.mmio` (E0011).
- No two layers' resolved ranges overlap, whether via partition
  claims or absolute addresses (E0003 / E0017).
- Total claimed bytes <= sram size (E0012).
- A partition's region cells don't overlap absolute-address
  ranges of any other layer (E0017).

This is what catches the "I forgot the OCaml heap collides with
the DSL heap" class of bug at `check` time, before the emulator
starts.

## Layer kinds

| kind            | input(s)              | tool             | artifact      | typical load | gap |
|-----------------|-----------------------|------------------|---------------|--------------|-----|
| `assembler`     | `.s`                  | `assembler`      | `.bin`        | memory       |     |
| `binary`        | `.bin`                | (none, copy)     | `.bin`        | memory       |     |
| `pcode`         | `.spc`                | `pcode_assembler`| `.p24`        | memory       |     |
| `pcode-image`   | `.p24m` (pre-linked)  | (none, copy)     | `.p24m`       | memory       |     |
| `text`          | UTF-8 text            | (none)           | bytes         | uart         |     |
| `data`          | bytes                 | (none)           | bytes         | uart, memory |     |
| `composite`     | list of layers        | linker (host)    | one `.bin`    | memory       | A1  |
| `uart-preamble` | ordered list of files | (none, concat)   | bytes         | uart         | A3  |
| `uart-prebuffer`| bytes                 | (none, copy)     | bytes         | memory       | F3  |
| `snapshot`     | host post-processor    | extractor (host) | binary blob   | memory       | A5  |
| `regenerated`   | rebuild script         | script           | any           | memory/uart  | A4  |

### `composite` layer (gap A1)

Combines N child layers into one image, linked host-side. Used by
snobol4 (link24), macrolisp's multi-module demo (concatenated
modules at fixed slots), and monitor's program registry.

```toml
[layers.snobol4_image]
kind = "composite"
linker = "link24"           # references [tools.link24]
modules = [
  { name = "sno_main", input = "build/sno_main.s", base = "auto" },
  { name = "sno_util", input = "build/sno_util.s", base = "auto" },
  { name = "sno_lex",  input = "build/sno_lex.s",  base = "auto" },
  { name = "sno_exec", input = "build/sno_exec.s", base = "auto" },
]
artifact = "snobol4.bin"
load.method  = "memory"
load.address = "0x000000"
```

`base = "auto"` lets the linker compute contiguous bases; an
explicit hex address pins a module at a fixed slot.

### `uart-preamble` layer (gap A3)

Ordered list of source files concatenated as the UART payload
before user-supplied input. Used by all four forth kernels.

```toml
[layers.forth_preamble]
kind = "uart-preamble"
sources = [
  "core/00-prelude.fth",
  "core/10-arith.fth",
  "core/20-strings.fth",
]
load.method = "uart"
load.terminator = "none"
```

### `uart-prebuffer` layer (gap F3)

A canned UART command stream pre-staged in *memory* at a fixed
address; consumed by the runtime before falling through to live
UART. Used by yocto-ed (`SYE_CMD_ADDR=0x0F0000`).

```toml
[layers.swye_prestage]
kind = "uart-prebuffer"
input = "tests/canned-keystrokes.bin"
load.method = "memory"
load.address = "0x0F0000"
size = "0x000400"
```

### `snapshot` layer (gap A5)

A two-phase artifact: phase 1 runs an emulator scenario that
emits state via UART; phase 2 reloads the rehydrated snapshot.
Used by macrolisp (`snapshot-save.s` -> `extract-snapshot.py`
-> `prelude.snap`).

```toml
[layers.macrolisp_prelude]
kind = "snapshot"
generator = { scenario = "snapshot-save", capture = "uart" }
post_process = { tool = "extract_snapshot_py", input_format = "tml-hex" }
artifact = "build/prelude.snap"
load.method = "memory"
load.address = "0x080000"
```

The launcher runs the generator scenario the first time the
snapshot is needed, captures UART, hands it to the post-processor,
and caches the resulting blob. Subsequent runs hit the cache.

### `regenerated` layer (gap A4)

A layer whose source is committed to git but reproducible from a
build script. Used by forth-from-forth's `kernel.s`.

```toml
[layers.fff_kernel]
kind = "regenerated"
input = "forth-from-forth/kernel.s"
regen = "forth-from-forth/scripts/build-kernel.sh"
artifact = "forth-from-forth/kernel.bin"
load.method = "memory"
load.address = "0x000000"
```

The cache key incorporates `sha256(<regen script output>)`. If
the committed `kernel.s` and the regen output diverge,
`sw-launch check` warns (new code E0021).

## Load methods

```toml
load.method = "memory"
load.address = "0x010000"   # required if absolute_addresses
# OR
claims = [{ partition = 1, region = "code" }]   # partition mode
```

```toml
load.method = "uart"
load.max_bytes = 4096       # required, hard cap
load.terminator = "EOT" | "EOF" | "none"
load.encoding = "raw" | "escape-interpreted"   # default raw; gap D2
load.role = "command" | "source" | "data"      # gap D5
load.in_band_terminator = ")OFF"               # optional; gap D2
```

```toml
# Composing multiple UART chunks (gap D1: ocaml/tuplet pattern).
# Order is the order chunks are declared in the scenario.
[scenarios.<n>.uart]
chunks = [
  { layer = "ocaml_source",  terminator = "EOT" },
  { layer = "ocaml_stdin",   terminator = "none" },
]
```

UART layers can also declare per-file framing (gap D3):

```toml
[layers.ocaml_source]
kind = "uart-preamble"
sources = ["demo.ml", "newlang.ml"]
[layers.ocaml_source.per_file]
prelude_template = "let __module = \"{module}\"\n"
# placeholders: {name}, {stem}, {module}=stem with leading uppercase
```

Source pre-normalization (gap D4) is declared via named
transforms (see "Tool model" below):

```toml
[layers.ocaml_source]
# ...
normalize = ["logical-line-fold", "strip-comments"]
```

Future load methods (declared but unimplemented in MVP):
`card`, `disk`, `flash`, `serial-loader`.

## Patches

```toml
patches = [
  { target = "pcode_vm.code_ptr", value = "0x010000" },
  { target = "0x000A12",          value = "0x000042" },
]
```

### `target` forms

- **Literal hex address**: `target = "0x000A12"`. Patches the
  byte/word at that absolute address.
- **`<layer>.<symbol>`**: resolved by reading the producing
  layer's listing/map file. The producing layer must declare
  `exports.symbols = ["..."]` and produce a parseable `.lst`.
- **`<upstream-layer>.<symbol>` across vendors** (gap B2): if
  `<upstream-layer>` is a vendored layer in the scenario,
  symbols come from its vendored listing or sidecar. This
  enables tuplet to pull pvm symbols from sw-cor24-ocaml's
  build without re-deriving them.

### `value` forms

- **Literal hex**: `value = "0x010000"`.
- **`self.address` / `self.end` / `self.size`**: only valid
  inside a segment block; expands to the segment's resolved
  address/end/size.
- **`<layer>.<segment>.address` etc.** (gap B2): cross-layer
  segment address reference, useful when one layer's heap
  ceiling must equal another layer's start.
- **`sidecar:<path>`** (gap B1): reads a hex-formatted address
  from a build-time sidecar file. Used by ocaml and tuplet for
  `code_ptr_addr.txt` and `heap_limit_addr.txt`. The sidecar
  path is relative to the producing layer's build directory.
  The lockfile records the sidecar's sha256; a stale sidecar
  is E0019.

  ```toml
  patches = [
    { target = "0xsidecar:vendor:sw-pcode/build/code_ptr_addr.txt",
      value = "ocaml_interp.address" },
    { target = "0xsidecar:vendor:sw-pcode/build/heap_limit_addr.txt",
      value = "ocaml_interp.value_heap.end" },
  ]
  ```

  The `0xsidecar:` prefix on `target` means: read this sidecar
  for the address of the *symbol being patched*, then patch
  that address with `value`. (Verbose but explicit; alternative
  syntax discussed in step 2 trajectory.)

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
17. (E0017) Partition-cell collision: two layers claim the same
    `(partition, region)` cell. Diagnostic names both layers and
    the offending cell.
18. (E0018) Mixed-mode collision: a partition-relative claim and
    an absolute-address layer's resolved range overlap. Diagnostic
    shows both ranges in absolute hex.
19. (E0019) Sidecar staleness: a `sidecar:` patch source's
    sha256 differs from the lockfile entry. Suggests
    `vendor sync` or `--update-lock`.
20. (E0020) Shared-region overlap: two layers declare
    `[layers.<n>.shared_regions.<r>]` covering the same address
    range without naming each other as cooperating peers. (Gap H3.)
21. (E0021) Regenerated drift (warning, not error): a
    `kind = "regenerated"` layer's committed input differs from
    the regen-script output. Suggests rerunning the regen.
22. (E0022) Non-contiguous multi-region claim (warning, not
    error): a heap or stack claims partition X r3 + partition Y r0
    (skipping partitions in between is unusual; flag it). User can
    silence with `claims_contiguity = "non-strict"`.
23. (E0023) Mode/load mismatch: `run.mode = "resident"` requires
    at least one layer with `kind = "binary"` declaring a
    program slot, otherwise the resident shell has nothing to
    invoke. (Gap E3, F1.)
24. (E0024) Cycle/timeout outlier (warning, not error): scenario
    `run.max_cycles` is more than 100x the median for this target.
    Often legitimate; flagged to surface accidents.
25. (E0025) Composite linker missing: a `kind = "composite"` layer
    references a `linker` that is not in `[tools.*]`.
26. (E0026) Per-file framing on non-uart layer: `per_file` block
    on a layer whose `load.method != "uart"`.
27. (E0027) Guard-region too small (warning, not error): adjacent
    layers in absolute mode are separated by less than the
    target's `min_guard` (default 0; configurable per scenario).
28. (E0028) Scenario heap claims exceed
    `memory_profile.budget.heap_max`. Diagnostic shows the sum of
    heap claims, the budget, and the offending layers. *Hard error.*
29. (E0029) Scenario heap claims exceed 80% of
    `memory_profile.budget.heap_max`. *Warning, not error.* Useful
    early signal that bloat is creeping in.
30. (E0030) Heap region > 32 KiB without a `heap_justification`
    block. Diagnostic names the layer and the
    accepted-category list (see `docs/memory-stance.md`).
31. (E0031) Scenario references a `memory_profile` not declared
    in `[memory_profiles.*]`. Diagnostic suggests the closest
    Levenshtein match among declared profiles.
32. (E0032) Layer claim names a `(partition, region)` not
    present in the profile it references. Diagnostic lists the
    profile's available `(partition, region)` pairs.
33. (E0033) Memory profile declares overlapping partitions.
    Sanity check on the profile itself (separate from cross-layer
    overlap, E0003 / E0017).
34. (E0034) Total reserved exceeds the 1 MiB rule of thumb
    (`docs/memory-stance.md`). *Warning, not error.* Silenced per
    layer with `acknowledge_oversized = true`. Profile budgets
    that *individually* respect this rule but combine over 1 MiB
    in a multi-profile scenario also trigger this.

`--strict` mode promotes warnings to errors, with two
exceptions:
- E0024 (cycle-budget outlier) and E0027 (guard too small) stay
  warnings even under `--strict`.
- `heap_justification.category = "dead-leak"` and
  `"algorithmic-bloat"` are *always* errors under `--strict`,
  regardless of `acknowledge_oversized`. The launcher refuses to
  bless those categories as production-acceptable.

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

## Run modes (gap E3)

Every scenario declares a `run.mode`:

| mode       | meaning                                                 |
|------------|---------------------------------------------------------|
| `batch`    | one-shot run; UART input is fully prepared in advance; emulator exits when halted; expectations checked against final UART. Default for Scenarios A-D. |
| `terminal` | interactive; stdin streams into UART, stdout streams from UART; the launcher does not auto-halt. |
| `resident` | the loaded image is a resident shell or monitor; the launcher kicks off and hands control to a TUI driver; expectations are partial/streaming. Default for Scenario E. |
| `echo-line`| interactive line-buffered; useful for REPLs where each line gets its own expectation. |

The launcher translates each mode into the appropriate
`cor24-run` flags (`--terminal`, `--echo`, etc.).

## Programs slot table (gap F1)

A scenario can declare a *program registry* the resident shell
exposes. Each entry maps a name to a layer's entry address.
Used by monitor's `mon_run_request` registry; reusable by any
resident-mode scenario.

```toml
[scenarios.monitor-with-forth]
target = "cor24"
layers = ["monitor", "sws", "forth_kernel"]
entry  = "0x000000"

[scenarios.monitor-with-forth.run]
mode       = "resident"
timeout_ms = 0           # no timeout in resident mode
max_cycles = 0
halt_on    = "user-quit"

[[scenarios.monitor-with-forth.programs]]
slot  = "forth"
layer = "forth_kernel"
entry = "self.address"   # entry = layer's load address

[[scenarios.monitor-with-forth.programs]]
slot  = "list"
layer = "monitor"
entry = "monitor.list_command"
```

The launcher writes the resolved `(slot, entry)` table into a
shared-memory region the monitor reads at boot (declared in
`monitor`'s `shared_regions`).

## Shared regions (gap F2)

Address ranges shared between layers (resident shell + program;
editor + host driver). Declared per layer, validated globally.

```toml
[layers.sws.shared_regions]
run_cmd_buf = { start = "0x0F0000", size = "0x000400", role = "input"  }
run_out_buf = { start = "0x0F0400", size = "0x001000", role = "output" }
run_entry_0 = { start = "0x0FFE00", size = "0x000003", role = "control" }

[layers.swye.shared_regions]
swye_cmd     = { start = "0x0F0000", size = "0x000400", role = "input" }
swye_quit    = { start = "0x0F0400", size = "0x001000", role = "output" }
```

If two layers in the same scenario declare overlapping
`shared_regions` without naming each other in `cooperates_with`,
that's E0020.

## Conditional loads (gap E2)

Some scenarios load extra layers based on filename presence
or environment variables (apl: presence of a `.cor24` file flips
on a batch image; snobol4: presence of input data switches
runtime mode).

```toml
[scenarios.apl-batch]
target = "cor24"
base_layers = ["apl_interp"]

[[scenarios.apl-batch.conditional_loads]]
when     = { file_present = "examples/{program}.cor24" }
add_layer = "apl_batch_image"

[[scenarios.apl-batch.conditional_loads]]
when     = { env_set = "APL_TRACE" }
add_layer = "trace_overlay"
```

Predicates: `file_present`, `file_absent`, `env_set`,
`env_eq = { name = "...", value = "..." }`, `profile = "..."`.

## Profiles

Profiles override scenario `run.*` settings without redefining the
scenario.

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

[profiles.compile-budget]   # gap E1: plsw uses this kind of profile
max_cycles  = 200_000_000
timeout_ms  = 120_000

[profiles.run-budget]
max_cycles  = 50_000_000
timeout_ms  = 30_000
```

`sw-launch run --profile test <scenario>` overrides any
scenario-level run config with the profile's. Default profile is
`demo`. Per-scenario profile lists (gap E1) let one scenario
declare multiple available profiles:

```toml
[scenarios.plsw-hello.profiles.compile]
inherits = "compile-budget"
add_layers = ["plsw_compiler"]

[scenarios.plsw-hello.profiles.run]
inherits = "run-budget"
add_layers = ["plsw_compiled_program"]
```

## Tool model (new in v1.1; gaps C1, C2, G1, G2, G3)

`[tools.<name>]` describes how to invoke a build tool. Each tool
has a `kind`, a resolution `source`, and optional version pin.

### Tool kinds

| kind             | meaning                                       | gap |
|------------------|-----------------------------------------------|-----|
| `host-binary`    | a host executable, e.g. cor24-run, pa24r      |     |
| `emulator-hosted`| invoke the emulator with a compiler image and source over UART; capture stdout between markers; output is the next stage's input | C1 |
| `script`         | shell or python script                        |     |
| `composite`      | sequence of tool invocations                  |     |

```toml
[tools.assembler]
kind     = "host-binary"
source   = { path = "/usr/local/bin/cor24-run" }
version  = { command = ["cor24-run", "--version"], expect_regex = "^cor24-run 0\\." }

[tools.pa24r]
kind     = "host-binary"
source   = { vendor = "sw-pcode", artifact = "pa24r" }

[tools.link24]
kind     = "host-binary"
source   = { sibling = "../sw-cor24-plsw/components/linker", artifact = "target/release/link24" }

[tools.plsw_compile]
kind     = "emulator-hosted"
runtime  = { layer = "plsw_compiler" }   # pre-existing layer
input    = { method = "uart", framing = { begin = "FILE:", end = "" } }
output   = { method = "uart", capture_between = ["BEGIN", "END"] }

[tools.extract_snapshot_py]
kind     = "script"
source   = { path = "scripts/extract-snapshot.py" }
```

### Source resolution kinds (gaps G1, G2, G3)

- `path`: absolute or repo-relative file path.
- `vendor`: refers to a `[vendor.<name>]` entry; lockfile pins
  the version.
- `sibling`: refers to a sibling repo path; lockfile pins the
  sibling's HEAD SHA at pin time. (Gap G3.)
- `from_path`: just looks up the binary on `$PATH`; the
  lockfile records the *observed* version string and binary
  sha256 even though no pin was declared. (Gap G2; covers apl,
  forth, macrolisp, monitor.)

### Post-process transforms (gap C2)

A small allow-list of named transforms tools can chain:

| name                   | parameters              | meaning                                    |
|------------------------|-------------------------|--------------------------------------------|
| `extract-between`      | `begin`, `end`          | keep only lines between markers            |
| `strip-prefix-lines`   | `prefix`                | drop matching prefix lines                 |
| `ascii-filter`         | (none)                  | strip non-ASCII bytes                      |
| `logical-line-fold`    | (none)                  | OCaml-style logical-line folding           |
| `strip-comments`       | `style = "ml"\|"shell"` | drop `(* ... *)` or `# ...`                |
| `strip-bom`            | (none)                  | strip leading BOM                          |
| `splice`               | `begin`, `end`, `with`  | replace marked region with another payload |
| `append`               | `bytes`                 | concat literal bytes (e.g. EOT)            |
| `dedup-blank`          | (none)                  | collapse runs of blank lines               |

Tools and layers can declare a chain:

```toml
[layers.ocaml_source]
# ...
normalize = ["logical-line-fold", "strip-comments"]

[tools.plsw_compile]
# ...
post_process = [
  { extract-between = { begin = "BEGIN", end = "END" } },
  { append = { bytes = "" } },
]
```

The transform set is closed; new transforms require a schema
version bump.

## Out of scope (v1.1)

The following gaps were observed but explicitly deferred:

- **I1. Filesystem stubs** (sws's `fs_read_file`). The launcher
  doesn't model a virtual filesystem.
- **I2. GC scheduling annotations.** The runtime owns this; the
  launcher only owns the heap region.
- **I3. Diff-based snapshot updates.** Phase 5+ if at all.
- **Multi-target sweeps in one config.** A scenario binds to one
  target; cross-target runs are driven by shell.
- **Continuous-integration metadata.** The lockfile records what
  was used; CI is a separate concern.
- **Pre-emption / signal handling.** The launcher is fire-and-
  forget plus a host-side timeout. Future Phase 5 work for
  `run.mode = "resident"`.

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
