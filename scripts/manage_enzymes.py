#!/usr/bin/env python3
"""Install a pinned REBASE enzyme release for DNAgent. Python 3.11+, no pip dependencies.

REBASE (http://rebase.neb.com) is "Copyright (c) Dr. Richard J. Roberts ... All rights
reserved" and states no redistribution licence, so DNAgent never commits it. This script
downloads the EMBOSS-format files of one pinned release for your own local use, checks
their SHA-256, and writes a manifest that `dnagent` and the desktop app load. Without it,
DNAgent uses its hand-curated built-in enzyme set. Please cite REBASE (Roberts et al.,
Nucleic Acids Res. 2023; 51:D629-D630) when you use results that depend on it.

    python3 scripts/manage_enzymes.py plan      # show what would be installed
    python3 scripts/manage_enzymes.py install   # download, verify, publish
    python3 scripts/manage_enzymes.py verify    # re-check an installed release
"""

import argparse
from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path
import shutil
import sys
import tempfile
import urllib.request

RELEASE = "609"
# Pinned on 2026-09-29. REBASE serves the current release at the link URLs; when it moves
# on, the hash check fails and a new release has to be pinned deliberately.
FILES = {
    "emboss_e": "8db05883737a7942bc2254a5d6d07289cfc1ef17f51afcfe402a1df49fbbbe33",
    "emboss_r": "0dd7d945bc2c1681a714c5fd6ac441929d50ca1dc027a6f67d54c11dbe9ef900",
    "emboss_s": "7e7bf2d42eeec21fb14d00f0e8b25fc7a3af413ceb83fc88c3f3386f1b9551aa",
}
# Only the current-release links are served (versioned file names return 404). Plain HTTP
# is a fallback for Python builds without CA certificates; integrity comes from the
# pinned SHA-256 either way.
SOURCES = ["https://rebase.neb.com/rebase/link_{name}", "http://rebase.neb.com/rebase/link_{name}"]
MAX_BYTES = 8 * 1024**2


def default_root() -> Path:
    override = os.environ.get("DNAGENT_ENZYME_DIR")
    if override:
        return Path(override).expanduser()
    if sys.platform == "darwin":
        return Path.home() / "Library/Application Support/DNAgent/enzymes"
    return Path(os.environ.get("XDG_DATA_HOME", str(Path.home() / ".local/share"))) / "dnagent/enzymes"


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def fetch(name: str, target: Path) -> str:
    errors = []
    for template in SOURCES:
        url = template.format(name=name, release=RELEASE)
        try:
            with urllib.request.urlopen(url, timeout=60) as response:
                data = response.read(MAX_BYTES + 1)
            if len(data) > MAX_BYTES:
                raise ValueError("file larger than expected")
            target.write_bytes(data)
            digest = sha256(target)
            if digest == FILES[name]:
                return url
            header = data[:400].decode("ascii", "replace")
            errors.append(f"{url}: SHA-256 {digest} does not match the pinned release {RELEASE} ({header.splitlines()[1].strip() if len(header.splitlines()) > 1 else 'no header'})")
        except Exception as error:  # noqa: BLE001 - reported below
            errors.append(f"{url}: {error}")
    raise SystemExit(f"could not obtain {name}.{RELEASE}:\n  " + "\n  ".join(errors))


def release_dir(root: Path) -> Path:
    return root / f"rebase-{RELEASE}"


def verify(directory: Path) -> dict:
    manifest = json.loads((directory / "manifest.json").read_text())
    for name, expected in FILES.items():
        path = directory / f"{name}.{RELEASE}"
        if sha256(path) != expected or manifest["files"][name]["sha256"] != expected:
            raise SystemExit(f"{path} does not match the pinned release")
    return manifest


def install(root: Path) -> None:
    target = release_dir(root)
    if (target / "manifest.json").exists():
        verify(target)
        print(f"already installed and verified: {target}")
        return
    root.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(dir=root, prefix=".staging-") as staging:
        staging_dir = Path(staging)
        records = {}
        for name in FILES:
            path = staging_dir / f"{name}.{RELEASE}"
            url = fetch(name, path)
            records[name] = {"file": path.name, "sha256": FILES[name], "url": url}
        manifest = {
            "format": "dnagent-enzyme-catalogue",
            "version": 1,
            "provider": "REBASE",
            "release": RELEASE,
            "installed": datetime.now(timezone.utc).isoformat(timespec="seconds"),
            "licence_note": "REBASE: Copyright (c) Dr. Richard J. Roberts. Downloaded for local use; not redistributed by DNAgent.",
            "files": records,
        }
        (staging_dir / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n")
        shutil.move(str(staging_dir), str(target))
        Path(staging).mkdir(exist_ok=True)  # let TemporaryDirectory clean up quietly
    verify(target)
    print(f"installed REBASE {RELEASE} into {target}")


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("command", choices=["plan", "install", "verify"])
    parser.add_argument("--root", type=Path, default=None, help="data folder (default: DNAGENT_ENZYME_DIR or the platform data folder)")
    args = parser.parse_args()
    root = (args.root or default_root()).expanduser()
    if args.command == "plan":
        print(f"REBASE release {RELEASE} (EMBOSS format) -> {release_dir(root)}")
        for name, digest in FILES.items():
            print(f"  {name}.{RELEASE}  sha256 {digest}")
    elif args.command == "install":
        install(root)
    else:
        manifest = verify(release_dir(root))
        print(f"verified REBASE {manifest['release']} in {release_dir(root)}")


if __name__ == "__main__":
    main()
