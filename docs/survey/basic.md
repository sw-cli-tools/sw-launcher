# Survey: sw-cor24-basic

COR24 BASIC v1 is a Pascal-implemented, line-numbered, integer-only BASIC interpreter that runs on the COR24 p-code VM. The build pipeline is unusual: a hosted COR24 emulator run (`cor24-run` / `cor24-emu`) executes a pre-assembled Pascal compiler (`p24p.bin`) with the BASIC source piped in via UART, producing `.spc` output that is then linked (`pl24r`), patched with sed, and assembled (`pa24r`) to `build/basic.p24`. Every demo runs the resulting `.p24` not on COR24 itself but on the host-side p-code trace interpreter `pv24t`, with the `.bas` program piped in on stdin/`-i` as UART input. There is no `vendor/` directory and no `active.env`/`version.json`; sibling tool binaries are referenced by relative path (`../sw-cor24-pcode/target/release/`, `../sw-cor24-pascal/`, `../sw-cor24-emulator/target/release/`) at floating HEAD. The repo's only on-disk artifacts are `build/basic.p24` (final p-code image) and `build/p24p.bin` (cached COR24-native compiler binary). See `/Users/mike/github/sw-embed/sw-cor24-basic/README.md`, `/Users/mike/github/sw-embed/sw-cor24-basic/scripts/build-basic.sh`, and `/Users/mike/github/sw-embed/sw-cor24-basic/docs/architecture.md`.

## 1. Run scripts

Build script:
- `scripts/build-basic.sh` (lines 28-30):
  `"$EMU" --load-binary "$P24P_BIN@0" --entry 0 --stack-kilobytes 8 -u "$(cat "$REPO_DIR/src/basic.pas")"$'\x04' --speed 0 -n 2000000000`
  where `$EMU` is `../sw-cor24-emulator/target/release/cor24-emu` if built, else `cor24-run` on PATH. Also at line 23: `cor24-run --assemble "$P24P_S" "$P24P_BIN" /dev/null` to (re)assemble `../sw-cor24-pascal/compiler/p24p.s` to `build/p24p.bin`.

Generic runner:
- `scripts/run-basic.sh` (line 29): `"$PV24T" "$P24" -i "$INPUT" -n "$BASIC_MAX_INSN"` with `PV24T=../sw-cor24-pcode/target/release/pv24t`, `P24=build/basic.p24`, `INPUT="$(cat "$BAS")"$'\n\x04'`, default `BASIC_MAX_INSN=0` (unlimited).

Demo wrappers (all under `scripts/demo-*.sh`):
- Thin wrappers that just `exec run-basic.sh examples/<name>.bas`: `demo-hello.sh`, `demo-fibonacci.sh`, `demo-factorial.sh`, `demo-count.sh`, `demo-fizzbuzz.sh`, `demo-calc.sh`, `demo-memdump.sh`, `demo-bitwise.sh`, `demo-cont.sh`, `demo-data.sh`, `demo-dim.sh`, `demo-mod.sh`, `demo-on.sh` (each is 5-6 lines; same single `pv24t` invocation as `run-basic.sh`).
- `scripts/demo-startrek.sh` (line 20): `{ cat "$BAS"; cat; } | "$PV24T" "$P24" -n 0` (concatenates the .bas with live stdin for interactive play).
- `scripts/demo-trek-adventure.sh` (line 22): `"$PV24T" "$P24" -n 0 -i "$(cat "$BAS")"`.
- `scripts/demo-guess.sh` (line 19): `"$PV24T" "$P24" -n 0 -i "$(cat "$BAS")"`.
- `scripts/demo-robot-chase.sh` (line 27): `"$PV24T" "$P24" -n 0 -i "LET R=$SEED"$'\n'"$(cat "$BAS")"` (PRNG seed prepended).

`examples/blink.bas` has no wrapper; its target is `0xFF0000` MMIO which `pv24t` cannot service (see `docs/demos.md` lines 116-129).

## 2. Memory loads

Build-time loads into `cor24-emu` (only step that talks to COR24):

| layer-name | path | address | source | rationale |
|---|---|---|---|---|
| p24p compiler | `build/p24p.bin` | `0` (`@0`) | sibling repo `../sw-cor24-pascal/compiler/p24p.s` re-assembled if newer | The Pascal compiler is COR24-native; loaded at 0 so `--entry 0` reaches its reset vector. |

Run-time loads into `pv24t` (host p-code VM, no flag-driven layout):

| layer-name | path | address | source | rationale |
|---|---|---|---|---|
| basic interpreter | `build/basic.p24` | implicit | local build product | `pv24t` takes a single `.p24` positional arg; address layout is owned by the VM, not the launcher. |

