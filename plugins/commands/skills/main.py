#!/usr/bin/env python3
"""PM skills plugin — AI agent skill registry manager."""

from __future__ import annotations

import argparse
import json
import os
import shutil
import subprocess
import sys
from collections import OrderedDict
from datetime import datetime, timezone
from pathlib import Path

import yaml

STORE_DIR = Path(os.environ.get("PM_CONFIG_DIR", str(Path.home() / ".config" / "pm"))).expanduser().resolve()
SKILLS_DIR = STORE_DIR / "skills"
REGISTRY_PATH = SKILLS_DIR / "registry.yaml"
OLD_STORE_DIR = Path.home() / ".skills" / "dotfiles" / "skills"

VALID_SOURCES = {"custom", "downloaded", "builtin"}
VALID_AGENTS = {"claude", "codex", "gemini", "opencode", "universal"}

FIELD_ORDER = [
    "name",
    "source",
    "agent",
    "tags",
    "path",
    "description",
    "install_source",
    "import_origin",
    "created_at",
    "updated_at",
    "disabled",
]

SKILL_MD_TEMPLATE = """---
description: "{description}"
---

# {name}

TODO: Write skill instructions here.
"""

COPY_IGNORE = {".git", "__pycache__", "node_modules", ".DS_Store", ".venv", "venv"}


def default_registry() -> dict:
    return {
        "version": 1,
        "profiles": {
            "dev": {"description": "개발 프로젝트 공통", "tags": ["dev", "openspec", "util", "meta"]},
            "full": {"description": "모든 스킬", "tags": ["*"]},
        },
        "skills": [],
    }


def ensure_store() -> None:
    (STORE_DIR / "plugins" / "commands" / "skills").mkdir(parents=True, exist_ok=True)
    (SKILLS_DIR / "custom").mkdir(parents=True, exist_ok=True)
    (SKILLS_DIR / "downloaded").mkdir(parents=True, exist_ok=True)

    if REGISTRY_PATH.exists():
        return

    if OLD_STORE_DIR.exists():
        shutil.copytree(OLD_STORE_DIR, SKILLS_DIR, dirs_exist_ok=True)
        if REGISTRY_PATH.exists():
            return

    RegistryWriter(REGISTRY_PATH).save(default_registry())


def expand_path(p: str | None) -> Path | None:
    if p is None:
        return None
    expanded = os.path.expanduser(p)
    path = Path(expanded)
    if not path.is_absolute():
        path = STORE_DIR / path
    return path.resolve()


def rel_store_path(path: Path) -> str:
    try:
        return path.resolve().relative_to(STORE_DIR).as_posix()
    except ValueError:
        return str(path.resolve())


def now_iso() -> str:
    return datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")


def format_table(rows: list[list[str]], headers: list[str]) -> str:
    all_rows = [headers] + rows
    widths = [max(len(str(cell)) for cell in col) for col in zip(*all_rows)]
    lines = []
    lines.append("  ".join(h.ljust(w) for h, w in zip(headers, widths)))
    lines.append("  ".join("─" * w for w in widths))
    for row in rows:
        lines.append("  ".join(str(cell).ljust(w) for cell, w in zip(row, widths)))
    return "\n".join(lines)


def ordered_skill(entry: dict) -> OrderedDict:
    return OrderedDict(
        (k, entry[k])
        for k in FIELD_ORDER
        if k in entry and entry[k] is not None and entry[k] != "" and entry[k] != []
    )


class FlowList(list):
    pass


class FlowListDumper(yaml.Dumper):
    pass


def _flow_list_representer(dumper, data):
    return dumper.represent_sequence("tag:yaml.org,2002:seq", data, flow_style=True)


FlowListDumper.add_representer(FlowList, _flow_list_representer)


def levenshtein(a: str, b: str) -> int:
    if len(a) < len(b):
        return levenshtein(b, a)
    if not b:
        return len(a)
    prev = list(range(len(b) + 1))
    for i, ca in enumerate(a):
        curr = [i + 1]
        for j, cb in enumerate(b):
            curr.append(min(prev[j + 1] + 1, curr[j] + 1, prev[j] + (0 if ca == cb else 1)))
        prev = curr
    return prev[-1]


