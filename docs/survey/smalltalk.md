# Survey: sw-cor24-smalltalk

A tiny "Tinytalk" Smalltalk-v0 hosted entirely in COR24 BASIC v1.
The repo ships a BASIC VM core (`src/vm.bas`, 465 lines) plus a
host-side `.st` -> BASIC compiler (`tools/stc.awk`, 746 lines).
Demos are built by concatenating a generated image header, the VM,
and an optional hand-written driver into one `.bas` file under
`build/`, then handing that to the sibling `sw-cor24-basic` runner
which executes it on the COR24 p-code emulator. Layered runtime:
`.st` -> BASIC source -> BASIC interpreter -> p-code -> COR24
emulator.

## 1. Run scripts

Three shell wrappers in `scripts/`. None invoke the emulator
directly; they all `exec` the sibling
`../sw-cor24-basic/scripts/run-basic.sh` which is the actual
p-code/emulator entry. The compile step (`.st` -> `.bas`) only
happens on the host -- it is `awk`, not COR24 code.

- `scripts/run-bare.sh` (run-bare.sh:20). Passes a single `.bas`
  straight through with no concatenation. Used only by
  `examples/smoke/*.bas` substrate sanity tests.
- `scripts/run-st.sh` (run-st.sh:34-37). Pipeline: `tools/stc.awk
  < X.st > build/X_compiled.bas`, then `cat build/X_compiled.bas
  src/vm.bas > build/X_full.bas`, then `printf 'RUN\nBYE\n' >>`,
  then run-basic. The compiler emits a complete program (driver
  stub + image bootstrap + main bytecode DATA), so `.st` files
  alone produce a runnable demo. Default for v1-dialect demos.
- `scripts/run.sh` (run.sh:16-17). Calls `scripts/build.sh
  <demo>` then run-basic on `build/<demo>.bas`. Used for legacy
  demos that need a hand-written `.bas` driver
  (`examples/d5_calc.bas`, `examples/d8_step.bas`).
- `scripts/build.sh` (build.sh:1-95). The legacy assembler.
  Compiles the `.st` in `MODE=methods_only` (build.sh:40), so the
  driver stub and main DATA are suppressed and the
  `examples/<demo>.bas` driver supplies the top level. Falls
  back to hand-written `src/image_<demo>.bas` if no `.st` exists
  (build.sh:45-55, files not present in current tree). Order of
  concatenation: `IMG VM DRV` (build.sh:75). Test-transcript
  splice between `RUN` and `BYE` for `INPUT`-driven REPLs
  (build.sh:81-93).

No top-level `justfile`, `Makefile`, or `build.sh`; everything is
under `scripts/`.

## 2. Memory loads

Memory population is BASIC `DIM` arrays, not raw `PEEK`/`POKE`
(after FR-1 dogfood, README.md:113). Singletons and method dict
are installed by GOSUBs in `src/vm.bas`:

- `INSTALL_SINGLETONS` at vm.bas:100 (line 10100). DIMs `H(511)`
  heap, `S(127)` eval stack, `O(127)` bytecode pool,
  `P/M/L/R/Y(15)` frame stack, `C/G/A/B(15)` method dict, `K(15)`
  class super, `T(15)` something. POKEs nil/true/false at heap
  addresses 0/4/8 (vm.bas:78-89). Sets `H=16`.
- `read_and_install_methods` at line 10800 (referenced from
  generated header, build/d1_add_compiled.bas:23 calls
  `GOSUB 10800`). Reads method-dict DATA records of the form
  `<class>, <selector>, <bclen>, <bytes...>` then `-1, 0, 0`
  terminator (build/image_d1_add.bas:7-8 shows
  `DATA 2,1,3, 13,1,8` = SmallInteger class, selector +, length
  3, bytecode `PRIMITIVE 1, RETURN_TOP`).