No `--load-binary` is used at demo run time. `pv24t` exposes no `--load-binary @addr` syntax in any script.

## 3. Patches

No `--patch <addr>=<value>` invocations appear in any script. The only patching done in this repo is a textual `sed` rewrite of the linked `.spc` (build-basic.sh lines 52-55) to swap `p24p`'s `readln` lowering for a single `sys 2` (GETC) call inside `_user_read_line`. That is not a memory patch and has no analogue in the launcher schema.

## 4. UART payload structure

Two distinct payloads, both fed as a single concatenated string:

Build (one-shot compile, build-basic.sh line 29):
- Order: full contents of `src/basic.pas` followed by a literal `\x04` (EOT).
- Terminator: `\x04` (EOT). `cor24-run -u` accepts the entire string up front; the EOT signals end-of-file to the running compiler.
- Cited: `-u "$(cat "$REPO_DIR/src/basic.pas")"$'\x04'`.

Run (run-basic.sh lines 24, 29):
- Order: contents of `<file>.bas`, then a newline, then `\x04`.
- Terminator: `\x04` (EOT) after a trailing `\n`. The newline ensures the final BASIC line (typically `BYE`) is line-terminated; without it, `read_line` blocks waiting for the rest of the line and the run hangs.
- Cited: `INPUT="$(cat "$BAS")"$'\n\x04'` then `"$PV24T" "$P24" -i "$INPUT"`.

Special cases: `demo-startrek.sh` line 20 substitutes piped stdin (`{ cat "$BAS"; cat; }`) for the `-i` flag so the user can keep typing after the program is loaded; `demo-robot-chase.sh` line 27 prepends `LET R=<seed>\n` before the `.bas` to seed the PRNG.

## 5. Heaps and stacks

The interpreter's BASIC-level stacks (GOSUB/FOR) and program/variable storage are statically declared as Pascal arrays inside `src/basic.pas` (lines 4-22). They are not loaded by the launcher and have no `--load-binary` symbol; they live in the p-code VM's globals/heap area at whatever offsets `p24p`/`pa24r` assign:
- `pg : array[0..16383] of char` -- program area (`PS=16384`, packed sorted tokenized lines), grows up from offset 0 in the array.
- `gs : array[0..63] of integer` -- GOSUB return-pointer stack, depth 64, grows up via `gp` index.
- `fv/fl/fs/fr : array[0..15] of integer` -- FOR-stack quartets, depth 16, grow up via `fp` index.
- `vars : array[0..25] of integer` -- A..Z scalar table.
- `apool : array[0..1023] of integer` -- DIM array pool (`AS=1024`), bumped via `apsp`.
- `lb : array[0..79] of char`, `tb : array[0..127] of integer` -- input/token line buffers.
Growth direction: all up (Pascal array indices monotonically increase in the push direction). See `docs/architecture.md` lines 60-87 and `docs/design.md` lines 408-448 for the conceptual layout; the actual sizes live in `src/basic.pas` line 3.

Build-time COR24 stack: `--stack-kilobytes 8` (build-basic.sh line 28) gives the running `p24p` compiler an 8 KiB COR24-native stack; the emulator chooses the address (top-of-RAM convention, downward growth). This is the only stack the launcher actually configures.

`pv24t` is invoked with no stack/heap flags in any demo script; the host VM owns its own layout.

## 6. Build artifacts vs vendored

Built locally:
- `build/basic.p24` -- final p-code image, produced by `build-basic.sh` from `src/basic.pas` via the `p24p -> pl24r -> sed -> pa24r` chain.
- `build/p24p.bin` -- cache of the COR24-native Pascal compiler, re-assembled by `cor24-run --assemble` only when `../sw-cor24-pascal/compiler/p24p.s` is newer (build-basic.sh lines 22-24).

Consumed from sibling repos at floating HEAD (no version.json, no active.env, no vendor dir):
- `../sw-cor24-pcode/target/release/pl24r` (linker)
- `../sw-cor24-pcode/target/release/pa24r` (assembler)
- `../sw-cor24-pcode/target/release/pv24t` (host p-code interpreter; the actual runner)
- `../sw-cor24-pascal/compiler/p24p.s` (Pascal compiler source)
- `../sw-cor24-pascal/runtime/runtime.spc` (Pascal runtime, linked in)
- `../sw-cor24-emulator/target/release/cor24-emu` (or `cor24-run` on PATH as fallback)

Version pins: none. Every dependency is "whatever HEAD is in the sibling working tree." `README.md` line 71-73 and `docs/basic-interpreter-modules.md` line 84-87 cite cross-repo issue numbers (e.g. `sw-cor24-pascal#2`) but no commit hashes or tags are recorded.

