# AGENTS.md

Operating contract for a repo that uses `acm` and wants enforced detailed feature planning.
Keep this file as the fast path, then move heavier architecture, checklist, or troubleshooting material into linked repo-local maintainer docs as the project grows.

## Purpose

- `soundspan-app` is a thin Tauri shell for a remote `soundspan` instance with a Rust native-audio backend.
- Use this file for the task loop and repo-wide rules.
- Use `docs/maintainer-map.md` when you need repo-specific routing for setup shell, native audio, config/security, packaging, or release files.

## Source Of Truth

- Follow this file first.
- Keep canonical rules in `.acm/acm-rules.yaml` (preferred) or `acm-rules.yaml` at the repo root.
- Keep canonical tags in `.acm/acm-tags.yaml` and executable checks in `.acm/acm-tests.yaml`.
- Keep canonical completion workflow gates in `.acm/acm-workflows.yaml` (preferred) or `acm-workflows.yaml`.
- Keep repo-specific maintainer context in `docs/maintainer-map.md` and design-stage context in `docs/designs/**`.
- If tool-specific instructions conflict with this file, this file wins unless a human explicitly says otherwise.

## Required Task Loop

1. Read this file and the human task.
2. Run `acm context` before opening or editing project files.
3. Follow all hard rules returned in the receipt.
4. Use `fetch` only for the pointers, plans, and task keys needed for the current step.
5. When a task spans multiple steps, multiple files, or a likely handoff, create or update `work`.
6. For net-new feature work or large capability expansions, create the repo's detailed feature plan before implementation.
7. If code, config, schema, or other executable behavior changes, run `verify` before `done`.
8. If `.acm/acm-workflows.yaml` requires review task keys such as `review:cross-llm`, prefer `review --run` when the task defines a `run` block; otherwise use manual `review` fields or `work` before `done`.
9. End every task with `done`, including every changed file for file-backed work when you know them, or letting ACM derive the task delta from the receipt baseline. When that detected delta is empty, the closeout is effectively no-file.
10. If you learn a reusable decision, gotcha, or preference, record it with `memory`.

When the task changes rules, tags, tests, workflows, onboarding, or tool-surface behavior, refresh broker state with `acm sync --mode working_tree --insert-new-candidates` and then run `acm health --include-details` before `done`.

If you need to resume after compaction or inspect archived work, use direct CLI `acm history` with `--entity work` for plan/task discovery or another entity for memories, receipts, and runs, then `acm fetch` the returned `fetch_keys`.
If you need to debug project setup, loaded ACM files, integrations, or what `context` would load for a task, use `acm status`.

## Working Rules

- Do not silently expand governed file scope. Refresh context first if the task spills into adjacent systems, and use `work.plan.discovered_paths` when later-discovered files must be declared for review/done.
- Read the full relevant source before editing it. Do not guess at Tauri wiring, IPC boundaries, or packaging metadata.
- Prefer small, reviewable changes over broad cleanup.
- For behavior-changing Rust work under `src-tauri/src/**`, write or update a failing Rust test first, record a completed `tdd:red` task before implementation, and keep a Rust test-bearing file change in the same task. If practical Rust coverage is not appropriate, record a completed `tdd:exemption` task with a concrete justification. Docs, packaging, onboarding, and workflow-governance-only work are exempt.
- Do not invent product requirements, compatibility guarantees, or migration behavior when the repo does not define them.
- If verification fails, either fix the issue or report the failure clearly. Do not claim the task is complete as if checks passed.
- Verification claims must come from actual command output. Re-run verification after any subsequent code change, and avoid speculative pass language such as "should work" or "probably fine."
- Keep work state current when you pause, hand off, or hit a blocker.
- After three consecutive failed attempts to fix the same issue, stop, document what was tried, reassess the root-cause hypothesis, and ask for direction if the root cause is still unclear.
- Prefer targeted verification. For native code or Tauri config changes, default to `cargo test --manifest-path src-tauri/Cargo.toml` and `cargo check --manifest-path src-tauri/Cargo.toml`; reserve full `cargo tauri build` for packaging/release work or when the user explicitly asks for it.
- When `.acm/acm-workflows.yaml` selects `review:cross-llm`, satisfy it with `acm review --run --receipt-id <receipt-id>` instead of treating review as optional prose.
- Reviewer provider, model, reasoning, and shared `--yolo` settings live in `.acm/acm-workflows.yaml`; `--yolo` maps to native Codex yolo mode or Claude dangerous-permissions mode.

## Repository-Specific Rules

