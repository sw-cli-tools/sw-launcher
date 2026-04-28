# AGENTS.md

Instructions for AI coding agents (Claude Code, opencode, Cursor, etc.)
working in this repository. This project uses **agentrail** to keep
multi-session agent work on track. Follow these rules exactly.

Rename or copy this file to whatever your agent reads (`AGENTS.md`,
`CLAUDE.md`, `.cursorrules`, etc.) -- the content is the same.

---

## The one-paragraph summary

This project uses agentrail to record work as a sequence of **steps** in a
**saga**. Each session you run does exactly one step: read the step prompt
with `agentrail next`, start it with `agentrail begin`, do the work, commit
your changes with git, and close the step with `agentrail complete`. Then
stop -- the next step is for the next session. The `.agentrail/` directory
is the durable record and must be tracked in git like source code.

## The session protocol (follow exactly)

### 1. START -- read your instructions

```bash
agentrail next
```

This prints the current step's prompt, relevant context files, skill
documents, and past successful trajectories for this task type. **Read
the entire output carefully.** It is the instruction for this session.

If `agentrail next` exits with no current step, the saga is paused or
complete -- stop and ask the user what to do. Do not invent work.

### 2. BEGIN -- transition the step

```bash
agentrail begin
```

This marks the step as `in-progress`. Required before doing work.

### 3. WORK -- do exactly what the step prompt says

- Do not ask the user "want me to proceed?" or "shall I start?". The
  step prompt **is** your instruction. Execute it.
- Do not expand scope. If you notice other problems, note them for a
  future step -- do not silently fix them in this one.
- Stay within the files the step prompt references. If you need to touch
  something outside that scope, pause and ask.

### 4. COMMIT -- commit your work with git

```bash
git add <code files> .agentrail/
git commit -m "<clear message>"
```

**This must happen before `agentrail complete`.** `agentrail complete`
captures the current `HEAD` commit hash into the step's `commits` field,
which is how future `agentrail audit` runs link the step back to its
commit. If you complete before committing, the linkage is wrong.

**Always stage `.agentrail/` alongside your code changes.** Every step
you touch creates or updates files under `.agentrail/steps/<NNN>-<slug>/`
(at minimum `step.toml` flips state). Those files are part of the
durable record and must land in the same commit as the step's code.
A commit that updates code without staging the matching `.agentrail/`
diff is a bug -- amend or follow up before completing.

### 5. COMPLETE -- close the step

```bash
agentrail complete \
  --summary "what you accomplished in one or two sentences" \
  --reward 1 \
  --actions "tools and approach used"
```

Flags:
- `--reward 1` on success, `--reward -1 --failure-mode "<what-went-wrong>"`
  on failure. Reward is used for trajectory recording so future sessions
  can learn from what worked.
- Add `--done` if this was the last step of the saga.
- Use `--next-slug` and `--next-prompt` to define the next step if you
  know what it should be; otherwise the human will plan it.

### 6. PUSH -- publish the step

```bash
git status                  # confirm clean tree, including .agentrail/
git push
```

**Every completed step must be pushed before the session ends.** This
is non-negotiable:

- The saga record (`.agentrail/`) only protects future sessions if it
  reaches the remote. Local-only commits are invisible to other
  machines, to CI, and to anyone reviewing the work.
- `agentrail complete` writes a final transition into
  `.agentrail/steps/<NNN>-<slug>/step.toml`. Push *after* `complete`,
  not before, so that final write is included in the pushed commit.
  (If you pushed before `complete`, do a follow-up `git commit -am
  "agentrail: finalize step <slug>"` and push again.)
- If `git push` fails (no upstream, network error, conflict), resolve
  it now -- do not defer. A step that is committed but never pushed
  looks "done" locally and "not started" everywhere else.

After push: confirm `git status` is clean and `git log @{u}..` is
empty.

### 7. STOP -- do not continue

**Do not make any further changes after `git push`.** Any changes
after the step is pushed are untracked by the saga and invisible to
the next session. If you see more work to do, it belongs in the next
step, not this one.

---

## Rules for `.agentrail/` (CRITICAL -- do not violate)

The `.agentrail/` directory is the durable record of saga/step history.
Treat it like source code.

### Always track, commit, and push it

- `.agentrail/` **must** be tracked in git. Never add it to `.gitignore`.
  If you inherit a repo that has `.agentrail/` ignored, that is a bug --
  unignore it and commit the existing contents first.
- Commit step artifacts as each step completes, in the same commit as
  your code changes.