## 7. Known limits

- Source ceiling: `p24p` UART input buffer = 16384 bytes (`README.md` lines 71-73). Bumped from 8192 by `sw-cor24-pascal#2`. The whole interpreter still has to fit; future "unit build" (steps 018-019) is queued to remove this.
- Build cycle cap: `-n 2000000000` instructions for the compile-via-emulator step (build-basic.sh line 30); `--speed 0` means run as fast as possible (no rate limiting).
- Build stack: `--stack-kilobytes 8` for the p24p compiler run (build-basic.sh line 28).
- Run cycle cap: `pv24t -n 0` (unlimited) is the default for every demo and for `run-basic.sh` (run-basic.sh line 28: `: "${BASIC_MAX_INSN:=0}"`); interactive demos require this so they can wait on input. Override with `BASIC_MAX_INSN=<N>` env var.
- Program area: 16384 bytes packed tokenized lines (`PS=16384`, `src/basic.pas` line 3).
- DIM pool: 1024 integers shared across all DIM arrays (`AS=1024`, same line; `docs/design.md` lines 275-283).
- Stacks: GOSUB depth 64, FOR depth 16, both fixed at compile time (`docs/design.md` lines 484, 514).
- String literal length: 255 chars (`docs/design.md` line 114).
- Integer width: signed 24-bit (`docs/design.md` line 304); 12! is the largest factorial that fits (`docs/demos.md` line 92).
- 24 keywords in `ik` table; ABS tokenization bug tracked as `sw-cor24-basic#1` (`docs/demos.md` lines 60-65).
- `pv24t` linear memory does not include `0xFF0000+` MMIO, so `examples/blink.bas` traps under `pv24t` (`docs/demos.md` lines 116-129).

## Schema gaps for this repo

`docs/design.md` (sw-launcher) currently only models scenarios where `cor24-run`/`cor24-emu` is the runner and `--load-binary @addr` / `--patch addr=val` carry the layout. This repo breaks that mold in three ways the schema must absorb without loss:

1. Pluggable runner per scenario. The build invokes `cor24-emu`; every demo invokes `pv24t`. These take different positional/flag conventions: `pv24t <p24-path> [-i <input>] [-n <cycles>]` has no `--load-binary`, no `--patch`, no `--entry`, no `--speed`. Add a `runner` field on `[scenarios.X]` (e.g. `runner = "pv24t"` vs `runner = "cor24-emu"`) and let each runner declare which load primitives it accepts. Memory-layer tables become optional when the runner is `pv24t`.
2. Compile-by-emulation as a build step. `build-basic.sh` runs `cor24-emu` itself as part of building `basic.p24`: load `p24p.bin@0`, feed `basic.pas` plus EOT on UART, capture UART output, sed-massage it, link, assemble. The schema needs a `[builders.<name>]` concept that can express "run scenario S, capture UART, post-process, emit artifact A" -- not just "shell out to a tool." Today's `kind = "assembler"` layer model is a one-step transform; this is a multi-step pipeline whose first step is itself a launcher-managed run.
3. UART-as-payload at build time. Distinguish two different UART roles: (a) feeding source code to a compiler running inside the emulator (build-basic.sh `-u` payload, terminated by `\x04`); (b) feeding a `.bas` program to a compiled interpreter at run time (run-basic.sh `-i` payload, terminated by `\n\x04`). Both need a `terminator` field (`"eot"`, `"newline+eot"`, `"none"`) and a flag for whether the payload is fed via `-u`/`-i` or piped on stdin (the startrek demo's `{ cat; cat; }` pattern).

Other smaller gaps:
- Express `--stack-kilobytes N` as a first-class field rather than a free-form arg (build-basic.sh uses 8).
- Express "unlimited cycles" cleanly. `pv24t -n 0` and `cor24-emu -n 2000000000` both mean "don't cap me" but the encodings differ; an env-overridable `max_cycles` (cf. `BASIC_MAX_INSN`) is also needed.
- Allow seed/preamble injection (robot-chase prepends `LET R=$SEED\n`); this is conceptually a synthetic UART layer ahead of the real payload.
- Express vendored-by-sibling-path as a source kind. There is no `vendor/`, no `active.env`, no version pins -- dependencies are bare `../sibling/target/release/<bin>` paths that the schema should be able to resolve (and warn about) without inventing a phantom version.
- Allow a sed-style post-processing step on intermediate artifacts (the `.spc` GETC patch in build-basic.sh lines 52-55), or model it as an explicit "rewrite" layer with a recorded rule rather than embedding sed scripts in build code.
