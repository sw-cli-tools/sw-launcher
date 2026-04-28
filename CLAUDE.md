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
git add <files>
git commit -m "<clear message>"
```

**This must happen before `agentrail complete`.** `agentrail complete`
captures the current `HEAD` commit hash into the step's `commits` field,
which is how future `agentrail audit` runs link the step back to its
commit. If you complete before committing, the linkage is wrong.

Include `.agentrail/` files you touched in the commit -- they are part of
the record.

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

### 6. STOP -- do not continue

**Do not make any further changes after `agentrail complete`.** Any
changes after complete are untracked by the saga and invisible to the
next session. If you see more work to do, it belongs in the next step,
not this one.

---

## Rules for `.agentrail/` (CRITICAL -- do not violate)

The `.agentrail/` directory is the durable record of saga/step history.
Treat it like source code.

### Always track it in git

- `.agentrail/` **must** be tracked in git. Never add it to `.gitignore`.
  If you inherit a repo that has `.agentrail/` ignored, that is a bug --
  unignore it and commit the existing contents first.
- Commit step artifacts as each step completes, in the same commit as
  your code changes.

### Never edit or delete files under `.agentrail/` by hand

- **Do not** `rm`, `rm -rf`, `mv`, or use `Write`/`Edit` on any file
  under `.agentrail/` or `.agentrail-archive/`.
- Always go through agentrail subcommands: `init`, `add`, `begin`,
  `complete`, `abort`, `archive`, `plan`, `audit`.
- Direct deletion of untracked step files is **unrecoverable** -- git
  reflog cannot restore blobs that were never staged. This has happened
  before and lost saga history.

### Commit order matters

Work -> `git add` -> `git commit` -> `agentrail complete`. In that order.
Completing before committing means `commits` is empty and the audit
command can't link step to commit.

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
