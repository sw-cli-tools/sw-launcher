# Survey: sw-cor24-yocto-ed

A minimal modal text editor (gap buffer, 3-line UART display, edit + command
modes) written in C, cross-compiled with tc24r, and run on the cor24-run
emulator. It is a pure editor: there is NO "run" command, no embedded
assembler, and no compiler. Source typed in the buffer can only be saved
(currently a stub) or copied to a fixed memory address on quit; getting it
back out for assembly is an external host-side step. So this is not a
third "edit-and-run-in-emulator" pattern at all -- it is a "type into a
buffer, ship the bytes back over UART" pattern, with control-transfer left
to the host.

## 1. Run scripts / entry points

- `justfile:9-11` -- `just build`: `tc24r src/swye.c -o build/swye.s`.
  Single-translation-unit build: `swye.c` `#include`s buffer/command/editor/
  render `.c` files directly (`src/swye.c:47-50`).
- `justfile:13-14` -- `just run`: `cor24-run --run build/swye.s
  --load-binary tests/testfile.txt@0x10000 --terminal --echo --speed 0`.
  The "file to edit" is preloaded into emulator RAM at `0x10000`.
- `justfile:16-23` -- `demo-cursor`, `demo-cmd-mode`, `demo-edit-mode`:
  same launch with scripted UART input via `-u`.
- `justfile:25-26` -- `just test`: `reg-rs run -p yocto_ed`.
- Program entry: `src/swye.c:57` `int main(void)`.
- Alternative entry (legacy, unused by build): `src/main.c:12` -- separate
  multi-TU layout with `include/*.h`, kept around but not compiled by the
  current justfile.

## 2. Memory layout (text buffer, code, working memory)

From `src/swye.c`, `src/compat.h`, and `docs/design.md:122-128`:

| Region | Address | Source |
|---|---|---|
| Editor code | `0x000000` | `docs/design.md:124` |
| Pre-baked UART command stream | `0x0F0000` (`SYE_CMD_ADDR`) | `src/compat.h:21` |
| Buffer-out copy on quit | `0x0F0400` | `src/swye.c:80` |
| Loaded source / "initial text" | `0x010000` (`SYE_FILE_ADDR`) | `src/swye.c:53`; `justfile:14` `@0x10000` |
| Gap buffer storage | BSS, `g_work_buffer[4096]` | `src/swye.c:52,55,60` |
| Heap (per design note) | `0x020000` | `docs/design.md:126` |
| Stack | EBR / `0x040000` | `AGENTS.md:152`, `docs/design.md:127` |
| UART data MMIO | `0xFF0100` | `src/compat.h:1` |
| UART status MMIO | `0xFF0101` | `src/compat.h:2` |

Notable: the actively edited text lives in a 4 KB BSS array
`g_work_buffer` (`src/swye.c:52,55`), NOT at the conceptual `0x010000`
"source buffer" address. `0x010000` is only the read-only initial-text
region that `--load-binary` populates and that `sye_buffer_load_cstr`
copies INTO `g_work_buffer` via `sye_editor_init`
(`src/swye.c:60`, `src/editor.c:30-31`, `src/buffer.c:21-35`).

## 3. Transfer of control (edit -> run, if any)

There is no edit-to-run transition inside the emulator. The main loop
(`src/swye.c:63-78`) only dispatches edit-mode and command-mode keys
until `editor.quit_requested` is set by the `quit` command
(`src/command.c:78-80`, `src/editor.c:108-110`).

The only control transfer at all is on quit: `src/swye.c:80` does
`sye_buffer_copy_out(&editor.buffer, (char *)0x0F0400, 4096);` which
flattens the gap buffer into a contiguous C string at fixed address
`0x0F0400`, then `return 0`. There is no jump into the buffer, no call
to an assembler, no exec.

## 4. UART / editor command set

UART I/O (`src/compat.h:1-19` and `src/platform_uart.c:3-21`):
- `0xFF0101 & 0x80` -- TX busy, spin while set.
- `0xFF0101 & 0x01` -- RX ready, spin until set.
- `0xFF0100` -- read (RX) or write (TX) one byte.

