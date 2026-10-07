#!/usr/bin/env python3
#/**
# * @file third-party-notices.py
# * @author doraemon-hub-art (1660219734@qq.com)
# * @brief Write the third-party notices that travel in the package
# * @date 2026-10-07
# *
# * @copyright Copyright (c) 2026
# */

# Every dependency that ends up in the build is listed with its name, its version and the licence it
# is under, and the licence texts found beside those dependencies are collected once each. The two
# lockfiles and the trees they describe are the only sources: nothing is fetched, and nothing is
# written down from memory.
#
#   scripts/third-party-notices.py [--out FILE]
#
# Reads Cargo.lock, package-lock.json and node_modules from the repository this file sits in, and
# the crate sources from CARGO_HOME (default ~/.cargo).

import argparse
import json
import os
import re
import sys
from pathlib import Path

LICENCE_FILES = ("LICENSE", "LICENCE", "LICENSE.md", "LICENSE.txt", "LICENCE.txt", "COPYING", "COPYING.txt")
COPYLEFT = re.compile(r"GPL|MPL|CDDL|EPL|SSPL|Commons", re.I)


def read_text(path):
    try:
        return Path(path).read_text(encoding="utf-8", errors="replace").strip()
    except OSError:
        return ""


def licence_refs(directory):
    """The licence file inside a package directory, if it carries one."""
    for name in LICENCE_FILES:
        for entry in sorted(Path(directory).glob(f"{name}*")):
            if entry.is_file():
                return entry
    return None


def rust_packages(root):
    lock = read_text(root / "Cargo.lock")
    pairs = re.findall(r'\[\[package\]\]\nname = "([^"]+)"\nversion = "([^"]+)"', lock)
    registry = Path(os.environ.get("CARGO_HOME", Path.home() / ".cargo")) / "registry" / "src"
    sources = sorted(registry.glob("*"))
    out = []
    for name, version in pairs:
        for base in sources:
            manifest = base / f"{name}-{version}" / "Cargo.toml"
            if not manifest.exists():
                continue
            text = read_text(manifest)
            found = re.search(r'^license *= *(?:"([^"\n]+)"|([^\n]+))', text, re.M)
            licence = (found.group(1) or found.group(2)).strip() if found else ""
            out.append((name, version, licence, licence_refs(manifest.parent)))
            break
    return out


def npm_packages(root):
    lock = json.loads(read_text(root / "package-lock.json") or "{}")
    out = []
    for name, entry in sorted((lock.get("packages") or {}).items()):
        if not name.startswith("node_modules/"):
            continue
        folder = root / name
        manifest = folder / "package.json"
        if not manifest.exists():
            continue
        meta = json.loads(read_text(manifest) or "{}")
        licence = meta.get("license") or ""
        if isinstance(licence, dict):
            licence = licence.get("type", "")
        out.append((name.split("node_modules/")[-1], meta.get("version", entry.get("version", "")), licence, licence_refs(folder)))
    return out


def render(root, rust, npm):
    texts = {}
    lines = [
        "THIRD-PARTY NOTICES",
        "===================",
        "",
        "The components below are what the two lockfiles describe, each with the licence it is",
        "under. The list is the whole lock, so a few entries are only pulled in on platforms",
        "other than Linux. The licence texts found beside those components follow at the end.",
        "",
        "This program is free software under GPL-3.0-only; the source is at",
        "https://github.com/doraemon-hub-art/more-effective-logger",
        "",
    ]
    for title, packages in (("Rust crates", rust), ("npm packages", npm)):
        grouped = {}
        for name, version, licence, ref in packages:
            grouped.setdefault(licence or "(no licence field)", []).append(f"{name} {version}")
            if licence and licence not in texts and ref is not None:
                texts[licence] = read_text(ref)
        lines.append(f"{title} ({len(packages)})")
        lines.append("-" * len(f"{title} ({len(packages)})"))
        for licence in sorted(grouped, key=lambda l: (-len(grouped[l]), l)):
            names = grouped[licence]
            lines.append("")
            lines.append(f"{licence} ({len(names)})")
            lines.append("    " + ", ".join(names))
        lines.append("")
    lines.append("Licence texts")
    lines.append("-------------")
    for licence in sorted(texts):
        if not texts[licence]:
            continue
        lines.append("")
        lines.append("=" * 78)
        lines.append(licence)
        lines.append("=" * 78)
        lines.append("")
        lines.append(texts[licence])
        lines.append("")
    missing = [l for l in sorted(texts) if not texts[l]]
    if missing:
        lines.append("")
        lines.append("No text was found beside these, they name the licence on their own: " + ", ".join(missing))
    return "\n".join(lines) + "\n"


def main():
    parser = argparse.ArgumentParser(description="Write the third-party notices of this repository.")
    parser.add_argument("--root", default=str(Path(__file__).resolve().parent.parent))
    parser.add_argument("--out", default=None, help="default: target/third-party-notices.txt")
    args = parser.parse_args()
    root = Path(args.root).resolve()
    out = Path(args.out) if args.out else root / "target" / "third-party-notices.txt"

    rust = rust_packages(root)
    npm = npm_packages(root)
    if not rust and not npm:
        print("nothing to collect: no Cargo.lock and no package-lock.json under " + str(root), file=sys.stderr)
        return 1

    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(render(root, rust, npm), encoding="utf-8")
    print(f"wrote {out} ({len(rust)} crates, {len(npm)} npm packages, {out.stat().st_size} bytes)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
