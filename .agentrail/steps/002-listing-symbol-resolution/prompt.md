# Step 2: listing-symbol-resolution

Promote E0006 from "shape only" (Phase 1) to "fully resolved"
(Phase 2). A patch with `target = "<other_layer>.<symbol>"` now
resolves to `<other_layer>'s load address + offset of <symbol>
in its listing`. This is the missing piece for Scenario B's
`code_ptr` patch to become a real `--patch <addr>=<value>`
argument.

TDD: tests first.

## Inputs

- `docs/design.md` "Patches" section's `target` forms list.
- `docs/survey/pascal.md` and `docs/survey/ocaml.md` -- both
  rely on this resolution shape.
- `src/manifest.rs` from step 008 -- `resolve_patch` currently
  returns `None` for non-hex targets. Replace that with a
  cross-layer lookup using the producing layer's `Listing`.
- `src/tool.rs` `Listing::resolve` -- already returns
  `Option<u32>` for a symbol name.

## What lands

- `src/manifest.rs::resolve_patch` extended:
  - if `target` starts with `0x`: literal hex (Phase 1
    behavior, unchanged).
  - if `target` parses as `<layer>.<symbol>`: look up
    `artifacts.by_layer.<layer>`. If absent, return `None`
    (validate caught it via E0006). If present, take the load
    address of that layer's MemoryLoad in the in-progress
    plan + the symbol's offset from its listing.
  - the patch's `value` form is unchanged from Phase 1
    (literal hex), with one new case for Scenario B:
    `value = "<layer>.address"` resolves to that layer's load
    address. (Distinct from `self.address`, which is for
    segment-internal patches.)
- `src/validate/...` (within validate.rs's existing structure):
  E0006 promoted to fully resolved. The check now requires:
  - the producing layer exists in cfg.layers
  - the symbol appears in the producing layer's
    `exports.symbols` list
  - if the producing layer has no exports declared, E0006
    fires with the hint: "declare `exports.symbols = [...]` on
    layer X".
  - on a typo, E0006 emits a Levenshtein "did you mean ...?"
    suggestion over the producing layer's `exports.symbols`.

The build-time symbol resolution (reading the layer's actual
listing) happens in manifest, not validate -- validate runs
without invoking tools. So E0006 in validate only checks the
declared shape; the actual symbol-not-in-listing case is a
runtime error in `manifest::LoadPlan::build` returning
`Error::cli` with E0006 in the message.

## Tests (write first)

Unit tests (in tests/manifest_unit.rs or a new
tests/cross_layer_patch_unit.rs):

- `cross_layer_symbol_resolves_through_listing`:
  - layer A: assembler kind, load.address = 0x000000,
    listing has `code_ptr` at 0x0010.
  - layer B: pcode kind, load.address = 0x010000,
    patches = [{ target = "A.code_ptr", value = "0x010000" }]
  - LoadPlan::build returns one ResolvedPatch with
    address = 0x0010, value = 0x010000.

- `cross_layer_value_address_resolves`:
  - layer A: assembler at 0x000000 with `code_ptr` at 0x0010
  - layer B: at 0x010000
  - patches = [{ target = "A.code_ptr",
    value = "B.address" }]
  - resolves to address = 0x0010, value = 0x010000.

Validate negative tests (in tests/validate_unit.rs):

- `e0006_symbol_not_in_exports`:
  - producing layer declares
    `exports.symbols = ["other_symbol"]`
  - downstream patch's symbol = "code_ptr"
  - validate emits E0006 with did-you-mean hint pointing at
    "other_symbol".

- `e0006_layer_has_no_exports_block`:
  - downstream patch references "A.code_ptr"
  - layer A has no `exports` field
  - E0006 with hint to declare exports.

- `e0006_unknown_referenced_layer`:
  - patch target "Q.x", layer Q does not exist
  - E0006 with hint listing available layer names.

## Pre-commit gate

```bash
cargo test
cargo clippy --all-targets --all-features -- -D warnings
cargo fmt --all
markdown-checker -f "**/*.md"
sw-checklist
```

## Done when

- The two manifest unit tests pass.
- Three validate negative tests pass with stable E0006.
- Phase 1's existing tests still pass (Scenario A has zero
  cross-layer patches; behavior is unchanged for it).
- Levenshtein suggestion text is included in E0006 messages.
- `agentrail complete --summary "..." --reward 1`.

## Stop after complete

Step 3 (pcode-image-layer) is a fresh session.
