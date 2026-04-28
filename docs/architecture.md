# sw-launcher Architecture

## One-line summary

`sw-launch` is a host-side Rust CLI that reads a `sw-launch.toml`,
resolves layered build artifacts (with caching and vendoring),
constructs a deterministic load plan, invokes a target emulator, and
checks expected output.

## High-level data flow

```
sw-launch.toml ---> [parse] ---> Config
sw-launch.lock ---> [parse] ---> Lockfile
                                    |
                                    v
                            [validate scenario]
                                    |
                                    v
                          [resolve layer DAG]
                                    |
                                    v
                  +-----------------+------------------+
                  |                                    |
          [for each layer]                     [for each tool]
                  |                                    |
                  v                                    v
            [cache lookup]                    [verify version]
              hit / miss                              |
                  |                                    |
                  v                                    |
            [invoke tool]<-----------------------------+
                  |
                  v
            [hash artifact]
                  |
                  v
            [populate cache]
                  |
                  v
          [build LoadPlan]
                  |
                  v
       [emit emulator argv]
                  |
                  v
         [run cor24-run]  --captured stdout/stderr-->  [expectations]
                  |                                          |
                  v                                          v
              exit status                              report pass/fail
```

## Crate / module layout

A single binary crate at the repo root, no workspace yet (kept simple
until a target backend warrants its own crate).

```
src/
  main.rs        # thin -- arg parse, dispatch, exit code
  cli.rs         # clap definitions for run/build/check/graph/cache/vendor/doctor
  config.rs      # serde types for sw-launch.toml; deserialize + normalize
  validate.rs    # memory-overlap, UART-size, layer-method compatibility, cycles
  graph.rs       # layer DAG; topological order; petgraph
  cache.rs       # content-addressed cache; key derivation; layout under
                 # ~/.cache/sw-launch/
  vendor.rs      # lockfile read/write; resolution (stubbed to local paths in MVP)
  tool.rs        # ToolInvocation: assembler, p-code asm, p-code link, custom
  layer.rs       # Layer kinds (Asm, Binary, PCode, Source, Data); build action
  scenario.rs    # Scenario: list of layers + run config + expectations
  manifest.rs    # LoadPlan: address map, --load-binary args, --patch args, entry
  target/        # target backends
    mod.rs       # trait Target { fn build_argv(...); fn run(...); }
    cor24.rs     # cor24-run driver
  run.rs         # spawn, stream stdout/stderr, enforce timeout / max-cycles
  expect.rs      # check stdout_contains, exit_code, regex matches
  error.rs       # thiserror enum for diagnostic messages
tests/
  fixtures/      # tiny .s / .spc / .ml inputs that exercise each scenario shape
  scenario_a.rs  # assembled program at 0 + UART (TDD red/green)
  scenario_b.rs  # pvm + p-code blob with code_ptr patch
  scenario_c.rs  # nested interpreter (interpreter + source + data)
  validation.rs  # negative-path tests for each validator rule
  cache.rs       # determinism / hit / miss tests
```

Source-file size budget: keep each module under 500 lines. `target/`
folder exists from day one so we can add `target/ibm1130.rs` etc.
without renaming.

## External dependencies (initial)

- `clap` (derive) -- CLI
- `serde` + `toml` -- config parsing
- `serde_json` -- emit machine-readable graph / explain output
- `camino` -- UTF-8 paths
- `sha2` -- cache key hashing
- `petgraph` -- layer DAG and cycle detection
- `thiserror` + `anyhow` -- error model
- `assert_cmd` + `predicates` + `tempfile` -- integration tests

No async runtime. Subprocess via `std::process::Command`.

## Layers are composites, not blobs

A layer is `(artifact?) + (segments)`. Segments may be **embedded** in
the artifact (statically reserved inside the assembled image, e.g.
`pvm.s`'s `eval_stack`/`call_stack`/`heap_seg`) or **reserved** by the
loader at a configured address.

The COR24 hardware stack is only 3 KB EBR -- enough for `pvm.s`
itself, nothing more. Every higher layer (p-code VM internals, OCaml
interpreter, DSL on top, app data) needs its own heap and (often)
its own stacks placed in high SRAM and patched into the runtime so
the runtime knows where to find them. `sw-launch` is the thing that
allocates those regions, checks for collisions, and emits the
patches.

See `docs/design.md` "Memory segments per layer" for the schema and
"Memory layout for Scenarios B and C" for example pictures.

## Addressing model: partition grid

Schema v1.1 (after the survey) makes the *partition grid* the
default addressing mode: 8 partitions of 128 KiB, 4 regions per
partition (32 KiB each, named `code`/`heap`/`spare`/`stack`).
Partition-relative claims compose more cleanly than absolute
addresses and produce a stable visual for `sw-launch graph`.

Layers that don't fit (plsw's monolithic ~1 MiB compiler;
mid-partition slot-based layouts like macrolisp's multi-module
demo) opt out via `absolute_addresses = true` and continue to
work. The validator runs the overlap check at byte grain, so
mixed-mode scenarios are valid.

See `docs/survey/partition-model-proposal.md` for the rationale
and fit analysis across the 13 surveyed repos.

