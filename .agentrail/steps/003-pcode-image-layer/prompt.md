# Step 3: pcode-image-layer

Wire `Layer.kind = "pcode"` and `Layer.kind = "pcode-image"`
through `assemble_artifacts` so the cli's run/build path picks
the right tool per layer.

TDD: tests first.

## Inputs

- v1.2 schema in `docs/design.md` already lists `pcode` and
  `pcode-image` in the layer-kinds table.
- Step 1's `PcodeAssembler`.
- Step 008's `manifest::collect_memory` -- already produces
  `MemoryLoad` from a memory layer's load.address; the
  artifact path comes from `Artifacts.by_layer`.
- Step 009's `cli::assemble_artifacts` -- only invokes
  `Assembler` today; this step adds the dispatch on
  `layer.kind`.

## What lands

- `src/cli.rs` `assemble_artifacts` extended:
  - `kind = "assembler"` -> `Assembler::build`
    (Phase 1 behavior, unchanged)
  - `kind = "pcode"`     -> `PcodeAssembler::build`
  - `kind = "pcode-image"` -> no build; the artifact path is
    `cfg_dir.join(layer.input)` directly. Listing is empty
    (a pre-linked `.p24m` has no `.lst` sidecar).
  - `kind = "binary"`    -> same as `pcode-image`: no build,
    take `layer.input` as the artifact path. (Phase 1 was
    silently skipping binary-kind layers in the build phase
    because no build was needed; this step makes that
    explicit so the path appears in `Artifacts`.)
- `src/cli.rs` may need a helper struct `ToolBox` holding
  both `Assembler` and `PcodeAssembler` so the orchestrator
  passes one value through; keep cli.rs's function count
  under sw-checklist's budget.

## Tests (write first)

Unit tests:

- `assemble_artifacts_dispatches_assembler_for_assembler_kind`
- `assemble_artifacts_dispatches_pcode_for_pcode_kind`
- `assemble_artifacts_passes_through_pcode_image_kind_unchanged`
- `assemble_artifacts_passes_through_binary_kind_unchanged`

Use stub assemblers (the `/usr/bin/true` no-op trick from
step 007) so unit tests don't depend on cor24-run / pa24r.

Integration (in `tests/scenario_b_assemble.rs` or merged into
`tests/scenario_a.rs` to avoid binary-count creep): with both
`cor24-run` and `pa24r` on PATH, `sw-launch build pcode-hello
--config tests/fixtures/scenario_b/sw-launch.toml` produces
both `pvm.bin` and `hello.p24` in the configured locations.

## Pre-commit gate

Standard.

## Done when

- All four kind-dispatch unit tests pass.
- `sw-launch build pcode-hello` (when both tools on PATH)
  produces both artifacts.
- Phase 1 / Scenario A still works.
- `agentrail complete --summary "..." --reward 1`.

## Stop after complete

Step 4 (scenario-b-fixture-and-runner) is a fresh session.