Input source has a quirky pre-script feature
(`src/compat.h:23-34`): `sye_getc()` first drains a NUL-terminated
command string at `SYE_CMD_ADDR = 0x0F0000`, only then falls through to
real UART. A host can stage canned keystrokes there.

Edit-mode keys (`src/editor.c:35-66`):

| Key | Action |
|---|---|
| ESC (27) | switch to command mode |
| `\n` / `\r` | insert newline |
| BS (8) / DEL (127) | delete-backward |
| printable 32-126 | insert char |

Command-mode keys (`src/editor.c:68-114`): characters accumulate into
`editor->command_buffer` (cap 128, `src/swye.c:13`), Enter executes,
Backspace edits, ESC or `C-]` (29) cancels back to edit mode.

Command grammar: `[count] command [args]` -- parsed in
`src/command.c:8-47`, executed `repeat_count` times in
`src/command.c:89-105`.

Implemented commands (`src/command.c:50-86`):

| Command | Effect |
|---|---|
| `left` / `right` | shift gap (cursor) one char (`buffer.c:90-108`) |
| `del` / `backdel` | delete forward / backward (`buffer.c:72-88`) |
| `newline` | insert `\n` |
| `insert TEXT` | insert literal arg string (`buffer.c:59-70`) |
| `save` | sets `dirty=0`, writes status `"saved (stub)"` -- no I/O |
| `new` | reinit buffer (clear) |
| `quit` | sets `SYE_STATUS_QUIT` -> main loop exits |
| `help` | writes a help message to status buffer |

`docs/design.md:42-58` documents additional commands (`up/down`,
`bol/eol`, `bob/eob`, `fwdw/backw`, `delline`, `goto N`) but these are
NOT in `src/command.c` -- design overshoots implementation.

There is NO `run`, NO `assemble`, NO `eval` command.

## 5. Heaps and stacks

- No dynamic allocation in source. All buffers are static or
  stack-resident.
- The gap buffer's storage is the BSS array `g_work_buffer[4096]`
  (`src/swye.c:55,60`); inside it, the gap is `[gap_start, gap_end)`
  (`src/buffer.c:10-19`).
- `command_buffer[128]` and `status_buffer[128]` live inside
  `struct sye_editor` (`src/swye.c:34-37`), which is a stack local in
  `main` (`src/swye.c:58`).
- Render line buffers `text_line[256]`, `mode_line[256]`,
  `command_line[256]` are stack locals in `sye_editor_render`
  (`src/editor.c:117-119`).
- "Heap (gap buffer) at `0x020000`" and "Stack at `0x040000`" are
  documented as conceptual layout (`docs/design.md:126-127`,
  `AGENTS.md:152`) but the code as compiled does not place storage
  there explicitly -- it relies on tc24r's defaults plus BSS placement.

## 6. Build artifacts vs vendored

| Path | Status | Note |
|---|---|---|
| `build/swye.s` | gitignored (`.gitignore:1`) | tc24r output, present locally |
| `work/reg-rs/` | gitignored (`.gitignore:2`) | reg-rs golden DB, 7 test cases (`yocto_ed_backdel`, `cmd_mode`, `cursor_right`, `edit_mode`, `edit`, `insert`, `smoke`) |
| `.agentrail/sessions/`, `.agentrail/trajectories/` | gitignored (`.gitignore:3-4`) | -- |
| `src/main.c` | tracked but unused | older multi-TU entry; current build uses `src/swye.c` |
| `include/*.h` | tracked but unused by current build | five headers (`buffer.h`, `command.h`, `editor.h`, `platform.h`, `render.h`) for the `main.c` path |
| `src/platform_uart.c` | tracked but unused | duplicated by inline `compat.h` UART code |
| `tests/testfile.txt` | tracked, 93 bytes, 3 lines | preloaded at `0x10000` for runs |

So the repo has a ghost dual structure: a clean multi-TU `main.c` +
`include/` + `platform_uart.c` design, and an actually-used
single-TU `swye.c` + `compat.h` + sibling `.c` includes design. Only the
latter is referenced by the justfile.

## 7. Edit-and-run model

This is the load-bearing question and the answer is: yocto-ed is a
**pure editor**, not an "edit-and-run" environment. Specifically:

