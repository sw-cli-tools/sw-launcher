# Step 1: survey-repos

Produce a survey of how every working language repo on COR24, plus the
canonical failing one, currently arranges its memory layout, build
artifacts, and run scripts. The output of this step *grounds the
schema* used by every later step. No Rust is written here.

## Repos in scope

Working (each ships a working layout; learn from each):

- `~/github/sw-embed/sw-cor24-apl`
- `~/github/sw-embed/sw-cor24-basic`
- `~/github/sw-embed/sw-cor24-forth`
- `~/github/sw-embed/sw-cor24-macrolisp`
- `~/github/sw-embed/sw-cor24-ocaml`
- `~/github/sw-embed/sw-cor24-pascal`
- `~/github/sw-embed/sw-cor24-plsw`
- `~/github/sw-embed/sw-cor24-smalltalk`
- `~/github/sw-embed/sw-cor24-snobol4`

Failing canonical case:

- `~/github/sw-vibe-coding/tuplet`

## What to capture per repo

Write `docs/survey/<repo>.md` for each repo with these sections:

1. **Run script(s)**: list every `scripts/run-*.sh` (or equivalent
   justfile/Makefile target). Quote the literal `cor24-run` command
   line each one emits (resolve env vars where you can).
2. **Memory loads**: every `--load-binary <path>@<addr>`, with what
   the path is (locally built? vendored?), the address, and the
   rationale. Tabulate.
3. **Patches**: every `--patch <addr>=<value>`, the symbol it points
   to (look in build outputs like `code_ptr_addr.txt` or in `.lst`
   files), and why.
4. **UART payload structure**: what's fed via `--uart-input` (or
   `-u`)? Source code? EOT terminator? Trailing data? In what
   order?
5. **Heaps and stacks**: enumerate every region used by the program.
   For each, record:
   - embedded in an artifact (which one? at what symbol?) or
     reserved by the loader (at what address, what size)?
   - grows up or down?
   - what symbol(s) does the runtime read to find it?
   - how was the address chosen (gap between layers? above all
     layers? below another layer?)
6. **Build artifacts vs vendored**: what does the repo build
   itself, what does it consume from `vendor/`? Cite version pins
   from `vendor/active.env` or equivalent.
7. **Known limits**: max source size, max image size, max cycles,
   any other guardrails the script enforces.

## Output

- `docs/survey/index.md` -- comparison table across all repos. One
  row per repo, columns for: number of memory layers, number of
  patches, UART carries source? UART carries data? heap is embedded
  or reserved? stack is embedded or reserved? heap grows up or
  down? approximate total SRAM used.
- `docs/survey/<repo>.md` -- one per repo, structure above.
- `docs/survey/schema-gaps.md` -- a final section that lists every
  TOML field the existing `docs/design.md` would need to add or
  change to express each repo without loss. Be specific: name the
  field, name the repo that needs it, name the example value.
- `docs/survey/tuplet-failure-hypothesis.md` -- a short doc (under
  one page) describing what tuplet's `scripts/run-ml-memory.sh`
  does, what is most likely failing, and which `sw-launch`
  validation rule (by E-code) would have caught it.

## Working steps

```bash
agentrail begin
# explore each repo:
ls ~/github/sw-embed/sw-cor24-* | head
for repo in apl basic forth macrolisp ocaml pascal plsw smalltalk snobol4; do
  ls ~/github/sw-embed/sw-cor24-$repo/scripts/ 2>/dev/null
done
# read the run scripts, the build scripts, vendor/active.env, any
# *_addr.txt files, any architecture.md / design.md, then write
# the per-repo doc.
```

## Pre-commit gate

This step touches only docs. Required gates:

```bash
markdown-checker -f "docs/**/*.md"
sw-checklist
```

ASCII-only markdown. Fix any sw-checklist failure immediately.

## Done when

- All ten per-repo docs exist with all seven sections filled in.
- `docs/survey/index.md` table is complete with no `?` cells.
- `docs/survey/schema-gaps.md` lists at least the gaps observed
  (heap-grows-down, multiple memory images at non-trivial offsets,
  build-time-resolved patch addresses via `*_addr.txt`, any
  per-repo idiosyncrasy you find).
- `docs/survey/tuplet-failure-hypothesis.md` cites the specific
  line(s) in `tuplet/scripts/run-ml-memory.sh` and the E-code from
  `docs/design.md` that would catch the failure.
- Commit message describes the step. Do not include code.
- `agentrail complete --summary "..." --reward 1`.

## Stop after complete

Do not start step 2 in the same session. Step 2 (revise-schema)
begins with a fresh `agentrail next`.
