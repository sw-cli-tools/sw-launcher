# Step 5: graph

\`sw-launch graph <scenario>\` prints the layer DAG so users
and agents can see what a scenario depends on without
running it. Mostly presentation logic on top of the
existing LoadPlan + Config.

## What lands

### Subcommand surface

```
sw-launch graph <scenario> [--config <path>] [--json]
```

Output (text default):

```
demo:
  pcode_vm   (assembler:vendored:sw-cor24-pcode/vm/pvm.s)
    -> ocaml_interp   (binary:.../ocaml.p24m)
      -> ocaml_source (text:./demo.ml, uart EOT)
```

Each line: layer-name, kind, source spec / input path,
followed by indentation showing dependency direction. The
edge ordering is the same order the LoadPlan loads them.

\`--json\` emits a structured DAG suitable for piping into
agents:

```json
{
  \"scenario\": \"demo\",
  \"target\": \"cor24\",
  \"layers\": [
    { \"name\": \"pcode_vm\", \"kind\": \"assembler\",
      \"input\": \"...\", \"depends_on\": [] },
    { \"name\": \"ocaml_interp\", \"kind\": \"binary\",
      \"input\": \"...\", \"depends_on\": [\"pcode_vm\"] },
    { \"name\": \"ocaml_source\", \"kind\": \"text\",
      \"input\": \"./demo.ml\", \"depends_on\": [\"ocaml_interp\"] }
  ]
}
```

Edges come from cross-layer patches: any patch with
\`target = <layer>.<symbol>\` introduces an edge from the
patching layer to the patched layer. Layers without explicit
edges fall back to scenario-declaration order.

### Module placement

src/graph.rs (Option A) -- thin presentation logic over
Config + LoadPlan. Phase 5 sub-crate extraction can lift it
later if it grows.

### Tests (3 new)

tests/graph_cli.rs:
1. graph_text_renders_scenario_a_in_declaration_order
2. graph_json_round_trips_through_serde
3. graph_includes_cross_layer_patch_edges (use scenario_b
   with code_ptr patch -> assert pcode_app depends_on
   pcode_vm)

## Pre-commit gate

Standard. The 14 carry-forward sw-checklist failures should
not regress.

## Done when

- \`sw-launch graph nested-demo\` produces the picture that
  matches docs/survey/ocaml.md's nested-interpreter diagram.
- All 3 tests pass.
- \`--json\` parses as valid JSON.

## Citations

- docs/saga-phase4-plan.md step 5.
- docs/design.md \"CLI surface\".

## Scope guardrails

- Do NOT add interactive rendering (mermaid, graphviz). Step
  5 is text + JSON only.
- Do NOT add cycle detection beyond what manifest validation
  already does (E0006).
- Do NOT compute cache hit/miss in graph output -- that's
  what \`cache explain\` does.