The generated driver's `RESTORE 600 / FOR I=0 TO 9 / READ V /
LET O(15+I)=V` (build/d1_add_compiled.bas:7-11) loads main
bytecode straight into the bytecode-pool array `O()` at offset
15, then sets `M=15 L=10 P=0 R=0` and `GOSUB 12000` (the
dispatch loop entry, build/d1_add_compiled.bas:12-16).

## 3. Patches

None. There is no late patch / poke-to-fix step in any run
script. The VM image is fully generated from `.st` source plus
`vm.bas`; nothing is mutated post-load.

## 4. UART payload structure

Not applicable here. `run-st.sh` and `run.sh` invoke
`../sw-cor24-basic/scripts/run-basic.sh` and have no awareness
of UART, .pcd, or hex packing. The BASIC repo is responsible
for whatever it does to feed `pv24t`. From this repo's
perspective the "payload" is a single `build/<name>_full.bas`
text file, ~498 lines for D1 (`build/d1_add_full.bas`).

## 5. Heaps and stacks

All five live as `DIM` arrays inside the running BASIC program;
sized at `INSTALL_SINGLETONS` (vm.bas:104-108):

- Object heap: `H(0..511)` -- 512 words, bump-allocated by
  scalar `H` (vm.bas:79-87, allocator at 9700). Header is
  3 words (class, size, format) then payload.
- Eval stack: `S(0..127)` -- Smalltalk operand stack, `EPUSH`
  vm.bas:88-93, `EPOP` vm.bas:94-99.
- Frame stack: parallel arrays `P/M/L/R/Y(0..15)` -- 16 frames
  max, holding saved PC, method addr, method len, receiver,
  cleanup target (vm.bas:12-15, 105).
- Method dictionary: parallel arrays `C/G/A/B(0..15)` -- 16
  entries: class, selector, bcstart, bclen (vm.bas:16-17, 106).
- Class super chain: `K(0..15)`, `K(0)=-1` terminator
  (vm.bas:50, 107, 121).
- Bytecode pool: `O(0..127)` -- 128 bytes shared by all
  methods (vm.bas:51-53, 104).

VM registers are BASIC scalars `A..Z` (architecture.md:88-113).
Same single-letter name can be both scalar `X` and array `X()`
because BASIC v1 distinguishes them (vm.bas:7-9). The Smalltalk
call stack is *not* the BASIC `GOSUB` stack
(architecture.md:64-66).

## 6. Build artifacts vs vendored

- Vendored (in git): `src/vm.bas`, `tools/stc.awk`, all
  `examples/*.st`, the two driver `.bas` files
  `examples/d5_calc.bas` and `examples/d8_step.bas`, and four
  smoke programs under `examples/smoke/`. Confirmed via
  `git ls-files`.
- Generated (gitignored, .gitignore:2): everything under
  `build/`. For each demo `X` you get up to four files:
  `build/X_compiled.bas` (stc output, full driver), `build/
  X_full.bas` (compiled + vm.bas + RUN/BYE), `build/X.bas`
  (legacy build.sh path, image+vm+driver), and `build/image_X.
  bas` (methods-only image header). Lengths for D1: 31 / 498 /
  428 / 8.
- External vendored: `../sw-cor24-basic` (run-st.sh:16-21
  errors out if missing) and transitively `../sw-cor24-pcode`,
  `../sw-cor24-emulator` (README.md:181-186). Treated as a
  fixed substrate; this repo contains no C/Python/Rust
  (README.md:187).

## 7. Known limits

From design.md and README.md status section:

- Heap 512 words, eval stack 128, frame stack 16, method dict
  16, bytecode pool 128 bytes, class table 16. All hard-sized
  arrays (vm.bas:104-108).
- Methods longer than 256 bytes unsupported (design.md:172).
- 14 opcodes, 6 primitives (README.md:58-62), selector ids
  hardcoded in `tools/stc.awk` (stc.awk:52-77, 27 selectors).
- Block evaluation is eager in v0; lazy blocks need real
  closures, out of scope (README.md:67-68).
- `STOP` was fatal in v1 but FR-6 added `CONT`; D8 stepper
  uses scalar `J=1` flag (README.md:97, 119).
- Class table holds 16 slots; user classes start at id 10
  (design.md:128-130). `tools/stc.awk` hard-codes Counter=10,
  BoundedCounter=11, ... Program=15 (stc.awk:95-100). Adding
  a user class beyond 15 requires editing the compiler.
- BASIC v1 program area 16384 bytes; full demo must fit
  (design.md:30).

## Schema gaps for this repo

A launcher schema that only models "memory loads + UART
payload" would miss everything load-bearing here. Concrete
needs:

1. Pre-flight host compile step. `tools/stc.awk` runs on the
   host before any COR24 code executes; the schema must
   express "compile X.st with awk script Y to artifact Z" as a
   first-class build phase, not an external prerequisite.
2. Multi-source concatenation with order. `cat IMG VM DRV` and
   the `_compiled + vm + RUN/BYE` recipe both depend on
   line-number ranges that overlap unless concatenated in the
   right order (architecture.md:188-208). Schema must capture
   ordered file list, not a set.
3. Mode flags on the compiler. `stc.awk` has `MODE=methods_only`
   that changes the output structure (build.sh:40). Generic
   "run a script" is not enough; per-target compiler args
   are part of the build contract.
4. Optional input-transcript splice between sentinel lines
   (build.sh:81-93). The launcher needs a notion of "if file
   T exists, splice at marker M" -- distinct from a normal
   build step because it depends on a runtime test artifact.
5. Sibling-repo dependency by relative path. Both run scripts
   resolve `$REPO_DIR/../sw-cor24-basic` and bail if absent
   (run-st.sh:14-21). Schema needs a typed "delegate to
   sibling launcher" handoff so the BASIC-level launcher
   owns the actual emulator invocation.
6. No memory-load / patch / UART concepts apply here at all;
   the schema must allow those layers to be empty for repos
   like this and present for lower layers (the BASIC repo).
7. Multiple run modes per repo (`run-bare.sh` for raw
   BASIC, `run-st.sh` for `.st`, `run.sh` for legacy
   `.bas`-driver demos). One repo, three target shapes;
   schema needs target kinds, not one-target-per-repo.
