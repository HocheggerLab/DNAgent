"""Offline installer lifecycle tests. BLAST is mocked, not biological validation."""

import copy
import gzip
import hashlib
import json
from pathlib import Path
import subprocess
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import patch

import manage_references as refs


class ReferencesTest(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name) / "references"
        self.profile = copy.deepcopy(refs.load_profile())
        self.payloads = {}
        for source in self.profile["files"]:
            data = (
                gzip.compress(b">synthetic\nACGTACGT\n")
                if "database" in source
                else b"synthetic metadata\n"
            )
            self.payloads[source["name"]] = data
            source["md5"] = hashlib.md5(data).hexdigest()
        self.tools = {
            name: {"path": name, "version": "mock 1"}
            for name in ("makeblastdb", "blastdbcmd", "blastn")
        }
        self.addCleanup(patch.stopall)
        patch.object(refs, "check_tools", return_value=self.tools).start()
        patch.object(
            refs.shutil, "disk_usage", return_value=SimpleNamespace(free=30 * 1024**3)
        ).start()
        patch.object(refs, "download", side_effect=self.download).start()
        self.runner = patch.object(
            refs.subprocess, "run", side_effect=self.run_tool
        ).start()

    def download(self, url, path, maximum):
        path.write_bytes(self.payloads[path.name])

    def run_tool(self, args, **kwargs):
        if args[0] == "makeblastdb":
            prefix = Path(kwargs["cwd"]) / args[args.index("-out") + 1]
            prefix.with_suffix(".nin").write_bytes(b"mocked index")
        return subprocess.CompletedProcess(
            args, 0, stdout="mock database info", stderr=""
        )

    def test_success_verification_and_no_overwrite(self):
        destination = refs.install(self.root, self.profile)
        manifest = refs.verify(destination)
        self.assertEqual(len(manifest["databases"]), 2)
        self.assertTrue(
            (destination / "source" / self.profile["files"][0]["name"]).is_file()
        )
        self.assertFalse((destination / "genome.fna").exists())
        self.assertFalse((self.root / ".install-lock").exists())
        with self.assertRaises(FileExistsError):
            refs.install(self.root, self.profile)
        (destination / "blast/genome.nin").write_bytes(b"corrupt")
        with self.assertRaises(ValueError):
            refs.verify(destination)

    def test_checksum_mismatch_is_never_published(self):
        self.profile["files"][0]["md5"] = "0" * 32
        with self.assertRaises(ValueError):
            refs.install(self.root, self.profile)
        self.assertFalse(refs.installed_path(self.root, self.profile).exists())
        self.assertTrue(list((self.root / ".staging").iterdir()))
        self.assertFalse((self.root / ".install-lock").exists())

    def test_failed_build_is_never_published(self):
        self.runner.side_effect = subprocess.CalledProcessError(1, ["makeblastdb"])
        with self.assertRaises(subprocess.CalledProcessError):
            refs.install(self.root, self.profile)
        self.assertFalse(refs.installed_path(self.root, self.profile).exists())

    def test_missing_tools_or_space_prevents_network_and_store_creation(self):
        with patch.object(
            refs, "check_tools", side_effect=RuntimeError("missing tool")
        ):
            with self.assertRaises(RuntimeError):
                refs.install(self.root, self.profile)
        self.assertFalse(self.root.exists())
        with patch.object(
            refs.shutil, "disk_usage", return_value=SimpleNamespace(free=1)
        ):
            with self.assertRaises(RuntimeError):
                refs.install(self.root, self.profile)
        self.assertFalse(self.root.exists())
        refs.download.assert_not_called()

    def test_lock_prevents_concurrent_install(self):
        self.root.mkdir()
        (self.root / ".install-lock").mkdir()
        with self.assertRaises(FileExistsError):
            refs.install(self.root, self.profile)
        self.assertTrue((self.root / ".install-lock").exists())
        refs.download.assert_not_called()

    def test_manifest_traversal_rejected(self):
        destination = refs.install(self.root, self.profile)
        path = destination / "manifest.json"
        data = json.loads(path.read_text())
        data["files"][0]["path"] = "../../outside"
        path.write_text(json.dumps(data))
        with self.assertRaises(ValueError):
            refs.verify(destination)

    def test_stream_limit(self):
        import io

        with self.assertRaises(ValueError):
            refs.copy_limited(io.BytesIO(b"12345"), io.BytesIO(), 4)


if __name__ == "__main__":
    unittest.main()
