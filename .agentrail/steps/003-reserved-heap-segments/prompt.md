# Step 3: reserved-heap-segments

Make non-embedded `kind = "heap" | "stack" | "bss"` segments
contribute to overlap accounting in `LoadPlan.segments` so the
existing E0003 catches collisions between reserved regions and
loaded artifacts. cor24-run zero-initializes any address not
covered by `--load-binary`, so the argv emitter doesn't need a
new flag.

## Inputs

- `docs/design.md` "Memory segments per layer" -- non-embedded
  segment shape with `load.address + size` (already in v1.2).
- `src/manifest.rs::resolve_segment` -- already produces a
  ResolvedSegment with `embedded = false` for non-embedded
  segments.
- `src/validate.rs` E0003 (overlap) and E0011 (in-region).

## What lands

src/manifest.rs
  - resolve_segment behavior unchanged for embedded segments.
    Non-embedded segments already produce a ResolvedSegment;
    confirm the start/end calculation uses load.address + size
    correctly.

src/validate.rs
  - E0003 expanded: include non-embedded segments in the global
    overlap check. Currently the overlap check considers
    memory_loads + embedded segments (resolved through
    listings). Non-embedded segments need to be in the same
    set, with their range = `(load.address, load.address +
    size)`.

## Tests

tests/manifest_unit.rs (+1 test)
  - reserved_heap_segment_appears_in_segments_not_loads:
      a layer with kind = "binary" and a segments[] containing
      one heap segment with embedded = false, size = 0x010000,
      load.address = 0x080000; LoadPlan.segments has one entry
      at [0x080000, 0x090000); LoadPlan.memory_loads has the
      binary's load_addr only.

tests/validate_unit.rs (+1 test)
  - e0003_reserved_heap_overlaps_loaded_binary:
      layer A binary at 0x080000 with size 0x008000;
      layer B has a reserved heap segment at 0x086000 size
      0x010000 -- they overlap [0x086000, 0x088000];
      validate fires E0003 with both layer names in the message.

## Pre-commit gate

Standard.

## Done when

- 2 new tests pass.
- Phase 2 corpus unchanged.
- `agentrail complete --summary "..." --reward 1`.

## Stop after complete

Step 4 (uart-chunk-composition) is a fresh session.
