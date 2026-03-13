#!/usr/bin/env python3

import argparse
import json
import os
import sys


MODE_CONFIG = {
    "maintainer-map": {
        "required_doc": "docs/maintainer-map.md",
        "trigger_exact": {
            "app-shell/index.html",
            "src-tauri/src/lib.rs",
            "src-tauri/src/main.rs",
            "src-tauri/src/config/security.rs",
            "src-tauri/src/config/store.rs",
            "src-tauri/tauri.conf.json",
        },
        "trigger_prefixes": (
            "src-tauri/src/audio/",
            "src-tauri/capabilities/",
            "aur/",
        ),
        "label": "maintainer routing surfaces",
    },
    "readme": {
        "required_doc": "README.md",
        "trigger_exact": {
            "app-shell/index.html",
            "src-tauri/src/config/security.rs",
            "src-tauri/tauri.conf.json",
        },
        "trigger_prefixes": (
            "src-tauri/src/audio/",
            "src-tauri/capabilities/",
            "aur/",
        ),
        "label": "user-facing onboarding, packaging, or permission surfaces",
    },
}


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


def matches_surface(path, config):
    if path in config["trigger_exact"]:
        return True
    return any(path.startswith(prefix) for prefix in config["trigger_prefixes"])


def parse_args():
    parser = argparse.ArgumentParser(
        description="Enforce repo-local documentation drift checks for soundspan-app."
    )
    parser.add_argument(
        "--mode",
        choices=sorted(MODE_CONFIG.keys()),
        required=True,
        help="Which documentation surface to enforce.",
    )
    parser.add_argument(
        "--files-changed-json",
        default=os.environ.get("ACM_VERIFY_FILES_CHANGED_JSON", "[]"),
        help="JSON array of repo-relative changed paths (defaults to ACM_VERIFY_FILES_CHANGED_JSON).",
    )
    return parser.parse_args()


def fail(message):
    print(f"acm-doc-drift-guard: {message}", file=sys.stderr)
    return 1


def main():
    args = parse_args()
    config = MODE_CONFIG[args.mode]

    try:
        changed_paths = parse_json_list(args.files_changed_json, "files_changed")
    except ValueError as exc:
        return fail(str(exc))

    triggered = [path for path in changed_paths if matches_surface(path, config)]
    if not triggered:
        print(f"acm-doc-drift-guard: skip - no {config['label']} matched")
        return 0

    if config["required_doc"] in changed_paths:
        print(
            f"acm-doc-drift-guard: pass - {config['required_doc']} changed alongside {len(triggered)} relevant path(s)"
        )
        return 0

    relevant = ", ".join(triggered[:5])
    if len(triggered) > 5:
        relevant += ", ..."
    return fail(
        f"{config['label']} changed without updating {config['required_doc']}; relevant changed paths: {relevant}"
    )


if __name__ == "__main__":
    sys.exit(main())
