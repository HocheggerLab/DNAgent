#!/usr/bin/env python3
"""Install pinned public references and native BLAST v5 indexes. Python 3.11+, no pip dependencies.

This is provisioning, not biological analysis or a primer-specificity claim.
"""

import argparse
from datetime import datetime, timezone
import gzip
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tempfile
import urllib.request

PROFILE_PATH = (
    Path(__file__).resolve().parents[1] / "references/human-refseq-grch38.json"
)
MIN_FREE_BYTES = 20 * 1024**3
CHUNK = 1024**2


def default_root():
    override = os.environ.get("DNAGENT_REFERENCE_DIR")
    if override:
        return Path(override).expanduser()
    if sys.platform == "darwin":
        return Path.home() / "Library/Application Support/DNAagent/references"
    return (
        Path(os.environ.get("XDG_DATA_HOME", str(Path.home() / ".local/share")))
        / "dnagent/references"
    )


def digest(path, algorithm="sha256"):
    h = hashlib.new(algorithm)
    with path.open("rb") as stream:
        while data := stream.read(CHUNK):
            h.update(data)
    return h.hexdigest()


def copy_limited(source, target, maximum):
    count = 0
    while data := source.read(CHUNK):
        count += len(data)
        if count > maximum:
            raise ValueError(f"source exceeds declared byte limit ({maximum})")
        target.write(data)
    return count


def download(url, path, maximum):
    if not url.startswith("https://ftp.ncbi.nlm.nih.gov/"):
        raise ValueError("only HTTPS public NCBI source downloads are supported")
    request = urllib.request.Request(
        url, headers={"User-Agent": "DNAagent-reference-installer/1"}
    )
    with urllib.request.urlopen(request, timeout=120) as source, path.open(
        "xb"
    ) as target:
        if not source.geturl().startswith("https://ftp.ncbi.nlm.nih.gov/"):
            raise ValueError("unexpected reference download redirect")
        copy_limited(source, target, maximum)


def check_tools():
    tools = {}
    for name in ("makeblastdb", "blastdbcmd", "blastn"):
        path = shutil.which(name)
        if not path:
            raise RuntimeError(
                f"{name} is missing. Install NCBI BLAST+ first (macOS: brew install blast). No download started."
            )
        version = subprocess.run(
            [path, "-version"], check=True, capture_output=True, text=True, timeout=30
        )
        tools[name] = {"path": path, "version": version.stdout.strip()}
    return tools


def safe_token(value):
    if not isinstance(value, str) or not re.fullmatch(
        r"[A-Za-z0-9][A-Za-z0-9_.-]*", value
    ):
        raise ValueError("invalid reference path component")
    return value


def load_profile():
    profile = json.loads(PROFILE_PATH.read_text())
    if profile["schema_version"] != 1:
        raise ValueError("unsupported reference profile version")
    safe_token(profile["profile"])
    safe_token(profile["release"])
    names = set()
    for source in profile["files"]:
        name = safe_token(source["name"])
        if name in names or not re.fullmatch("[0-9a-f]{32}", source["md5"]):
            raise ValueError("duplicate source name or invalid checksum")
        names.add(name)
        if "database" in source:
            safe_token(source["database"])
    return profile


def installed_path(root, profile):
    return root / profile["profile"] / profile["release"]


def file_record(path, base):
    return {
        "path": path.relative_to(base).as_posix(),
        "bytes": path.stat().st_size,
        "sha256": digest(path),
    }


def verify(directory):
    manifest = json.loads((directory / "manifest.json").read_text())
    if (
        manifest["schema_version"] != 1
        or manifest["status"] != "ready"
        or not manifest["files"]
    ):
        raise ValueError("not a completed reference installation")
    root = directory.resolve()
    for item in manifest["files"]:
        relative = Path(item["path"])
        if relative.is_absolute() or ".." in relative.parts:
            raise ValueError("unsafe manifest path")
        path = directory / relative
        if not path.resolve().is_relative_to(root) or path.is_symlink():
            raise ValueError("unsafe manifest link")
        if path.stat().st_size != item["bytes"] or digest(path) != item["sha256"]:
            raise ValueError(f"reference integrity failure: {relative}")
    return manifest


