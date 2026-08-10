import os
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

from substrate_config import (
    DEFAULT_SUBSTRATE_DIR,
    PG_ENV_KEYS,
    SubstrateConfigError,
    load_postgres_env,
    resolve_state_dotenv,
    resolve_substrate_dir,
    windows_path_to_wsl,
)


class SubstrateConfigTests(unittest.TestCase):
    def setUp(self):
        # ATHANOR_SUBSTRATE_ROOT is set on any machine with a live House, and
        # resolve_substrate_dir honours it over the structural default. Without
        # this the default-path test passes or fails by ambient environment.
        environment = patch.dict(os.environ, {}, clear=False)
        environment.start()
        os.environ.pop("ATHANOR_SUBSTRATE_ROOT", None)
        os.environ.pop("ATHANOR_STATE_DIR", None)
        for key in PG_ENV_KEYS:
            os.environ.pop(key, None)
        self.addCleanup(environment.stop)
        self.temp_dir = tempfile.TemporaryDirectory()
        self.root = Path(self.temp_dir.name)

    def tearDown(self):
        self.temp_dir.cleanup()

    def test_default_is_the_substrate_inside_the_product_tree(self):
        # The structural answer is <athanor-root>/substrate, derived from this
        # module's own location. It must not depend on a room directory, a
        # sibling checkout, or the process working directory.
        self.assertEqual(resolve_substrate_dir(), DEFAULT_SUBSTRATE_DIR.resolve())
        self.assertEqual(DEFAULT_SUBSTRATE_DIR.name, "substrate")
        self.assertTrue(DEFAULT_SUBSTRATE_DIR.is_absolute())

    def test_default_is_stable_across_working_directories(self):
        # The opposite of the assumption the old sibling default encoded: the
        # answer must not move when the process does.
        first = resolve_substrate_dir()
        previous = Path.cwd()
        try:
            os.chdir(self.root)
            self.assertEqual(resolve_substrate_dir(), first)
        finally:
            os.chdir(previous)

    def test_blank_environment_override_uses_structural_default(self):
        with patch.dict(os.environ, {"ATHANOR_SUBSTRATE_ROOT": "  "}):
            self.assertEqual(resolve_substrate_dir(), DEFAULT_SUBSTRATE_DIR.resolve())

    def test_environment_override_wins_over_default(self):
        override = self.root / "isolated-substrate"
        override.mkdir()
        with patch.dict(os.environ, {"ATHANOR_SUBSTRATE_ROOT": str(override)}):
            resolved = resolve_substrate_dir()
        self.assertEqual(resolved, override.resolve())
        self.assertNotEqual(resolved, DEFAULT_SUBSTRATE_DIR.resolve())

    def test_explicit_argument_wins_over_environment_override(self):
        argument = self.root / "explicit-substrate"
        argument.mkdir()
        environment = self.root / "environment-substrate"
        environment.mkdir()
        with patch.dict(os.environ, {"ATHANOR_SUBSTRATE_ROOT": str(environment)}):
            self.assertEqual(resolve_substrate_dir(argument), argument.resolve())

    def test_windows_drive_path_converts_for_posix_runtime(self):
        with patch("substrate_config.os.name", "posix"), patch("substrate_config.shutil.which", return_value=None):
            self.assertEqual(
                windows_path_to_wsl(r"C:\Example\Obsidian\substrate"),
                "/mnt/c/Example/Obsidian/substrate",
            )

    def test_invalid_override_fails_closed(self):
        missing = self.root / "does-not-exist"
        with patch.dict(os.environ, {"ATHANOR_SUBSTRATE_ROOT": str(missing)}):
            with self.assertRaises(SubstrateConfigError):
                resolve_substrate_dir()

    def test_relative_environment_override_is_rejected(self):
        with patch.dict(os.environ, {"ATHANOR_SUBSTRATE_ROOT": "relative/substrate"}):
            with self.assertRaisesRegex(SubstrateConfigError, "absolute path"):
                resolve_substrate_dir()

    def test_legacy_variable_is_not_accepted_at_runtime(self):
        # The cutover is clean: the old name must be inert, not a quiet alias.
        override = self.root / "legacy-substrate"
        override.mkdir()
        with patch.dict(os.environ, {"SOLARISAEL_SUBSTRATE": str(override)}):
            resolved = resolve_substrate_dir()
        self.assertEqual(resolved, DEFAULT_SUBSTRATE_DIR.resolve())
        self.assertNotEqual(resolved, override.resolve())

    def test_postgres_environment_process_values_overlay_dotenv(self):
        substrate = self.root / "substrate"
        substrate.mkdir()
        (substrate / ".env").write_text(
            "PGHOST=file-host\nPGPORT=5432\nPGDATABASE=house\nOTHER=value\n",
            encoding="utf-8",
        )
        env = load_postgres_env(
            substrate,
            environ={"PGHOST": "process-host", "PGUSER": "db-user"},
        )
        self.assertEqual(env["PGHOST"], "process-host")
        self.assertEqual(env["PGPORT"], "5432")
        self.assertEqual(env["PGUSER"], "db-user")
        self.assertEqual(env["OTHER"], "value")

    def test_state_dotenv_is_authoritative_over_product_compatibility_file(self):
        product = self.root / "the-athanor"
        substrate = product / "substrate"
        substrate.mkdir(parents=True)
        (substrate / ".env").write_text(
            "PGHOST=product-host\nPGDATABASE=product-db\n",
            encoding="utf-8",
        )
        state = self.root / "state-root"
        state_dotenv = state / "substrate" / ".env"
        state_dotenv.parent.mkdir(parents=True)
        state_dotenv.write_text(
            "PGHOST=state-host\nPGDATABASE=solarisael_memory\n",
            encoding="utf-8",
        )

        environment = {"ATHANOR_STATE_DIR": str(state), "PGUSER": "process-user"}
        self.assertEqual(resolve_state_dotenv(substrate, environ=environment), state_dotenv.resolve())
        self.assertEqual(
            load_postgres_env(substrate, environ=environment),
            {
                "PGHOST": "state-host",
                "PGDATABASE": "solarisael_memory",
                "PGUSER": "process-user",
            },
        )

    def test_product_dotenv_remains_a_fallback_when_state_dotenv_is_absent(self):
        product = self.root / "the-athanor"
        substrate = product / "substrate"
        substrate.mkdir(parents=True)
        (substrate / ".env").write_text(
            "PGHOST=product-host\nPGDATABASE=compatibility-db\n",
            encoding="utf-8",
        )

        env = load_postgres_env(
            substrate,
            environ={"ATHANOR_STATE_DIR": str(self.root / "missing-state")},
        )
        self.assertEqual(env["PGHOST"], "product-host")
        self.assertEqual(env["PGDATABASE"], "compatibility-db")


if __name__ == "__main__":
    unittest.main()
