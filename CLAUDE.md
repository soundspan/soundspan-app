# CLAUDE.md

Claude companion for a repo whose primary contract is `AGENTS.md`.

## Source Of Truth

- Follow `AGENTS.md` first.
- Use this file only to map Claude's workflow to the repo contract.
- Use `docs/maintainer-map.md` when you need repo-specific routing or invariants.
- If this file conflicts with `AGENTS.md`, `AGENTS.md` wins.

## Claude Workflow

1. Start with `/acm-context ...`.
2. Read the returned hard rules before touching files.
3. Use `/acm-work ...` when the task is multi-step, spans multiple files, or needs durable state.
4. For net-new feature work or large capability expansions, create the repo's detailed feature plan before implementation and keep its `stage:*` tasks current.
5. Use `/acm-verify ...` before `/acm-done ...` for any code, config, schema, or executable behavior change.
6. Use `/acm-review <receipt_id-or-plan_key> {"run":true}` when `.acm/acm-workflows.yaml` requires a review task such as `review:cross-llm` and the task defines a `run` block; otherwise use manual review JSON or `/acm-work ...`.
7. Use `/acm-done ...` to close the task; include changed files for file-backed work when you have them, or let ACM compute the task delta from the receipt baseline. When that detected delta is empty, the closeout is effectively no-file. ACM may enforce additional task keys from `.acm/acm-workflows.yaml` when file-backed work is detected.
8. Use `/acm-memory ...` for durable decisions and gotchas, including evidence from effective scope and preferring governed `evidence_paths` unless exact fetched keys are already available.

If the task changes rules, tags, tests, workflows, onboarding, or tool-surface behavior, run direct CLI `acm sync --mode working_tree --insert-new-candidates` and `acm health --include-details` before `/acm-done`.

If you need historical discovery after compaction, use direct CLI `acm history` with `--entity work` for plan/task discovery or another entity for memories, receipts, and runs, then `acm fetch` the returned `fetch_keys`; the default slash-command pack does not add a dedicated `/acm-history` command.
If you need runtime or setup diagnostics, use direct CLI `acm status`.

## Claude-Specific Notes

- Keep prompts specific enough that `context` returns the right rules, active work, memories, and any explicitly known scope.
- If the receipt looks stale or too narrow, re-run `/acm-context` with a better task description instead of guessing.
- If governed file work expands beyond the initial receipt scope, record the new files through `/acm-work` before expecting `/acm-review` or `/acm-done` to pass.
- Do not claim success when `/acm-verify` failed or was skipped for code changes. Verification claims should come from actual command output, and they become stale after later edits.
- In this repo, `app-shell/index.html` stays the local setup shell, `src-tauri/src/audio/**` owns native playback, and `src-tauri/src/config/security.rs` is the origin-check boundary for local versus remote IPC.
- For behavior-changing Rust work under `src-tauri/src/**`, write or update a failing Rust test first, use `/acm-work` to record a completed `tdd:red` task before implementation, and keep a Rust test-bearing file change in the same task unless a completed `tdd:exemption` task records why practical Rust coverage is not appropriate.
- For native-code or Tauri config work, prefer `cargo test --manifest-path src-tauri/Cargo.toml` and `cargo check --manifest-path src-tauri/Cargo.toml` before considering a full Tauri build.
- `/acm-review` stays thin. Use `{"run":true}` for runnable workflow gates because manual complete notes do not satisfy runnable gates, and reserve manual `status`, `outcome`, `blocked_reason`, and `evidence` fields for non-run mode.
- This repo now uses a runnable `review:cross-llm` gate for workflow-selected implementation and governance work. Use `/acm-review <receipt_id-or-plan_key> {"run":true}` when the workflow selects it.
- Reviewer provider and high-trust flag settings stay in `.acm/acm-workflows.yaml`; this repo uses the shared `--yolo` shortcut, which maps to native Codex yolo mode or Claude dangerous-permissions mode.
- If `/acm-review {"run":true}` reports repo changes but zero scoped review files, the receipt or declared discovered scope is too narrow. Re-run `/acm-context` or update `/acm-work` before retrying review.
- For feature work under this repo contract, populate the required `kind=feature` or `kind=feature_stream` plan shape, `stages`, `stage:*` tasks, `parent_task_key`, and leaf `acceptance_criteria` before implementation. See `docs/feature-plans.md`.
- Let `verify` enforce the feature-plan schema through `scripts/acm-feature-plan-validate.py`.
- When blocked on a missing product or architectural decision, surface the decision instead of improvising it.
- If three consecutive fix attempts fail, stop and surface the attempted fixes and the remaining root-cause uncertainty before continuing.
- Use direct CLI `acm history --entity work|memory|all ...` for archived plan and memory discovery, and `acm status --task-text "<task>" --phase <plan|execute|review>` for runtime/setup diagnostics.
- Changes to routing surfaces documented in `docs/maintainer-map.md`, or user-facing onboarding/packaging/permission surfaces described in `README.md`, should keep those docs aligned in the same task.

## Ruleset Maintenance

When `.acm/acm-rules.yaml`, `.acm/acm-tags.yaml`, `.acm/acm-tests.yaml`, `.acm/acm-workflows.yaml`, `AGENTS.md`, `CLAUDE.md`, or repo-local ACM companion docs change, refresh broker state with `acm sync` or `acm health --apply`, then run `acm health`. Treat fresh worktrees the same way before relying on retrieval.
