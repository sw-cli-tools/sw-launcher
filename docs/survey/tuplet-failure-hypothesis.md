# Tuplet failure hypothesis

The user reports that `~/github/sw-vibe-coding/tuplet` currently
fails where the rest of the corpus works. This doc states the
likely failure mode based on the survey, identifies the precise
script lines that introduce the risk, and names the `sw-launch`
validation rule that would have caught it.

## What the script does

`scripts/run-ml-memory.sh` (lines 106-117):

```
cor24-run \
  --load-binary "${build_dir}/pvm.bin@0" \
  --load-binary "${build_dir}/ocaml.p24m@0x040000" \
  --load-binary "${input_image}@0x080000" \
  --patch "0x${code_ptr}=0x040000" \
  --patch "0x${heap_limit}=0x03F000" \
  --entry 0 -u "${uart_input}" --speed 0 -n 3000000000
```

`code_ptr` and `heap_limit` are read from
`<ocaml-repo>/build/code_ptr_addr.txt` and
`<ocaml-repo>/build/heap_limit_addr.txt` (lines 90-91).

## The exposed contract

For this layout to be safe at runtime, three address invariants
must all hold:

1. **pvm.bin** occupies `[0x000000, end_of_pvm)` where
   `end_of_pvm <= 0x03F000`. The OCaml interpreter's downward-
   growing heap then has the corridor
   `[end_of_pvm, 0x03F000)` to work in.
2. **OCaml heap** never grows past `0x03F000` *and* never below
   `end_of_pvm`. The script patches only the *upper* bound
   (`heap_limit = 0x03F000`); the lower bound is whatever the
   runtime decides at startup, presumably `end_of_pvm` rounded.
3. **ocaml.p24m** occupies `[0x040000, 0x040000 + image_size)`
   where the upper bound is below `0x080000` (the input image
   base).

## Where the contract can break

There are four candidate failure modes, each of which the survey
points at:

### F1. pvm.bin grew past the heap floor

If the vendored `pvm.bin` (sw-cor24-pcode v0.1.0 in
`vendor/sw-pcode/v0.1.0/bin/pvm.s`) has been rebuilt with more
features, embedded eval/call/heap segments could push
`end_of_pvm` past whatever floor the OCaml runtime infers. The
script does not validate this; it just patches `heap_limit` to
`0x03F000` and trusts the vendored pvm to fit below.

**Catching rule**: E0003 (memory range overlap), made
segment-aware so embedded segments inside `pvm.bin` are
counted. The pvm's last embedded segment's end address
must be strictly less than the OCaml heap's floor.

### F2. OCaml heap floor and the OCaml image collide

The `0x03F000` ceiling sits 4 KiB below `0x040000`. If the
*floor* of the heap is set high enough that the OCaml runtime
runs out of corridor before reaching `0x03F000`, the runtime
TRAPs. If the floor is set too low and the heap pushes through
`end_of_pvm`, it corrupts the VM. Either case manifests as a
"silent miscompute then HALT" or as a TRAP from pvm.

**Catching rule**: E0011 / new error code for "reserved heap
segment must declare both a floor and a ceiling" -- the
limit-only patch is allowed, but the floor must be derivable
from a declared symbol or address. The schema gap in
`docs/survey/schema-gaps.md` section B3 covers this.

### F3. ocaml.p24m exceeds 0x040000 + 256 KiB

The image base is `0x040000`. The data layer is at `0x080000`.
That gives the interpreter image a maximum of 256 KiB. As the
OCaml interpreter grows (more built-ins, more pattern-matching
cases, etc.) this ceiling tightens. Without an explicit upper
bound check, an oversize `ocaml.p24m` overlaps the input
image and the interpreter reads garbage.

**Catching rule**: E0003 (segment-aware overlap) again, this
time between the `ocaml.p24m` code+data extent and the
`input_image` layer's start.

### F4. Stale resolved-address sidecars

`code_ptr_addr.txt` and `heap_limit_addr.txt` are written by
`<ocaml-repo>/scripts/build.sh` (lines 70-81 of that file)
*at OCaml repo build time*. If the OCaml repo has been
rebuilt since the user last `cd`'d there, but the tuplet
script reads stale `.txt` files (e.g. because they're cached
in a Nix store, a CI artifact dir, or a path the user thinks
is current but isn't), the patches go to the wrong addresses.
Symptom: silent corruption, TRAP, or HALT after a few cycles.

**Catching rule**: a new schema feature (gap B1, "build-time
resolved sidecars") that records the sidecar's mtime / hash in
the lockfile and refuses a stale read.

## Most likely failure right now

Given the user's report ("the others work, this one doesn't"),
F1 or F4 is most likely. F2/F3 would also have broken
sw-cor24-ocaml's own demos, which the user says still work; F1
might break tuplet alone if tuplet's input image at `0x080000`
plus the OCaml heap pushing down were sized to fit *just* below
the pvm-without-feature-X version, and a recent pvm rebuild
ate the slack.

Recommended next debug step (out of scope for this survey, but
documented for the next saga step): run

```
cd ~/github/sw-embed/sw-cor24-ocaml
just build
cat build/code_ptr_addr.txt build/heap_limit_addr.txt
ls -la vendor/sw-pcode/v0.1.0/bin/pvm.s
grep -E "^heap_seg:|^eval_stack:|^call_stack:" build/pvm.lst | tail -20
```

and confirm that the resolved `heap_limit` is still safely above
`end_of_pvm`.

## What sw-launch does about it

`sw-launch check tuplet` (after Phase 1) will:

1. Resolve `pvm.bin`'s last embedded segment's end address from
   `pvm.lst`.
2. Resolve `ocaml.p24m`'s declared end address from its v2 P24
   header `code_size + data_size`.
3. Read `code_ptr_addr.txt` and `heap_limit_addr.txt` and
   require them to lie inside `pvm.bin`'s data segment.
4. Reject (E0003) any overlap between pvm extent + OCaml heap
   corridor + ocaml.p24m extent + input_image extent + EBR +
   MMIO.
5. Warn (new error code) when adjacent layers' guard size is
   smaller than a configured minimum (default 1 KiB).

A `sw-launch check` failure here is a precise, actionable
diagnostic. A failure today is a TRAP message and a guess.