## Run modes

Schema v1.1 introduces explicit `run.mode` values: `batch`,
`terminal`, `resident`, `echo-line`. Most existing scenarios are
`batch` (one-shot run, fully prepared UART, halt on
`monitor-exit`). Resident-shell scenarios (monitor, sws,
yocto-ed) are `resident`: the launcher kicks the emulator off
and hands control to a TUI driver or canned-input harness, and
expectations are streaming rather than terminal.

See `docs/design.md` "Run modes" for the complete table.

## Key invariants

1. **Lockfile is the source of truth at run time.** The TOML expresses
   intent; the lock pins resolved versions and artifact hashes. `run`
   refuses to proceed against a stale lockfile unless `--update-lock`
   is passed.
2. **Cache is content-addressed.** A cache hit is sound: if the key
   matches, the artifact would be byte-identical to a fresh build, so
   reusing it can never produce a different scenario result.
3. **Validation runs before any tool invocation.** `sw-launch check`
   never spawns assemblers or emulators -- it only reads config + lock
   and reports.
4. **One target per scenario.** Multi-target sweeps are driven by the
   shell, not by the scenario format.
5. **Loaders never overlap.** Address-range overlap is a hard error,
   not a warning. UART payloads exceeding declared `max_bytes` are
   hard errors. The overlap check spans both code/data and reserved
   stack/heap segments, both embedded and non-embedded.
6. **Stacks and heaps are first-class.** A layer that does not
   declare its stack/heap segments (embedded or reserved) cannot be
   composed with another layer that allocates above it -- the
   validator refuses the scenario rather than risking a silent
   collision.
7. **Partition grid is the default addressing mode.** Schema v1.1
   makes partition-relative claims the recommended way to express
   memory layout; absolute addresses are supported but require
   `absolute_addresses = true` and surface as a deviation in
   `sw-launch graph`.
8. **Run mode is declared, not inferred.** Every scenario states
   one of `batch | terminal | resident | echo-line`. The launcher
   refuses to apply mode-incompatible flags (e.g. timeout in
   resident mode is informational, not load-bearing).

## Targets and backends

Targets are described in TOML and dispatched through a `Target` trait.
For COR24, the backend wraps `cor24-run`:

- `--load-binary path@hex_addr` for each memory-loaded layer
- `--patch addr=value` for runtime patches (e.g., p-code VM
  `code_ptr` pointing to bytecode origin)
- `--entry hex_addr` for the COR24 PC start
- `--uart-input "..."` for UART-fed text/data layers
- `--speed 0` and `-n <max-insn>` for non-interactive bounded runs
- `--terminal` for interactive sessions (driven by run profile)

Future backends declare different loader semantics: a `card-or-disk`
loader for IBM 1130, an `intel-hex` loader for 1802, an `elf-flat`
loader for RISC-V. Each backend translates a generic `LoadPlan` into
its native invocation.

## Cache directory layout

```
~/.cache/sw-launch/
  schema-1/
    artifacts/
      <sha256>.bin           # opaque artifact blobs
    layers/
      <layer-key>.json       # what the artifact was, build provenance
    runs/
      <scenario-run-id>/     # captured stdout/stderr, manifest, exit
```

Every cache entry records: tool path + version, input hashes, build
args, dependency hashes, schema version, and absolute load address.
That last bit matters because most COR24 binaries are non-PIC at this
stage and hash-equivalent only at a fixed address.

## Vendor directory layout

```
~/.local/share/sw-launch/
  vendor/
    <package>/<version>/
      version.json           # resolved manifest
      <artifacts>            # binaries / .s files / .p24 / etc.
```

For MVP, `vendor sync` is a stub that copies from local sibling
repos described in TOML. The schema already accommodates `git`
sources (rev/tag) so swapping the resolver in later is mechanical.

## Run lifecycle

1. `cli` parses argv, locates `sw-launch.toml`.
2. `config` deserializes into typed `Config`.
3. `vendor` loads `sw-launch.lock`, fails fast on staleness.
4. `validate` enforces structural rules.
5. `graph` topologically orders layers.
6. `cache` checks each layer key; on miss, `tool` invokes the
   appropriate builder; result is hashed and stored.
7. `manifest` produces a `LoadPlan` -- ordered list of memory and
   UART operations plus an entry address.
8. `target` translates the plan into emulator argv.
9. `run` spawns the emulator and captures output.
10. `expect` matches against scenario expectations and emits a
    structured report.

`sw-launch check` stops after step 4. `sw-launch build` stops after
step 6. `sw-launch graph` stops after step 5. `sw-launch run` runs
the full pipeline.

## Error model

All user-visible errors carry: a stable error code, a one-line
summary, the offending TOML span when applicable, and a remediation
hint. Example:

```
error[E0042]: layer `demo_input` overlaps `ocaml_pcode`
  --> sw-launch.toml:81:1
   |
   |   demo_input  : 0x00C000..0x00D2A0
   |   ocaml_pcode : 0x008000..0x00CFFF
   |
   = hint: move demo_input to >=0x00D000 or shrink it
```

This is also why `sw-launch check` is a separate verb -- agents are
expected to run it before `run` and consume errors programmatically.