def install(root, profile):
    if root.resolve().is_relative_to(PROFILE_PATH.parents[1]):
        raise ValueError(
            "reference data must be installed outside the source repository"
        )
    destination = installed_path(root, profile)
    if destination.exists():
        raise FileExistsError(
            f"already exists: {destination}; use verify. Installations are never overwritten."
        )
    tools = check_tools()  # Before root creation or network traffic.
    ancestor = root
    while not ancestor.exists():
        ancestor = ancestor.parent
    free = shutil.disk_usage(ancestor).free
    if free < MIN_FREE_BYTES:
        raise RuntimeError(
            f"need at least 20 GiB free for staging; available {free / 1024**3:.1f} GiB"
        )
    root.mkdir(parents=True, exist_ok=True)
    lock = root / ".install-lock"
    lock.mkdir()  # Exclusive across installers. A stale lock requires manual inspection.
    stage = None
    try:
        staging = root / ".staging"
        staging.mkdir(exist_ok=True)
        stage = Path(tempfile.mkdtemp(prefix=profile["release"] + "-", dir=staging))
        (stage / "source").mkdir()
        (stage / "blast").mkdir()
        (stage / "logs").mkdir()
        builds = []
        for source in profile["files"]:
            path = stage / "source" / source["name"]
            print(f'Downloading {source["name"]}', flush=True)
            download(
                profile["base_url"] + source["name"], path, source["max_download_bytes"]
            )
            if digest(path, "md5") != source["md5"]:
                raise ValueError(
                    f'NCBI checksum differs from pinned release: {source["name"]}; refusing silent update'
                )
            if "database" not in source:
                continue
            database = source["database"]
            unpacked = stage / f"{database}.fna"
            with gzip.open(path, "rb") as compressed, unpacked.open("xb") as output:
                count = copy_limited(
                    compressed, output, source["max_uncompressed_bytes"]
                )
            uncompressed_sha256 = digest(unpacked)
            args = [
                "-in",
                unpacked.name,
                "-dbtype",
                "nucl",
                "-parse_seqids",
                "-blastdb_version",
                "5",
                "-taxid",
                str(profile["taxid"]),
                "-out",
                f"blast/{database}",
                "-title",
                f'{profile["release"]} {database}',
            ]
            print(f"Building {database} BLAST database", flush=True)
            with (stage / "logs" / f"{database}.build.log").open("w") as log:
                subprocess.run(
                    [tools["makeblastdb"]["path"], *args],
                    cwd=stage,
                    stdout=log,
                    stderr=subprocess.STDOUT,
                    check=True,
                )
            info = subprocess.run(
                [tools["blastdbcmd"]["path"], "-db", f"blast/{database}", "-info"],
                cwd=stage,
                check=True,
                capture_output=True,
                text=True,
            )
            (stage / "logs" / f"{database}.info.txt").write_text(info.stdout)
            files = list((stage / "blast").glob(database + ".*"))
            if not files:
                raise RuntimeError(
                    f"makeblastdb produced no index files for {database}"
                )
            builds.append(
                {
                    "name": database,
                    "prefix": f"blast/{database}",
                    "arguments": args,
                    "uncompressed_bytes": count,
                    "uncompressed_sha256": uncompressed_sha256,
                    "database_info": info.stdout.strip(),
                }
            )
            unpacked.unlink()  # Only this installer's temporary decompressed copy.
        manifest = {
            "schema_version": 1,
            "status": "ready",
            "created_utc": datetime.now(timezone.utc).isoformat(),
            "profile": profile,
            "tools": tools,
            "databases": builds,
            "files": [
                file_record(p, stage) for p in sorted(stage.rglob("*")) if p.is_file()
            ],
        }
        (stage / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n")
        verify(stage)
        destination.parent.mkdir(exist_ok=True)
        if destination.exists():
            raise FileExistsError(f"refusing to replace {destination}")
        stage.rename(
            destination
        )  # Same filesystem; only complete verified installs become visible.
        return destination
    except BaseException:
        if stage is not None:
            print(
                f"Incomplete installation retained for inspection: {stage}",
                file=sys.stderr,
            )
        raise
    finally:
        lock.rmdir()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("command", choices=["plan", "install", "verify", "list"])
    parser.add_argument("--root", type=Path, default=default_root())
    args = parser.parse_args()
    root = args.root.expanduser().resolve()
    profile = load_profile()
    if args.command == "plan":
        print(
            json.dumps(
                {
                    "profile": profile,
                    "destination": str(installed_path(root, profile)),
                    "minimum_free_bytes": MIN_FREE_BYTES,
                    "required_tools": ["makeblastdb", "blastdbcmd", "blastn"],
                },
                indent=2,
            )
        )
    elif args.command == "install":
        print(f"Ready: {install(root,profile)}")
    elif args.command == "verify":
        directory = installed_path(root, profile)
        result = verify(directory)
        print(
            json.dumps(
                {
                    "verified": str(directory),
                    "release": result["profile"]["release"],
                    "databases": result["databases"],
                },
                indent=2,
            )
        )
    else:
        for path in sorted(root.glob("*/*/manifest.json")):
            manifest = json.loads(path.read_text())
            print(
                json.dumps(
                    {
                        "path": str(path.parent),
                        "release": manifest["profile"]["release"],
                        "status": manifest["status"],
                        "integrity_checked": False,
                    }
                )
            )


if __name__ == "__main__":
    try:
        main()
    except (OSError, ValueError, RuntimeError, subprocess.SubprocessError) as error:
        print(f"Reference setup failed: {error}", file=sys.stderr)
        sys.exit(1)
