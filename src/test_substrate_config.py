import os
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

from substrate_config import (
    SubstrateConfigError,
    load_postgres_env,
    resolve_substrate_dir,
    windows_path_to_wsl,
)


class SubstrateConfigTests(unittest.TestCase):
    def setUp(self):
        # SOLARISAEL_SUBSTRATE is set on any machine with a live House, and
        # resolve_substrate_dir honours it over the sibling default. Without
        # this the default-path test passes or fails by ambient environment.
        environment = patch.dict(os.environ, {}, clear=False)
        environment.start()
        os.environ.pop("SOLARISAEL_SUBSTRATE", None)
        self.addCleanup(environment.stop)
        self.temp_dir = tempfile.TemporaryDirectory()
        self.root = Path(self.temp_dir.name)
        self.room_dir = self.root / "room"
        self.room_dir.mkdir()
        self.default_dir = self.root / "house" / "substrate"
        self.default_dir.mkdir(parents=True)

    def tearDown(self):
        self.temp_dir.cleanup()

    def test_default_resolves_sibling_substrate(self):
        self.assertEqual(resolve_substrate_dir(self.room_dir), self.default_dir.resolve())

    def test_blank_environment_override_uses_sibling_default(self):
        with patch.dict(os.environ, {"SOLARISAEL_SUBSTRATE": "  "}):
            self.assertEqual(resolve_substrate_dir(self.room_dir), self.default_dir.resolve())

    def test_environment_override_wins_over_default(self):
        override = self.root / "isolated-substrate"
        override.mkdir()
        with patch.dict(os.environ, {"SOLARISAEL_SUBSTRATE": str(override)}):
            self.assertEqual(resolve_substrate_dir(self.room_dir), override.resolve())

    def test_windows_drive_path_converts_for_posix_runtime(self):
        with patch("substrate_config.os.name", "posix"), patch("substrate_config.shutil.which", return_value=None):
            self.assertEqual(
                windows_path_to_wsl(r"C:\Example\Obsidian\substrate"),
                "/mnt/c/Example/Obsidian/substrate",
            )

    def test_invalid_override_fails_closed(self):
        missing = self.root / "does-not-exist"
        with patch.dict(os.environ, {"SOLARISAEL_SUBSTRATE": str(missing)}):
            with self.assertRaises(SubstrateConfigError):
                resolve_substrate_dir(self.room_dir)

    def test_relative_environment_override_is_rejected(self):
        with patch.dict(os.environ, {"SOLARISAEL_SUBSTRATE": "relative/substrate"}):
            with self.assertRaisesRegex(SubstrateConfigError, "absolute path"):
                resolve_substrate_dir(self.room_dir)

    def test_postgres_environment_process_values_overlay_dotenv(self):
        (self.default_dir / ".env").write_text(
            "PGHOST=file-host\nPGPORT=5432\nPGDATABASE=house\nOTHER=value\n",
            encoding="utf-8",
        )
        with patch.dict(os.environ, {"PGHOST": "process-host", "PGUSER": "db-user"}, clear=False):
            env = load_postgres_env(self.default_dir)
        self.assertEqual(env["PGHOST"], "process-host")
        self.assertEqual(env["PGPORT"], "5432")
        self.assertEqual(env["PGUSER"], "db-user")
        self.assertEqual(env["OTHER"], "value")


if __name__ == "__main__":
    unittest.main()