- **No `run` command exists.** The full command vtable is in
  `src/command.c:50-86`; nothing in it builds, assembles, or jumps to the
  buffer.
- **No assembler is linked in.** The toolchain (`tc24r`, `cor24-run`) is
  applied at host build time, not at runtime. The only thing the running
  editor knows how to do with the buffer is render it, mutate it, and
  on quit copy it to `0x0F0400`.
- **`save` is a stub.** `src/command.c:68-72` clears the dirty flag and
  prints "saved (stub)". There is no UART dump-on-save and no MMIO write.
- **Source lifecycle across quit**: the editor only "exports" the buffer
  on `editor.quit_requested` becoming true (`src/swye.c:80`). The export
  is a memcpy from gap buffer to `(char *)0x0F0400`, and then `return
  0`. The host (`cor24-run`) is expected to be the one that does
  anything useful with that region -- e.g. by inspecting emulator memory
  after exit.
- **Source lifecycle across boot**: source comes IN via
  `--load-binary tests/testfile.txt@0x10000` (`justfile:14`), which the
  emulator places at `0x10000`. `sye_editor_init` then copies it into
  `g_work_buffer` (`src/swye.c:60`, `src/editor.c:30-31`,
  `src/buffer.c:21-35`). So the canonical buffer of record while
  editing is `g_work_buffer` in BSS, not `0x10000`.

Comparison to the launcher's two known patterns:
- It is NOT "one-shot run-script": yocto-ed is a long-lived interactive
  REPL-style program over UART.
- It is NOT "monitor with paste-and-go": it has no command that takes
  the buffer and executes / assembles it.
- The closest analogy is "editor as a remote dumb terminal target"
  (compare `docs/future-integrations.md` -- the design explicitly
  imagines a host-side Rust UART bridge or Emacs TRAMP-style mode
  driving it). Transfer-of-control is **owned entirely by the host**.

So if the launcher needs a third pattern for yocto-ed, it is "host
drives an interactive editor over UART; control transfer happens
host-side after the editor quits, by reading emulator memory at
`0x0F0400`." It is not a self-contained edit-then-run target.

## Schema gaps for this repo

- **No "run" / "exec" / "assemble" command.** The launcher's existing
  schema fields for run-command-name and run-trigger keystroke do not
  apply here. May want a nullable `run_command: null` or an explicit
  `mode: editor-only` discriminator.
- **Buffer lives in BSS, not at the documented heap address.**
  `docs/design.md:122-128` says heap is at `0x020000` and stack at
  `0x040000`, but the actual gap-buffer storage is
  `g_work_buffer[4096]` placed wherever tc24r/linker drops BSS. If the
  schema records "where edited source lives," recording the *symbol*
  `g_work_buffer` is more truthful than recording an address.
- **Source-in vs source-out asymmetry.** Source enters at `0x10000`
  (load-binary), is edited in BSS, and exits at `0x0F0400` (copy-out).
  These are three different addresses for the "same" data at different
  lifecycle stages -- the schema may need separate
  `source_load_addr`, `source_edit_addr`, `source_export_addr` slots.
- **Pre-baked UART command stream at `0x0F0000`.** The `sye_cmd_ptr`
  pre-script mechanism (`src/compat.h:21-34`) is a launcher-relevant
  hook (drive the editor with a canned keystroke string in memory
  before falling through to real UART) and has no analogue in the
  surveyed peer repos so far.
- **`save` is a stub, no host file-out path.** Schema field for
  "how does the program persist work?" should be `none` or `stub` for
  this repo, distinct from "writes to UART," "writes to disk via host
  hook," etc.
- **Documented command set vs implemented command set diverge.**
  `docs/design.md:42-58` lists ~14 commands; `src/command.c` implements
  9. If the schema is fed from docs, it will be wrong; should be fed
  from source.
- **Two parallel source layouts.** `src/main.c` + `include/*.h` +
  `src/platform_uart.c` describe one entry point; `src/swye.c` +
  `src/compat.h` plus `#include`d `.c` files describe the actual built
  one. The schema needs to mark only one as authoritative
  (the latter -- it is what `justfile:11` compiles).