- `app-shell/index.html` is the local setup shell. It should stay focused on bootstrap and instance selection; the actual product UI continues to live in the configured remote `soundspan` instance unless a task explicitly changes that model.
- The Tauri bootstrap and command registration live in `src-tauri/src/lib.rs` and `src-tauri/src/main.rs`. Keep invoke registration, startup navigation, and plugin wiring aligned with the underlying Rust modules.
- Native playback behavior lives in `src-tauri/src/audio/**`. Keep command handlers, player/output/decoder logic, and any serialized playback state aligned when changing the native-audio path.
- Origin checks in `src-tauri/src/config/security.rs` are a security boundary. Local-only commands should stay gated to local app content, and remote playback IPC should stay limited to the configured `soundspan` instance origin.
- Persisted settings live in `config.json` through `tauri-plugin-store`. Keep `src-tauri/src/config/store.rs` updated when store keys change.
- Packaging, capabilities, and release metadata live in `src-tauri/tauri.conf.json`, `src-tauri/capabilities/**`, and `aur/**`. Keep identifiers, bundle targets, permissions, and package metadata aligned when those surfaces change.
- User-visible onboarding, audio-path, packaging, or permission changes should update `README.md` and any impacted maintainer or design docs in `docs/`.
- Changes to routing surfaces documented in `docs/maintainer-map.md` should keep that file aligned in the same task.

## Verification Evidence

- Run the verification command and read the complete output. Do not infer success from partial logs.
- Prefix verification evidence claims with `verify:` when you report them in task notes or completion summaries.
- Evidence is stale after any subsequent code change. Re-run verification after editing files again.
- Never substitute speculative language such as "should work", "looks fine", or "probably passes" for actual verification results.

## Debugging Protocol

1. Investigate: read the full error output, reproduce the issue, and trace the relevant data flow.
2. Analyze: compare against the known-good path and identify what changed.
3. Hypothesize: form one specific root-cause hypothesis.
4. Implement: apply a targeted fix and verify the root cause is resolved rather than masked.
5. Escalate: if three consecutive fix attempts fail, stop and surface what was tried and why it failed before continuing.

## When To Use work

Use `work` when any of the following are true:

- the task will take more than one material step
- more than one file or subsystem is involved
- the task includes explicit planning, verification, or handoff
- you need durable task state that should survive compaction or session reset

For code changes, include a `verify:tests` task. Add other task keys when they help resumption, coordination, or are required by `.acm/acm-workflows.yaml`. For single review-gate updates, `review` is the thinner convenience wrapper around `work`; use `review --run` for runnable workflow gates, and keep manual `status` / `outcome` / `blocked_reason` / `evidence` fields for non-run mode. If a planned task or review gate becomes obsolete, mark it `superseded` instead of leaving it open or `blocked`.
For behavior-changing Rust work under `src-tauri/src/**`, include either `tdd:red` or `tdd:exemption` alongside `verify:tests`.

## Historical Lookup

- Use `acm history --entity work --scope all --query "<topic>"` to find archived, current, deferred, or completed work by topic.
- Use `acm history --entity memory --query "<topic>"` for durable decisions and recurring pitfalls, or `--entity all` when you are not sure which surface holds the answer.
- Fetch the returned `fetch_keys` before acting on historical results.
- Use `acm status --task-text "<task>" --phase <plan|execute|review>` when you need runtime diagnostics or want to preview what ACM will load before editing code.

## Feature Plans

Use the richer ACM feature plan contract in this repo for net-new feature work and large capability expansions.

- Create a root ACM plan with `kind=feature` before implementation.
- Root feature plans must include `objective`, `in_scope`, `out_of_scope`, `constraints`, `references`, and stage statuses for `spec_outline`, `refined_spec`, and `implementation_plan`.
- Root feature plans must include top-level `stage:spec-outline`, `stage:refined-spec`, `stage:implementation-plan`, and `verify:tests` tasks.
- Put concrete child tasks beneath the `stage:*` grouping tasks with `parent_task_key`.
- Atomic tasks are leaf tasks. Leaf tasks must carry explicit `acceptance_criteria`.
- When the work splits into parallel execution streams, use `kind=feature_stream` plus `parent_plan_key`.
- Keep the detailed contract in `docs/feature-plans.md`.
- Let `scripts/acm-feature-plan-validate.py` enforce the schema through `verify`.

Use thinner plans for bugfixes, narrow maintenance, review-only work, or workflow-governance changes.

## Ruleset Maintenance

1. Edit the canonical rules, tags, tests, or workflow files.
2. Run `acm sync --mode working_tree --insert-new-candidates` or `acm health --apply`.
3. Run `acm health --include-details` and resolve blocking findings.

## ACM Maintenance

- When `.acm/**`, `AGENTS.md`, `CLAUDE.md`, or repo-local ACM helper scripts change, run `acm sync --mode working_tree --insert-new-candidates` and `acm health --include-details` before `done`.
- Use `acm health --apply` when ACM-managed state needs repair without a broader manual sync flow.
- Treat a fresh worktree like a fresh ACM runtime surface: run `acm sync --mode working_tree --insert-new-candidates` before relying on retrieval there, then `acm health --include-details` if the worktree may be stale.
- If you run ACM from outside the repo root, set `ACM_PROJECT_ROOT` to the active worktree path.

## Tool-Specific Companions

`CLAUDE.md`, `.claude/acm-broker/**`, `.codex/acm-broker/**`, `.opencode/acm-broker/**`, slash commands, and repo-local agent companions should stay thin and map their workflow back to this file.
If they disagree with this file, this file is authoritative.
