#!/usr/bin/env python3

import argparse
import json
import os
import subprocess
import sys


BEHAVIOR_PREFIX = "src-tauri/src/"
TESTS_PREFIX = "src-tauri/tests/"
COMPLETE_STATUSES = {"complete", "completed"}


def trimmed(value):
    return value.strip() if isinstance(value, str) else ""


def parse_json_list(raw, field_name):
    if not trimmed(raw):
        return []
    try:
        decoded = json.loads(raw)
    except json.JSONDecodeError as exc:
        raise ValueError(f"{field_name} must be valid JSON: {exc}") from exc
    if not isinstance(decoded, list):
        raise ValueError(f"{field_name} must decode to a JSON array")

    values = []
    seen = set()
    for item in decoded:
        value = trimmed(item)
        if not value:
            continue
        normalized = value.replace("\\", "/")
        if normalized in seen:
            continue
        seen.add(normalized)
        values.append(normalized)
    return values


def is_behavior_rust_path(path):
    return path.startswith(BEHAVIOR_PREFIX) and path.endswith(".rs")


def file_contains_rust_tests(path):
    if not os.path.isfile(path):
        return False
    try:
        with open(path, "r", encoding="utf-8") as handle:
            content = handle.read()
    except OSError:
        return False
    return "#[cfg(test)]" in content or "#[test]" in content


def is_rust_test_delta_path(path):
    if path.startswith(TESTS_PREFIX) and path.endswith(".rs"):
        return True
    if is_behavior_rust_path(path):
        return file_contains_rust_tests(path)
    return False


def task_matches_prefix(task_key, prefix):
    return task_key == prefix or task_key.startswith(prefix + ":")


def task_is_complete(task):
    return trimmed(task.get("status")) in COMPLETE_STATUSES


def first_completed_task(tasks, prefix):
    for task in tasks:
        task_key = trimmed(task.get("key"))
        if task_matches_prefix(task_key, prefix) and task_is_complete(task):
            return task
    return None


def derive_plan_key(plan_key, receipt_id):
    if trimmed(plan_key):
        return trimmed(plan_key)
    if trimmed(receipt_id):
        return f"plan:{trimmed(receipt_id)}"
    return ""


def fetch_plan(project, plan_key):
    env = os.environ.copy()
    env["ACM_LOG_SINK"] = "discard"
    result = subprocess.run(
        ["acm", "fetch", "--project", project, "--key", plan_key],
        capture_output=True,
        text=True,
        env=env,
    )
    if result.returncode != 0:
        raise RuntimeError(
            f"acm fetch failed for {plan_key}: {trimmed(result.stderr) or trimmed(result.stdout) or 'unknown error'}"
        )

    try:
        envelope = json.loads(result.stdout)
    except json.JSONDecodeError as exc:
        raise RuntimeError(f"acm fetch returned invalid JSON for {plan_key}: {exc}") from exc

    items = envelope.get("result", {}).get("items", [])
    if not items:
        return None
    if len(items) != 1:
        raise RuntimeError(f"expected one fetched item for {plan_key}, got {len(items)}")

    content = trimmed(items[0].get("content"))
    if not content:
        return None

    try:
        decoded = json.loads(content)
    except json.JSONDecodeError as exc:
        raise RuntimeError(f"plan content for {plan_key} was not valid JSON: {exc}") from exc
    if not isinstance(decoded, dict):
        raise RuntimeError(f"plan content for {plan_key} was not an object")
    return decoded


def parse_args():
    parser = argparse.ArgumentParser(
        description="Enforce soundspan-app's TDD workflow for native Rust behavior changes."
    )
    parser.add_argument(
        "--project",
        default=os.environ.get("ACM_PROJECT_ID", "soundspan-app"),
        help="ACM project id (defaults to ACM_PROJECT_ID or soundspan-app).",
    )
    parser.add_argument(
        "--plan-key",
        default=os.environ.get("ACM_PLAN_KEY", ""),
        help="Plan key to inspect for repo-local TDD tasks (defaults to ACM_PLAN_KEY).",
    )
    parser.add_argument(
        "--receipt-id",
        default=os.environ.get("ACM_RECEIPT_ID", ""),
        help="Receipt id used to derive plan:<receipt_id> when --plan-key is omitted.",
    )
    parser.add_argument(
        "--files-changed-json",
        default=os.environ.get("ACM_VERIFY_FILES_CHANGED_JSON", "[]"),
        help="JSON array of repo-relative changed paths (defaults to ACM_VERIFY_FILES_CHANGED_JSON).",
    )
    return parser.parse_args()


def fail(message):
    print(f"acm-tdd-guard: {message}", file=sys.stderr)
    return 1


def main():
    args = parse_args()
    try:
        changed_paths = parse_json_list(args.files_changed_json, "files_changed")
    except ValueError as exc:
        return fail(str(exc))

    behavior_paths = [path for path in changed_paths if is_behavior_rust_path(path)]
    if not behavior_paths:
        print("acm-tdd-guard: skip - no native Rust behavior files matched")
        return 0

    plan = None
    plan_key = derive_plan_key(args.plan_key, args.receipt_id)
    if plan_key:
        try:
            plan = fetch_plan(args.project, plan_key)
        except RuntimeError as exc:
            return fail(str(exc))

    tasks = plan.get("tasks", []) if isinstance(plan, dict) else []
    if not isinstance(tasks, list):
        tasks = []

    if first_completed_task(tasks, "tdd:exemption") is not None:
        print("acm-tdd-guard: pass - completed tdd:exemption task found")
        return 0

    if first_completed_task(tasks, "tdd:red") is None:
        return fail(
            "native Rust behavior changes under src-tauri/src/** require a completed tdd:red task before implementation or a completed tdd:exemption task with justification"
        )

    if any(is_rust_test_delta_path(path) for path in changed_paths):
        print("acm-tdd-guard: pass - completed tdd:red task found and Rust test-bearing file delta detected")
        return 0

    return fail(
        "native Rust behavior changes under src-tauri/src/** require a completed tdd:red task before implementation plus a Rust test-bearing file change in the same task, or a completed tdd:exemption task with justification"
    )



if __name__ == "__main__":
    sys.exit(main())
