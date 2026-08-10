"""Focused proof for health.py state resolution across the Windows/WSL boundary.

The seam these defend: health.py runs inside WSL, launched by a Windows adapter.
`ATHANOR_STATE_DIR` is a Windows process variable and does not cross, so the
dotenv arrives as an argv value. That argument must therefore work on its own,
and nothing on this path may raise — a traceback reaches the adapter as
malformed JSON rather than as a verdict.
"""
import json
import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parent))

import health  # noqa: E402
import state_paths  # noqa: E402

SUBSTRATE_DIR = Path(__file__).resolve().parent


class ResolveStateTests(unittest.TestCase):
    def setUp(self):
        environment = patch.dict(os.environ, {}, clear=False)
        environment.start()
        self.addCleanup(environment.stop)
        for key in ("ATHANOR_STATE_DIR", "SOLARISAEL_STATE_DIR", "SOLARISAEL_BACKUP_DIR"):
            os.environ.pop(key, None)
        self.temp_dir = tempfile.TemporaryDirectory()
        self.root = Path(self.temp_dir.name)
        self.addCleanup(self.temp_dir.cleanup)

    def install_dotenv(self) -> Path:
        """The canonical shape the installer produces: <state>/substrate/.env."""
        dotenv = self.root / "install" / "state" / "substrate" / ".env"
        dotenv.parent.mkdir(parents=True)
        dotenv.write_text("PGHOST=explicit-host\n", encoding="utf-8")
        return dotenv

    def test_explicit_canonical_env_file_names_its_own_state_root(self):
        # The whole point: no ATHANOR_STATE_DIR in the environment, and the
        # state root is still known, because the dotenv's location says it.
        dotenv = self.install_dotenv()
        resolved, state_root, source, error = health.resolve_state(str(dotenv))
        self.assertEqual(resolved, dotenv)
        self.assertEqual(state_root, dotenv.parent.parent)
        self.assertEqual(source, "explicit_env_file")
        self.assertIsNone(error)
        self.assertNotIn("ATHANOR_STATE_DIR", os.environ)

    def test_explicit_env_file_wins_over_the_structural_answer(self):
        # The alternative: a resolvable structural root exists (this checkout),
        # and the explicit argument must still beat it rather than tie.
        dotenv = self.install_dotenv()
        _, structural_root, _, _ = health.resolve_state(None)
        _, explicit_root, source, _ = health.resolve_state(str(dotenv))
        self.assertIsNotNone(structural_root)
        self.assertNotEqual(explicit_root, structural_root)
        self.assertEqual(source, "explicit_env_file")

    def test_non_canonical_explicit_file_names_only_itself(self):
        # A dotenv outside a `substrate` directory says nothing about the state
        # root, so the root falls back to structural resolution instead of
        # inventing one from an unrelated parent directory.
        stray = self.root / "elsewhere" / "my.env"
        stray.parent.mkdir(parents=True)
        stray.write_text("PGHOST=stray\n", encoding="utf-8")
        resolved, state_root, source, error = health.resolve_state(str(stray))
        self.assertEqual(resolved, stray)
        self.assertNotEqual(state_root, stray.parent.parent)
        self.assertEqual(state_root, state_paths.ATHANOR_ROOT / "state")
        self.assertEqual(source, "development_checkout")
        self.assertIsNone(error)

    def test_no_argument_uses_structural_resolution(self):
        dotenv, state_root, source, error = health.resolve_state(None)
        self.assertEqual(state_root, state_paths.ATHANOR_ROOT / "state")
        self.assertEqual(dotenv, state_root / "substrate" / ".env")
        self.assertEqual(source, "development_checkout")
        self.assertIsNone(error)

    def test_unresolvable_state_never_raises_and_is_reported(self):
        # A traceback here would reach the adapter as malformed JSON. The
        # contract is a value, always.
        def unresolvable():
            raise state_paths.StateRootError("no state root")

        with patch.object(state_paths, "resolve_state_root", unresolvable):
            dotenv, state_root, source, error = health.resolve_state(None)
        self.assertIsNone(dotenv)
        self.assertIsNone(state_root)
        self.assertIsNone(source)
        self.assertEqual(error, "no state root")

    def test_explicit_canonical_file_survives_an_unresolvable_state_root(self):
        # The load-bearing case for a staged install: structural resolution
        # cannot answer, and the explicit argument alone must still work.
        dotenv = self.install_dotenv()

        def unresolvable():
            raise state_paths.StateRootError("no state root")

        with patch.object(state_paths, "resolve_state_root", unresolvable):
            resolved, state_root, source, error = health.resolve_state(str(dotenv))
        self.assertEqual(resolved, dotenv)
        self.assertEqual(state_root, dotenv.parent.parent)
        self.assertEqual(source, "explicit_env_file")
        self.assertIsNone(error)


