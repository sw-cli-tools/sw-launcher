# sw-launcher PRD

## Problem

AI coding agents (and humans) working across the COR24 language tower
spend disproportionate effort *running* things. A demo or test almost
always involves several layered artifacts that must be built, ordered,
and loaded into emulator memory or fed via UART in exactly the right
way. Each higher-level project (basic, ocaml, pascal, plsw, smalltalk,
tuplet, ...) re-invents this orchestration in shell scripts, which:

- duplicates `cor24-run --load-binary X@0 --load-binary Y@0x010000
  --patch ... --entry 0` plumbing per project
- couples each project to specific paths in sibling repos
- offers no caching, so every test re-assembles `pvm.s` and re-builds
  the host toolchain
- has no schema for declaring "this app needs a p-code VM and a
  bytecode loader at this version"
- gives agents enough rope to invent novel, slow, broken pipelines

The result: agents try things that don't work, recompile too much, or
mis-load memory and silently fail.

## Goal

A single host-side CLI, **`sw-launch`** (repo `sw-cli-tools/sw-launcher`),
that turns a layered execution scenario into one boring, reproducible
command. Agents run `sw-launch run <scenario>` and the tool resolves
versions, builds artifacts (or pulls from cache), assembles a memory
load plan, invokes the emulator, and checks expectations.

## Scope (in)

- TOML-based scenario/layer/target schema (`sw-launch.toml`)
- Validation: memory overlap, UART size limits, missing tools, layer
  type vs load method compatibility, cyclic deps, scenario without a
  termination condition
- Content-addressed cache keyed on `(tool version, input hashes,
  dependency hashes, build args, load address, schema version)`
- Vendor model with a lockfile (`sw-launch.lock`) recording resolved
  refs and artifact hashes
- COR24 emulator backend (first target): drives `cor24-run` with
  `--load-binary X@addr`, `--patch addr=value`, `--entry`,
  `--uart-input`, `--terminal`, `--speed`, `-n <max-insn>`
- Three primitive scenario shapes (see `docs/design.md` for memory
  diagrams):
  - **A: assembled-program-at-zero**, with data via UART
  - **B: runtime-plus-binary**, e.g. p-code VM at 0 + a `.p24` blob at
    0x010000, with `code_ptr` patch
  - **C: nested-interpreter**, runtime + p-code interpreter + source +
    data, possibly across UART and memory channels
- `run`, `build`, `check`, `graph`, `cache list/explain`,
  `vendor sync/status`, `doctor` subcommands
- Test-driven development: every feature lands with unit and
  integration tests; CI gate: `cargo test`, `cargo clippy --all-targets
  --all-features -- -D warnings`, `cargo fmt --check`, `sw-checklist`

## Scope (out, for now)

- Targets other than COR24 (1130, 1802, RISC-V, ...) -- schema must
  *allow* them but no backend yet
- Git/HTTPS dependency fetching -- schema must allow it but vendor
  resolution stubs to local paths first
- Web UI / TUI
- Stress, fuzz, perf benchmarking
- Auto-generation of `sw-launch.toml` from existing per-project shell
  scripts (could be a future tool)

## Success criteria

A consuming project (e.g. `sw-cor24-basic`, `sw-cor24-ocaml`) replaces
its hand-rolled `scripts/run-*.sh` with a `sw-launch.toml` and a
single-line invocation, and:

1. The same command run twice does no redundant work the second time
   (cache hits).
2. `sw-launch check <scenario>` reports memory overlaps and missing
   tools without invoking the emulator.
3. `sw-launch run <scenario>` matches expected output (UART text,
   exit code) declared in TOML.
4. Bumping a vendored dependency is one edit to `sw-launch.toml` plus
   `sw-launch vendor sync`; bumping back is the same.
5. AI agents stop hand-rolling `cor24-run --load-binary ...` lines.

## Non-goals (explicit)

- We do not aim to replace `cor24-run`, `pa24r`, `pl24r`, or any
  language toolchain. `sw-launch` *drives* them.
- We do not implement memory layout for new architectures here. Each
  target backend declares its own loader rules in its plugin block.
- We do not parse or rewrite source files; the tool treats them as
  opaque inputs identified by hash and metadata.

## Stakeholders

- Primary user: AI coding agents working in `sw-embed/sw-cor24-*`
  repos, plus the human (Mike) reviewing and curating those agents.
- Secondary: future ports of language tooling to other targets
  (1130, 1802, RISC-V) that want the same scenario discipline.