- Push after every step's final commit (the one written by `agentrail
  complete`). A committed-but-unpushed `.agentrail/` is invisible to
  the next session on another machine, to CI, and to `agentrail
  audit` runs that look at remote history.
- Before starting a step (after `agentrail next`, before `agentrail
  begin`), verify `git status` is clean and `git log @{u}..` is
  empty. If they aren't, the previous session forgot to push --
  push now before starting new work.

### Never edit or delete files under `.agentrail/` by hand

- **Do not** `rm`, `rm -rf`, `mv`, or use `Write`/`Edit` on any file
  under `.agentrail/` or `.agentrail-archive/`.
- Always go through agentrail subcommands: `init`, `add`, `begin`,
  `complete`, `abort`, `archive`, `plan`, `audit`.
- Direct deletion of untracked step files is **unrecoverable** -- git
  reflog cannot restore blobs that were never staged. This has happened
  before and lost saga history.

### Commit order matters

Work -> `git add` -> `git commit` -> `agentrail complete` -> `git
commit -am "agentrail: finalize step <slug>"` (if `complete` mutated
files) -> `git push`. In that order. Completing before committing
means `commits` is empty and the audit command can't link step to
commit. Pushing before completing means the final state of
`.agentrail/steps/<NNN>/step.toml` lives only on your machine.

### Archiving a saga also commits and pushes

`agentrail archive --reason "..."` moves the current saga from
`.agentrail/` into `.agentrail-archive/<saga>/`. That is a content
move on disk and shows up as a large rename diff in git. The same
commit-and-push discipline applies:

```bash
agentrail archive --reason "Phase 1 complete; opening Phase 2 saga"
git add .agentrail/ .agentrail-archive/
git commit -m "agentrail: archive saga <name>"
git push
```

Never archive without immediately committing and pushing the result.
A locally archived saga that hasn't been pushed is the worst of both
worlds: the active `.agentrail/` no longer remembers it, and the
remote never saw the move.

---

## Recovering from gaps

If history gets out of sync -- for example, an agent made commits without
running `agentrail complete`, or steps were added without matching
commits -- use the audit command.

```bash
agentrail audit                    # human-readable markdown report
agentrail audit --emit-commands    # shell script of suggested add lines
agentrail audit --since v1.0       # only look at commits after v1.0
```

The report has four sections:

1. **Matched** -- commits that line up with a saga step (by recorded hash
   or by timestamp window for legacy steps).
2. **Orphan commits** -- commits with no matching step. These are the
   gaps.
3. **Orphan steps** -- steps whose recorded commit isn't in the current
   history (rebased away, squashed, never made).
4. **Working tree** -- uncommitted changes. Reported for awareness, not
   turned into commands.

With `--emit-commands`, the tool prints a shell script with one
`agentrail add --commit <hash> --slug ... --prompt ...` line per orphan
commit. **Review and edit the slugs and prompts before running** -- the
defaults are seeded from commit subjects and need human judgment.

## Retroactive history for old projects

If the project predates agentrail and you want to add a saga on top of
existing history:

```bash
agentrail audit --emit-commands > rebuild.sh
# Edit rebuild.sh: reword slugs and prompts as coherent step descriptions
sh rebuild.sh
```

The script begins with `agentrail init --retroactive --name development`
(when no saga exists) and then adds one step per historical commit.
Retroactive sagas are marked in `saga.toml` so future audits know those
commits are claimed.

Going forward from there, run new sagas normally.

## Safety net: `agentrail snapshot`

If you have files under `.agentrail/` that are not yet committed and
you're about to do something risky (a big agent run, a rebase, cleaning
up untracked files), run:

```bash
agentrail snapshot
```

This creates a git commit under `refs/agentrail/snapshots/<timestamp>`
containing a copy of `.agentrail/` and `.agentrail-archive/`. The user's
real git index is not touched -- it uses a throwaway temp index under the
hood. The snapshot survives `git gc` because a named ref holds it.

Restore from a snapshot with a normal git command:

```bash
git restore --source=refs/agentrail/snapshots/<timestamp> \
    -- .agentrail .agentrail-archive
```

List existing snapshots with `agentrail snapshot --list`.

This is a safety net, not a replacement for committing. Commit your
work normally -- use snapshot only as belt-and-suspenders insurance.

---

## Quick reference

| Command | When to use |
|---|---|
| `agentrail next` | Start of every session |
| `agentrail begin` | After reading `next`, before working |
| `agentrail complete --summary "..." --reward 1` | After committing |
| `agentrail status` | Inspect current saga state (read-only) |
| `agentrail history` | Show all step summaries (read-only) |
| `agentrail plan --update ...` | Revise the saga plan |
| `agentrail add --slug ... --prompt ...` | Add a step without completing current one (maintenance mode) |
| `agentrail abort --reason "..."` | Mark current step as blocked |
| `agentrail archive --reason "..."` | Close out a saga and start fresh |
| `agentrail audit` | Diagnose saga-vs-git gaps |
| `agentrail snapshot` | Save a safety-net copy of `.agentrail/` into the git object store (opt-in) |
| `agentrail snapshot --list` | List existing snapshot refs |

## What not to do

- Do not run `agentrail complete` before committing.
- Do not touch files under `.agentrail/` with anything other than
  `agentrail` subcommands.
- Do not keep working after `agentrail complete`. Stop and let the next
  session pick up.
- Do not add `.agentrail/` to `.gitignore`.
- Do not skip `agentrail next` "because you remember what the step was"
  -- the next output includes trajectories and skill docs that change as
  the system learns.