class BackupDirectoryTests(unittest.TestCase):
    def setUp(self):
        environment = patch.dict(os.environ, {}, clear=False)
        environment.start()
        self.addCleanup(environment.stop)
        os.environ.pop("SOLARISAEL_BACKUP_DIR", None)

    def test_backup_directory_follows_the_resolved_state_root(self):
        root = Path("/install/state") if os.name != "nt" else Path(r"C:\install\state")
        self.assertEqual(health.resolve_backup_directory(root), root / "substrate" / "backups")

    def test_unresolved_state_names_no_backup_directory(self):
        # Previously this path raised out of state_paths. It must now report.
        self.assertIsNone(health.resolve_backup_directory(None))
        probe = health.probe_backup(24.0, None)
        self.assertFalse(probe["ok"])
        self.assertIsNone(probe["directory"])
        self.assertIn("unresolved", probe["error"])

    def test_explicit_backup_override_still_wins_without_a_state_root(self):
        with patch.dict(os.environ, {"SOLARISAEL_BACKUP_DIR": str(Path.cwd())}):
            self.assertEqual(health.resolve_backup_directory(None), Path.cwd())


class HealthProcessTests(unittest.TestCase):
    """Drive the real script as a separate process, the way the adapter does."""

    def setUp(self):
        self.temp_dir = tempfile.TemporaryDirectory()
        self.root = Path(self.temp_dir.name)
        self.addCleanup(self.temp_dir.cleanup)

    def run_health(self, *args: str) -> dict:
        # A deliberately bare environment: no ATHANOR_* variable of any kind,
        # which is exactly the state a WSL child is launched in.
        env = {
            key: value
            for key, value in os.environ.items()
            if not key.startswith("ATHANOR_") and not key.startswith("SOLARISAEL_")
        }
        env["SOLARISAEL_HOUSE_DISABLE_POSTGRES"] = "1"
        completed = subprocess.run(
            [sys.executable, str(SUBSTRATE_DIR / "health.py"), "--skip-embedding", *args],
            capture_output=True,
            text=True,
            env=env,
            cwd=str(self.root),
            timeout=120,
        )
        self.assertTrue(completed.stdout.strip(), f"health.py printed nothing: {completed.stderr}")
        return json.loads(completed.stdout)

    def test_explicit_env_file_is_honored_with_no_athanor_variables_present(self):
        dotenv = self.root / "install" / "state" / "substrate" / ".env"
        dotenv.parent.mkdir(parents=True)
        dotenv.write_text("PGHOST=from-explicit-file\n", encoding="utf-8")

        verdict = self.run_health("--env-file", str(dotenv))
        topology = verdict["topology"]

        self.assertEqual(topology["dotenv"], str(dotenv))
        self.assertTrue(topology["dotenvExists"])
        self.assertEqual(topology["stateRootSource"], "explicit_env_file")
        self.assertEqual(topology["stateRoot"], str(dotenv.parent.parent))
        self.assertTrue(topology["ok"])

    def test_an_unreachable_server_is_reported_as_unreachable_not_as_a_bad_schema(self):
        # The real probe, against a host that cannot exist. Without `reachable`
        # the only signal is an absent schemaVersion, which a verifier renders
        # as "required 13; got undefined" and blames the schema for a server it
        # never contacted.
        dotenv = self.root / "install" / "state" / "substrate" / ".env"
        dotenv.parent.mkdir(parents=True)
        dotenv.write_text(
            "PGHOST=no-such-host.invalid\nPGPORT=5432\nPGUSER=nobody\n"
            "PGPASSWORD=unused\nPGDATABASE=nothing\n",
            encoding="utf-8",
        )

        verdict = self.run_health("--env-file", str(dotenv))
        database = verdict["database"]

        self.assertFalse(database["ok"])
        self.assertFalse(database["reachable"])
        self.assertNotIn("schemaVersion", database)
        self.assertIn("PostgreSQL substrate is unavailable or incomplete", verdict["degradedReasons"])

    def test_explicit_substrate_exe_is_reported_over_the_structural_fallback(self):
        # The seam a real AKASHA archive exposed: ATHANOR_SUBSTRATE_EXE is a
        # Windows variable and does not cross into WSL, so health.py fell back
        # to <product>/target/release/athanor-substrate and reported the
        # installed binary at adapters/omp/bin/<platform>/ as missing.
        installed = self.root / "the-athanor" / "adapters" / "omp" / "bin" / "windows-x64" / "athanor-substrate.exe"
        installed.parent.mkdir(parents=True)
        installed.write_bytes(b"MZ")

        verdict = self.run_health("--substrate-exe", str(installed))
        topology = verdict["topology"]

        self.assertEqual(topology["executable"], str(installed))
        self.assertTrue(topology["executableFound"])
        self.assertEqual(topology["executableSource"], "argument")

    def test_without_the_argument_the_executable_falls_back_structurally(self):
        # The alternative, and the one that produced the false negative. The
        # fallback must be visibly a fallback, not silently indistinguishable
        # from an explicitly named binary.
        verdict = self.run_health()
        topology = verdict["topology"]

        self.assertEqual(topology["executableSource"], "release_build")
        self.assertIn("target", topology["executable"])
        self.assertNotEqual(topology["executableSource"], "argument")

    def test_the_explicit_file_is_read_not_merely_named(self):
        # Naming the dotenv in topology proves resolution; it does not prove
        # the file was loaded. Assert a value from inside it reached the
        # process environment the probes run against.
        dotenv = self.root / "install" / "state" / "substrate" / ".env"
        dotenv.parent.mkdir(parents=True)
        dotenv.write_text("PGHOST=from-explicit-file\nPGDATABASE=explicit_db\n", encoding="utf-8")

        env = {
            key: value
            for key, value in os.environ.items()
            if not key.startswith("ATHANOR_") and not key.startswith("SOLARISAEL_") and not key.startswith("PG")
        }
        probe = (
            "import json, os, sys\n"
            f"sys.path.insert(0, {str(SUBSTRATE_DIR)!r})\n"
            "import health\n"
            "dotenv, root, source, error = health.resolve_state(sys.argv[1])\n"
            "health.load_dotenv(dotenv)\n"
            "print(json.dumps({'pghost': os.environ.get('PGHOST'),"
            " 'pgdatabase': os.environ.get('PGDATABASE'), 'source': source}))\n"
        )
        completed = subprocess.run(
            [sys.executable, "-c", probe, str(dotenv)],
            capture_output=True, text=True, env=env, cwd=str(self.root), timeout=120,
        )
        self.assertTrue(completed.stdout.strip(), completed.stderr)
        observed = json.loads(completed.stdout)
        self.assertEqual(observed["pghost"], "from-explicit-file")
        self.assertEqual(observed["pgdatabase"], "explicit_db")
        self.assertEqual(observed["source"], "explicit_env_file")

    def test_a_verdict_is_still_json_when_the_explicit_file_is_absent(self):
        # An absent dotenv is a degraded verdict, never a traceback.
        missing = self.root / "install" / "state" / "substrate" / ".env"
        missing.parent.mkdir(parents=True)

        verdict = self.run_health("--env-file", str(missing))

        self.assertEqual(verdict["substrateApi"], 1)
        self.assertFalse(verdict["topology"]["dotenvExists"])
        self.assertEqual(verdict["topology"]["stateRootSource"], "explicit_env_file")

    def test_without_the_argument_the_process_resolves_structurally(self):
        verdict = self.run_health()
        self.assertEqual(verdict["topology"]["stateRootSource"], "development_checkout")


if __name__ == "__main__":
    unittest.main()