class RegistryWriter:
    def __init__(self, path: Path = REGISTRY_PATH):
        self.path = path
        self._data: dict | None = None

    def load(self) -> dict:
        ensure_store()
        with open(self.path) as f:
            self._data = yaml.safe_load(f) or default_registry()
        return self._data

    def save(self, data: dict | None = None) -> None:
        if data is not None:
            self._data = data
        if self._data is None:
            raise RuntimeError("No data to save")

        skills = sorted(self._data.get("skills", []), key=lambda s: s.get("name", "").lower())
        self._data["skills"] = [dict(ordered_skill(s)) for s in skills]

        for profile in self._data.get("profiles", {}).values():
            if "tags" in profile:
                profile["tags"] = FlowList(profile["tags"])
        for skill in self._data["skills"]:
            if "tags" in skill:
                skill["tags"] = FlowList(skill["tags"])

        self._write_with_comments()

    def _write_with_comments(self) -> None:
        lines = []
        lines.append(f"version: {self._data.get('version', 1)}")
        lines.append("")
        lines.append("# ── Profile Definitions ───────────────────────────────────────────────")
        profiles_yaml = yaml.dump(
            {"profiles": self._data.get("profiles", {})},
            Dumper=FlowListDumper,
            default_flow_style=False,
            allow_unicode=True,
            sort_keys=False,
        )
        lines.append(profiles_yaml.rstrip())
        lines.append("")
        lines.append("# ── Skill Registry ────────────────────────────────────────────────────")
        lines.append("# source:  custom | downloaded | builtin")
        lines.append("# agent:   claude | codex | gemini | opencode | universal")
        lines.append("skills:")

        source_order = ["custom", "downloaded", "builtin"]
        source_labels = {
            "custom": "Custom",
            "downloaded": "Downloaded",
            "builtin": "Builtin",
        }
        skills_by_source: dict[str, list[dict]] = {}
        for skill in self._data.get("skills", []):
            source = skill.get("source", "custom")
            skills_by_source.setdefault(source, []).append(skill)

        for source in source_order:
            group = skills_by_source.get(source, [])
            if not group:
                continue
            label = source_labels[source]
            lines.append("")
            lines.append(f"  # ── {label} ({len(group)}) {'─' * max(1, 52 - len(label) - len(str(len(group))))}")
            for skill in group:
                lines.append("")
                skill_yaml = yaml.dump(
                    [skill],
                    Dumper=FlowListDumper,
                    default_flow_style=False,
                    allow_unicode=True,
                    sort_keys=False,
                )
                for line in skill_yaml.rstrip().split("\n"):
                    lines.append(f"  {line}")

        lines.append("")
        self.path.write_text("\n".join(lines))

    @property
    def data(self) -> dict:
        if self._data is None:
            self.load()
        return self._data

    @property
    def skills(self) -> list[dict]:
        return self.data.get("skills", [])

    def find_skill(self, name: str) -> dict | None:
        target = name.lower()
        for skill in self.skills:
            if skill["name"].lower() == target:
                return skill
        return None

    def find_similar(self, name: str, max_results: int = 5) -> list[str]:
        target = name.lower()
        scored = []
        for skill in self.skills:
            dist = levenshtein(target, skill["name"].lower())
            if dist <= max(3, len(target) // 2):
                scored.append((dist, skill["name"]))
        scored.sort()
        return [name for _, name in scored[:max_results]]

    def add_skill(self, entry: dict) -> None:
        if self.find_skill(entry["name"]):
            raise ValueError(f"Skill '{entry['name']}' already exists")
        entry.setdefault("created_at", now_iso())
        self.skills.append(entry)
        self.save()

    def remove_skill(self, name: str) -> dict:
        target = name.lower()
        for index, skill in enumerate(self.skills):
            if skill["name"].lower() == target:
                removed = self.skills.pop(index)
                self.save()
                return removed
        raise KeyError(f"Skill '{name}' not found")

    def update_skill(self, name: str, updates: dict) -> tuple[dict, dict]:
        skill = self.find_skill(name)
        if not skill:
            raise KeyError(f"Skill '{name}' not found")
        before = dict(skill)
        for key, value in updates.items():
            if value is not None:
                skill[key] = value
        skill["updated_at"] = now_iso()
        self.save()
        return before, dict(skill)


def load_registry() -> dict:
    return RegistryWriter().load()


def resolve_profile_tags(registry: dict, profile_name: str) -> list[str]:
    profiles = registry.get("profiles", {})
    if profile_name not in profiles:
        print(f"Error: unknown profile '{profile_name}'", file=sys.stderr)
        print(f"Available profiles: {', '.join(sorted(profiles.keys()))}", file=sys.stderr)
        sys.exit(1)
    return profiles[profile_name].get("tags", [])


def skill_matches_tags(skill: dict, tags: list[str]) -> bool:
    if "*" in tags:
        return True
    return bool(set(skill.get("tags", [])) & set(tags))


def skill_matches_agent(skill: dict, agent: str | None) -> bool:
    if agent is None:
        return True
    skill_agent = skill.get("agent", "universal")
    return skill_agent == "universal" or skill_agent == agent


def detect_target_agent(target: Path, explicit: str | None) -> str:
    if explicit:
        return explicit
    for marker, agent in [
        (".codex", "codex"),
        (".claude", "claude"),
        (".gemini", "gemini"),
        (".opencode", "opencode"),
    ]:
        if (target / marker).exists():
            return agent
    return "codex" if (Path.home() / ".codex").exists() else "claude"


def resolve_target_dir(agent: str, target: Path) -> Path:
    mapping = {
        "claude": target / ".claude" / "skills",
        "codex": target / ".codex" / "skills",
        "gemini": target / ".gemini" / "skills",
        "opencode": target / ".opencode" / "skills",
    }
    return mapping[agent]


def choose_profile(registry: dict, explicit_profile: str | None, explicit_tags: str | None) -> tuple[str | None, list[str]]:
    if explicit_tags:
        return None, [tag.strip() for tag in explicit_tags.split(",") if tag.strip()]
    if explicit_profile:
        return explicit_profile, resolve_profile_tags(registry, explicit_profile)

    project_tags = [tag.strip() for tag in os.environ.get("PM_PROJECT_TAGS", "").split(",") if tag.strip()]
    profiles = registry.get("profiles", {})
    best_profile = None
    best_score = 0

    for name, profile in profiles.items():
        if name == "full":
            continue
        tags = profile.get("tags", [])
        if "*" in tags:
            continue
        score = len(set(project_tags) & set(tags))
        if score > best_score:
            best_score = score
            best_profile = name

    if best_profile:
        return best_profile, resolve_profile_tags(registry, best_profile)
    if "dev" in profiles:
        return "dev", resolve_profile_tags(registry, "dev")
    if "full" in profiles:
        return "full", resolve_profile_tags(registry, "full")
    return None, []


def deployed_skill_dirs() -> list[tuple[str, Path]]:
    dirs = []
    project_path = os.environ.get("PM_PROJECT_PATH")
    if project_path:
        target = Path(project_path).expanduser()
        dirs.extend(
            [
                ("claude", target / ".claude" / "skills"),
                ("codex", target / ".codex" / "skills"),
                ("gemini", target / ".gemini" / "skills"),
                ("opencode", target / ".opencode" / "skills"),
            ]
        )
    dirs.extend(
        [
            ("claude", Path.home() / ".claude" / "skills"),
            ("codex", Path.home() / ".codex" / "skills"),
            ("gemini", Path.home() / ".gemini" / "skills"),
            ("opencode", Path.home() / ".opencode" / "skills"),
        ]
    )
    return [(agent, path) for agent, path in dirs if path.exists()]


def cmd_list(args: argparse.Namespace) -> None:
    registry = load_registry()
    skills = registry.get("skills", [])

    filtered = skills
    if args.source:
        filtered = [skill for skill in filtered if skill.get("source") == args.source]
    if args.agent:
        filtered = [skill for skill in filtered if skill_matches_agent(skill, args.agent)]
    if args.tag:
        filtered = [skill for skill in filtered if skill_matches_tags(skill, [args.tag])]
    if not args.disabled:
        filtered = [skill for skill in filtered if not skill.get("disabled", False)]

    if args.format == "json":
        print(json.dumps(filtered, indent=2, ensure_ascii=False))
        return
    if args.format == "yaml":
        print(yaml.dump(filtered, default_flow_style=False, allow_unicode=True, sort_keys=False))
        return

    rows = []
    for skill in filtered:
        rows.append(
            [
                skill["name"],
                skill.get("source", "?"),
                skill.get("agent", "?"),
                ", ".join(skill.get("tags", [])),
            ]
        )
    if rows:
        print(format_table(rows, ["NAME", "SOURCE", "AGENT", "TAGS"]))
        print(f"\n({len(rows)} skills)")
    else:
        print("No skills found matching filters.")


def cmd_info(args: argparse.Namespace) -> None:
    writer = RegistryWriter()
    writer.load()
    skill = writer.find_skill(args.name)

    if not skill:
        similar = writer.find_similar(args.name)
        msg = f"Skill '{args.name}' not found."
        if similar:
            msg += f"\n  Did you mean: {', '.join(similar)}"
        print(msg, file=sys.stderr)
        sys.exit(1)

    resolved = expand_path(skill.get("path"))
    exists = bool(resolved and resolved.exists())
    has_skill_md = bool(resolved and (resolved / "SKILL.md").exists())

    print(f"Name:        {skill['name']}")
    print(f"Source:      {skill.get('source', '?')}")
    print(f"Agent:       {skill.get('agent', '?')}")
    print(f"Tags:        {', '.join(skill.get('tags', []))}")
    print(f"Path:        {skill.get('path', 'null')}")
    print(f"Resolved:    {resolved or 'N/A'}")
    print(f"Exists:      {'yes' if exists else 'no'}")
    print(f"SKILL.md:    {'yes' if has_skill_md else 'no'}")
    print(f"Description: {skill.get('description', '')}")
    if skill.get("install_source"):
        print(f"Install src: {skill['install_source']}")
    if skill.get("disabled"):
        print("Disabled:    YES")
    if skill.get("created_at"):
        print(f"Created:     {skill['created_at']}")
    if skill.get("updated_at"):
        print(f"Updated:     {skill['updated_at']}")


def cmd_deploy(args: argparse.Namespace) -> None:
    registry = load_registry()
    skills = registry.get("skills", [])
    profile_name, tags = choose_profile(registry, args.profile, args.tags)
    if not tags:
        print("Error: no deploy tags resolved. Use a profile or --tags.", file=sys.stderr)
        sys.exit(1)

    target_raw = args.target or os.environ.get("PM_PROJECT_PATH")
    if not target_raw:
        print("Error: no target resolved. Run inside a PM project or pass --target.", file=sys.stderr)
        sys.exit(1)
    target = Path(os.path.expanduser(target_raw)).resolve()

    agent = detect_target_agent(target, args.agent)
    skills_dir = resolve_target_dir(agent, target)

    deployable = []
    for skill in skills:
        if skill.get("disabled", False):
            continue
        if skill.get("source") == "builtin":
            continue
        if not skill_matches_tags(skill, tags):
            continue
        if not skill_matches_agent(skill, agent):
            continue
        resolved = expand_path(skill.get("path"))
        if resolved is None:
            continue
        skill_md = resolved / "SKILL.md"
        if not skill_md.exists():
            if not args.dry_run:
                print(f"  ⚠ SKIP {skill['name']}: SKILL.md not found at {resolved}", file=sys.stderr)
            continue
        deployable.append((skill["name"], resolved))

    if not deployable:
        print("No skills matched the given filters.")
        return

    if not args.dry_run:
        skills_dir.mkdir(parents=True, exist_ok=True)

    created = 0
    skipped = 0
    for name, source_path in deployable:
        link_path = skills_dir / name
        if args.dry_run:
            if link_path.is_symlink() or link_path.exists():
                print(f"  [DRY-RUN] EXISTS  {name}")
                skipped += 1
            else:
                print(f"  [DRY-RUN] CREATE  {link_path} -> {source_path}")
                created += 1
            continue

        if link_path.is_symlink():
            if link_path.resolve() == source_path:
                skipped += 1
                continue
            link_path.unlink()
        elif link_path.exists():
            if not args.force:
                print(f"  ⚠ SKIP {name}: non-symlink exists (use --force)", file=sys.stderr)
                skipped += 1
                continue
            if link_path.is_dir():
                shutil.rmtree(link_path)
            else:
                link_path.unlink()

        link_path.symlink_to(source_path)
        created += 1

    removed = 0
    if args.clean and not args.dry_run and skills_dir.exists():
        deployed_names = {name for name, _ in deployable}
        for entry in skills_dir.iterdir():
            if entry.is_symlink() and entry.name not in deployed_names:
                entry.unlink()
                removed += 1

    prefix = "[DRY-RUN] " if args.dry_run else ""
    if profile_name:
        print(f"{prefix}Profile: {profile_name}")
    print(f"{prefix}Tags: {', '.join(tags)}")
    print(f"{prefix}Agent: {agent}")
    print(f"{prefix}Target: {skills_dir}")
    print(f"{prefix}Created: {created}, Skipped: {skipped}, Deployable: {len(deployable)}, Removed: {removed}")


def cmd_verify(args: argparse.Namespace) -> None:
    registry = load_registry()
    skills = registry.get("skills", [])
    errors = []
    warnings = []

    required_fields = ["name", "source", "agent", "tags", "description"]
    for index, skill in enumerate(skills):
        for field in required_fields:
            if field not in skill or skill[field] is None:
                if field == "tags" and skill.get("source") == "builtin":
                    continue
                errors.append(f"Skill #{index} ({skill.get('name', '?')}): missing '{field}'")

    seen_names: dict[str, str] = {}
    for skill in skills:
        if skill.get("source") not in VALID_SOURCES:
            errors.append(f"{skill['name']}: invalid source '{skill.get('source')}'")
        if skill.get("agent") not in VALID_AGENTS:
            errors.append(f"{skill['name']}: invalid agent '{skill.get('agent')}'")
        lower = skill["name"].lower()
        if lower in seen_names:
            errors.append(f"Duplicate name: '{skill['name']}' conflicts with '{seen_names[lower]}'")
        seen_names[lower] = skill["name"]

    for skill in skills:
        if skill.get("source") == "builtin" or skill.get("disabled", False):
            continue
        resolved = expand_path(skill.get("path"))
        if resolved is None:
            errors.append(f"{skill['name']}: path is null")
            continue
        if not resolved.exists():
            errors.append(f"{skill['name']}: path does not exist: {resolved}")
        elif not (resolved / "SKILL.md").exists():
            warnings.append(f"{skill['name']}: SKILL.md missing at {resolved}")

    if errors:
        print("ERRORS:")
        for error in errors:
            print(f"  * {error}")
    if warnings:
        print("WARNINGS:")
        for warning in warnings:
            print(f"  * {warning}")
    if not errors and not warnings:
        print("Registry is consistent. All checks passed.")

    print(f"\nSummary: {len(skills)} skills")
    print(f"Errors: {len(errors)}, Warnings: {len(warnings)}")
    sys.exit(1 if errors else 0)


def cmd_scan(args: argparse.Namespace) -> None:
    registry = load_registry()
    skills = registry.get("skills", [])

    registered_paths = set()
    registered_names = set()
    for skill in skills:
        registered_names.add(skill["name"].lower())
        resolved = expand_path(skill.get("path"))
        if resolved:
            registered_paths.add(resolved)

    scan_dirs = [
        ("custom", SKILLS_DIR / "custom"),
        ("downloaded", SKILLS_DIR / "downloaded"),
        ("codex", Path.home() / ".codex" / "skills"),
        ("claude", Path.home() / ".claude" / "skills"),
    ]

    found = []
    for label, scan_dir in scan_dirs:
        if not scan_dir.exists():
            continue
        for entry in scan_dir.iterdir():
            if entry.name.startswith("."):
                continue
            resolved = entry.resolve() if entry.is_symlink() else entry
            if not resolved.is_dir() or not (resolved / "SKILL.md").exists():
                continue
            if resolved in registered_paths or entry.name.lower() in registered_names:
                continue

            description = ""
            try:
                with open(resolved / "SKILL.md") as handle:
                    for line in handle:
                        if line.startswith("description:"):
                            description = line.split(":", 1)[1].strip().strip('"').strip("'")[:120]
                            break
            except Exception:
                pass

            source_guess = "custom" if label == "custom" else "downloaded"
            found.append(
                {
                    "name": entry.name,
                    "path": rel_store_path(resolved) if resolved.is_relative_to(STORE_DIR) else str(resolved),
                    "source_guess": source_guess,
                    "location": label,
                    "description": description,
                }
            )

    if not found:
        print("No unregistered skills found.")
        return

    print(f"Found {len(found)} unregistered skill(s):\n")
    for item in found:
        print(f"  {item['name']}")
        print(f"    Path:   {item['path']}")
        print(f"    Source: {item['source_guess']} (from {item['location']})")
        if item["description"]:
            print(f"    Desc:   {item['description']}")
        print()

    if args.register:
        writer = RegistryWriter()
        writer.load()
        added = 0
        for item in found:
            new_skill = {
                "name": item["name"],
                "source": item["source_guess"],
                "agent": "codex" if item["location"] == "codex" else "claude" if item["location"] == "claude" else "universal",
                "tags": [],
                "path": item["path"],
                "description": item["description"] or f"(no description) {item['name']}",
            }
            try:
                writer.add_skill(new_skill)
                added += 1
                print(f"  + Registered {item['name']}")
            except ValueError as error:
                print(f"  ! Skip {item['name']}: {error}", file=sys.stderr)
        print(f"\n{added} skills added to registry.")


def cmd_add(args: argparse.Namespace) -> None:
    writer = RegistryWriter()
    writer.load()

    tags = sorted(set(tag.strip() for tag in args.tags.split(",") if tag.strip()))
    entry = {
        "name": args.name,
        "source": args.source,
        "agent": args.agent,
        "tags": tags,
        "description": args.description or "",
    }

    if args.source == "custom":
        if args.path:
            resolved = expand_path(args.path)
        else:
            category = args.category or (tags[0] if tags else "misc")
            resolved = SKILLS_DIR / "custom" / category / args.name
        assert resolved is not None
        entry["path"] = rel_store_path(resolved)
        skill_md = resolved / "SKILL.md"
        if not skill_md.exists():
            resolved.mkdir(parents=True, exist_ok=True)
            skill_md.write_text(SKILL_MD_TEMPLATE.format(name=args.name, description=args.description or args.name))
            print(f"  Created {skill_md}")
    elif args.path:
        resolved = expand_path(args.path)
        assert resolved is not None
        entry["path"] = rel_store_path(resolved)

    if args.install_source:
        entry["install_source"] = args.install_source

    try:
        writer.add_skill(entry)
        print(f"  + Added '{args.name}'")
    except ValueError as error:
        print(f"Error: {error}", file=sys.stderr)
        sys.exit(1)


def cmd_remove(args: argparse.Namespace) -> None:
    writer = RegistryWriter()
    writer.load()

    skill = writer.find_skill(args.name)
    if not skill:
        print(f"Error: Skill '{args.name}' not found.", file=sys.stderr)
        sys.exit(1)

    if not args.force and sys.stdin.isatty():
        print(f"Will remove: {skill['name']}")
        if args.delete_files:
            print(f"  Will also delete: {expand_path(skill.get('path'))}")
        if input("Proceed? [y/N] ").strip().lower() != "y":
            print("Cancelled.")
            return

    removed = writer.remove_skill(args.name)
    print(f"  - Removed '{removed['name']}' from registry")

    if args.delete_files:
        resolved = expand_path(removed.get("path"))
        if resolved and resolved.exists():
            shutil.rmtree(resolved)
            print(f"  - Deleted {resolved}")


def cmd_update(args: argparse.Namespace) -> None:
    writer = RegistryWriter()
    writer.load()
    skill = writer.find_skill(args.name)
    if not skill:
        print(f"Error: Skill '{args.name}' not found.", file=sys.stderr)
        sys.exit(1)

    if args.set_tags and (args.add_tags or args.remove_tags):
        print("Error: --set-tags cannot be used with --add-tags or --remove-tags", file=sys.stderr)
        sys.exit(1)

    updates = {}
    if args.set_tags:
        updates["tags"] = sorted(set(tag.strip() for tag in args.set_tags.split(",") if tag.strip()))
    else:
        current_tags = set(skill.get("tags", []))
        changed = False
        if args.add_tags:
            current_tags |= set(tag.strip() for tag in args.add_tags.split(",") if tag.strip())
            changed = True
        if args.remove_tags:
            current_tags -= set(tag.strip() for tag in args.remove_tags.split(",") if tag.strip())
            changed = True
        if changed:
            updates["tags"] = sorted(current_tags)

    if args.agent:
        updates["agent"] = args.agent
    if args.description is not None:
        updates["description"] = args.description
    if args.source:
        updates["source"] = args.source
    if args.path:
        resolved = expand_path(args.path)
        assert resolved is not None
        updates["path"] = rel_store_path(resolved)

    if not updates:
        print("Nothing to update.")
        return

    before, after = writer.update_skill(args.name, updates)
    print(f"Updated '{args.name}':")
    for key in updates:
        if before.get(key) != after.get(key):
            print(f"  {key}: {before.get(key, '(none)')} -> {after.get(key, '(none)')}")


def cmd_import(args: argparse.Namespace) -> None:
    source_path = Path(os.path.expanduser(args.path)).resolve()
    if source_path.name == "SKILL.md":
        source_dir = source_path.parent
    elif (source_path / "SKILL.md").exists():
        source_dir = source_path
    else:
        print(f"Error: SKILL.md not found at {source_path}", file=sys.stderr)
        sys.exit(1)

    name = args.name or source_dir.name
    agent = args.agent
    if not agent:
        path_str = str(source_dir)
        if ".codex/" in path_str:
            agent = "codex"
        elif ".claude/" in path_str:
            agent = "claude"
        else:
            agent = "universal"

    tags = sorted(set(tag.strip() for tag in args.tags.split(",") if tag.strip()))
    category = args.category or "imported"
    target_dir = SKILLS_DIR / "custom" / category / name
    if target_dir.exists():
        print(f"Error: target directory already exists: {target_dir}", file=sys.stderr)
        sys.exit(1)

    def ignore_fn(directory, contents):
        return [item for item in contents if item in COPY_IGNORE]

    shutil.copytree(source_dir, target_dir, ignore=ignore_fn)
    print(f"  Copied {source_dir} -> {target_dir}")

    entry = {
        "name": name,
        "source": "custom",
        "agent": agent,
        "tags": tags,
        "path": rel_store_path(target_dir),
        "description": "",
        "import_origin": str(source_dir),
    }
    writer = RegistryWriter()
    writer.load()
    try:
        writer.add_skill(entry)
        print(f"  + Registered '{name}'")
    except ValueError as error:
        shutil.rmtree(target_dir)
        print(f"Error: {error}", file=sys.stderr)
        sys.exit(1)


def _install_github(spec: str, target: Path) -> bool:
    parts = spec.split("/", 2)
    if len(parts) < 2:
        raise ValueError(f"Invalid github spec: {spec}")
    owner, repo = parts[0], parts[1]
    subpath = parts[2] if len(parts) > 2 else ""
    target.mkdir(parents=True, exist_ok=True)

    if subpath:
        api_url = f"/repos/{owner}/{repo}/contents/{subpath}"
        result = subprocess.run(["gh", "api", api_url, "--jq", ".[] | .download_url"], capture_output=True, text=True)
        if result.returncode != 0:
            raise RuntimeError(result.stderr.strip())
        for url in result.stdout.strip().split("\n"):
            if url:
                filename = url.rsplit("/", 1)[-1]
                subprocess.run(["curl", "-sL", "-o", str(target / filename), url], check=True)
    else:
        result = subprocess.run(
            ["gh", "repo", "clone", f"{owner}/{repo}", str(target), "--", "--depth=1"],
            capture_output=True,
            text=True,
        )
        if result.returncode != 0:
            raise RuntimeError(result.stderr.strip())

    return (target / "SKILL.md").exists() or any(target.iterdir())


def _install_url(url: str, target: Path) -> bool:
    import urllib.request

    target.mkdir(parents=True, exist_ok=True)
    dest = target / "SKILL.md"
    urllib.request.urlretrieve(url, str(dest))
    return dest.exists()


def _install_local(path_str: str, target: Path) -> bool:
    source = Path(os.path.expanduser(path_str)).resolve()
    if not source.exists():
        raise FileNotFoundError(source)

    def ignore_fn(directory, contents):
        return [item for item in contents if item in COPY_IGNORE]

    if target.exists():
        shutil.rmtree(target)
    shutil.copytree(source, target, ignore=ignore_fn)
    return (target / "SKILL.md").exists()


def cmd_install(args: argparse.Namespace) -> None:
    writer = RegistryWriter()
    writer.load()
    only = set(args.only.split(",")) if args.only else None
    downloaded = [skill for skill in writer.skills if skill.get("source") == "downloaded"]
    if only:
        downloaded = [skill for skill in downloaded if skill["name"] in only]
    if not downloaded:
        print("No downloaded skills to install.")
        return

    installed = 0
    skipped = 0
    failed = []

    for skill in downloaded:
        name = skill["name"]
        install_source = skill.get("install_source", "")
        target = SKILLS_DIR / "downloaded" / name
        if target.exists() and not args.force:
            skipped += 1
            continue
        if not install_source:
            failed.append((name, "no install_source defined"))
            continue

        print(f"  Installing {name}...", end=" ")
        try:
            if install_source.startswith("github:"):
                success = _install_github(install_source[7:], target)
            elif install_source.startswith("url:"):
                success = _install_url(install_source[4:], target)
            elif install_source.startswith("local:"):
                success = _install_local(install_source[6:], target)
            else:
                raise ValueError(f"unknown install_source: {install_source}")

            if success:
                skill["path"] = rel_store_path(target)
                installed += 1
                print("OK")
            else:
                failed.append((name, "installation returned false"))
                print("FAIL")
        except Exception as error:
            failed.append((name, str(error)))
            print("FAIL")

    writer.save()
    print(f"\nInstalled: {installed}, Skipped: {skipped}, Failed: {len(failed)}")
    if failed:
        for name, reason in failed:
            print(f"  * {name}: {reason}")


def cmd_diff(args: argparse.Namespace) -> None:
    writer = RegistryWriter()
    writer.load()
    entries = []

    for skill in writer.skills:
        if skill.get("source") == "builtin":
            continue
        if skill.get("disabled", False):
            entries.append(("DISABLED", skill["name"], "skipped (disabled)"))
            continue
        resolved = expand_path(skill.get("path"))
        if resolved is None:
            entries.append(("MISSING", skill["name"], "path is null"))
            continue
        if not resolved.exists():
            entries.append(("MISSING", skill["name"], f"{resolved} not found"))
        elif not (resolved / "SKILL.md").exists():
            entries.append(("PARTIAL", skill["name"], f"SKILL.md missing at {resolved}"))
        else:
            entries.append(("OK", skill["name"], ""))

    registered_paths = {expand_path(skill.get("path")) for skill in writer.skills if skill.get("path")}
    registered_names = {skill["name"].lower() for skill in writer.skills}

    scan_dirs = [SKILLS_DIR / "custom", SKILLS_DIR / "downloaded"]
    for _, deploy_dir in deployed_skill_dirs():
        scan_dirs.append(deploy_dir)

    for scan_dir in scan_dirs:
        if not scan_dir.exists():
            continue
        for entry in scan_dir.iterdir():
            if entry.name.startswith("."):
                continue
            resolved = entry.resolve() if entry.is_symlink() else entry
            if not resolved.is_dir() or not (resolved / "SKILL.md").exists():
                continue
            if resolved in registered_paths:
                continue
            if entry.name.lower() in registered_names:
                entries.append(("MOVED", entry.name, f"found at {resolved} but registered elsewhere"))
            else:
                entries.append(("EXTRA", entry.name, f"{resolved} not in registry"))

    if args.format == "json":
        print(json.dumps([{"status": s, "name": n, "detail": d} for s, n, d in entries], indent=2))
    else:
        problem_entries = [(s, n, d) for s, n, d in entries if s != "OK"]
        if problem_entries:
            print(format_table([[s, n, d] for s, n, d in problem_entries], ["STATUS", "NAME", "DETAIL"]))
            print()
        ok_count = sum(1 for s, _, _ in entries if s == "OK")
        print(f"Total: {len(entries)}, OK: {ok_count}, Issues: {len(problem_entries)}")

    sys.exit(1 if any(s not in ("OK", "DISABLED") for s, _, _ in entries) else 0)


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(prog="pm skills", description="Centralized skill registry manager")
    sub = parser.add_subparsers(dest="command")

    p_list = sub.add_parser("list", help="List skills")
    p_list.add_argument("--source", choices=sorted(VALID_SOURCES))
    p_list.add_argument("--agent", choices=sorted(VALID_AGENTS))
    p_list.add_argument("--tag")
    p_list.add_argument("--disabled", action="store_true")
    p_list.add_argument("--format", choices=["table", "json", "yaml"], default="table")

    p_info = sub.add_parser("info", help="Show skill info")
    p_info.add_argument("name")

    p_deploy = sub.add_parser("deploy", help="Deploy skills")
    p_deploy.add_argument("profile", nargs="?")
    p_deploy.add_argument("--tags")
    p_deploy.add_argument("--target")
    p_deploy.add_argument("--agent", choices=sorted(VALID_AGENTS - {"universal"}))
    p_deploy.add_argument("--dry-run", action="store_true")
    p_deploy.add_argument("--clean", action="store_true")
    p_deploy.add_argument("--force", action="store_true")

    sub.add_parser("verify", help="Verify registry")

    p_scan = sub.add_parser("scan", help="Scan filesystem")
    p_scan.add_argument("--register", action="store_true")

    p_add = sub.add_parser("add", help="Register a new skill")
    p_add.add_argument("name")
    p_add.add_argument("--source", required=True, choices=sorted(VALID_SOURCES))
    p_add.add_argument("--agent", required=True, choices=sorted(VALID_AGENTS))
    p_add.add_argument("--tags", required=True)
    p_add.add_argument("--path")
    p_add.add_argument("--category")
    p_add.add_argument("--description", default="")
    p_add.add_argument("--install-source", dest="install_source")

    p_remove = sub.add_parser("remove", help="Remove a skill")
    p_remove.add_argument("name")
    p_remove.add_argument("--delete-files", action="store_true")
    p_remove.add_argument("--force", action="store_true")

    p_update = sub.add_parser("update", help="Update a skill")
    p_update.add_argument("name")
    p_update.add_argument("--add-tags")
    p_update.add_argument("--remove-tags")
    p_update.add_argument("--set-tags")
    p_update.add_argument("--agent", choices=sorted(VALID_AGENTS))
    p_update.add_argument("--source")
    p_update.add_argument("--description")
    p_update.add_argument("--path")

    p_import = sub.add_parser("import", help="Import a skill")
    p_import.add_argument("path")
    p_import.add_argument("--name")
    p_import.add_argument("--category", default="imported")
    p_import.add_argument("--agent", choices=sorted(VALID_AGENTS))
    p_import.add_argument("--tags", required=True)

    p_install = sub.add_parser("install", help="Install downloaded skills")
    p_install.add_argument("--only")
    p_install.add_argument("--force", action="store_true")

    p_diff = sub.add_parser("diff", help="Compare registry with filesystem")
    p_diff.add_argument("--format", choices=["table", "json"], default="table")

    return parser


def main() -> None:
    ensure_store()
    parser = build_parser()
    args = parser.parse_args()

    if args.command is None:
        parser.print_help()
        return

    commands = {
        "list": cmd_list,
        "info": cmd_info,
        "deploy": cmd_deploy,
        "verify": cmd_verify,
        "scan": cmd_scan,
        "add": cmd_add,
        "remove": cmd_remove,
        "update": cmd_update,
        "import": cmd_import,
        "install": cmd_install,
        "diff": cmd_diff,
    }
    commands[args.command](args)


if __name__ == "__main__":
    main()
