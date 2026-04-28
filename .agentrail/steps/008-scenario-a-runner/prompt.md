# Step 8: scenario-a-runner

Wire `sw-launch run <scenario>` end-to-end for Scenario A:
build (with in-process memoization from step 6) -> emit argv (step
7) -> spawn `cor24-run` -> capture stdout/stderr -> check
expectations. TDD: write the integration test against a fixture
echo.s first.

## What lands

- `src/run.rs`:
  - `fn execute(plan: &LoadPlan, target: &Target, profile: &Profile)
    -> Result<RunOutcome>`
  - `RunOutcome { exit_status: ExitStatus, stdout: String,
    stderr: String, uart_output: String, duration: Duration }`
  - extracts the UART payload from `cor24-run`'s output (look at
    real cor24-run output -- usually a `UART output: ...` block;
    confirm by running it once during step 8 development)
  - enforces `RunCfg.timeout_ms` via `wait_timeout` (add the
    `wait-timeout` crate, ~30 LOC dep) or roll a small thread-based
    timeout if the survey shows that crate is already in the
    ecosystem
- `src/expect.rs`:
  - `fn check(expect: &Expect, outcome: &RunOutcome) -> Result<(),
    Vec<Mismatch>>`
  - matchers: `uart_contains`, `uart_regex`, `uart_not_contains`,
    `exit_code`, `stdout_lines_eq`
  - `Mismatch { kind: MismatchKind, expected: String, got: String }`
  - readable Display impl that shows expected vs got side by side
- `cli::run` wires Config -> validate -> build (memoize) -> LoadPlan
  -> Target::build_argv -> run::execute -> expect::check;
  exits 0 only if everything passes, non-zero with a structured
  report otherwise

## Tests (write first)

Unit tests for `expect.rs`:

- `uart_contains` passes/fails correctly
- `uart_regex` compiles patterns once
- `exit_code = 0` and a non-zero outcome reports the mismatch
- `uart_not_contains` matches forbidden strings

Integration tests (`tests/scenario_a.rs`):

- `tests/fixtures/scenario_a/sw-launch.toml` -- a complete Scenario
  A: `echo.s` (already added in step 6), no embedded segments
  declared, UART input "hello\n", expects `uart_contains = ["hello"]`
- run via `assert_cmd` against `cargo run -- run echo`
- gate behind `cor24-run` on PATH; if missing, mark `#[ignore]` and
  print a one-line note
- second invocation in the same `cargo test` run hits the in-process
  memoization (assert the assembler is invoked at most once across
  the suite if feasible; otherwise document why not)

## Pre-commit gate

```bash
cargo test
cargo clippy --all-targets --all-features -- -D warnings
cargo fmt --all
markdown-checker -f "**/*.md"
sw-checklist
```

## Done when

- The Scenario A integration test passes against the real
  `cor24-run` on the dev machine.
- `sw-launch run echo` end to end: builds, runs, asserts, exits 0.
- Mismatch output for a deliberately-broken expectation is
  human-readable.
- Zero clippy warnings; sw-checklist green.
- Commit summarizes Phase 1 closure.
- `agentrail complete --summary "..." --reward 1`.

## Stop after complete

Step 9 (phase1-status) is a fresh session.
