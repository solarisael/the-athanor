from __future__ import annotations

import run_migrations


def test_load_dotenv_removes_matching_quotes(tmp_path, monkeypatch):
    env_file = tmp_path / ".env"
    env_file.write_text(
        'PGHOST="127.0.0.1"\nPGUSER=\'solarisael\'\nPGPORT=5432\n',
        encoding="utf-8",
    )
    for key in ("PGHOST", "PGUSER", "PGPORT"):
        monkeypatch.delenv(key, raising=False)

    run_migrations.load_dotenv(env_file)

    assert run_migrations.os.environ["PGHOST"] == "127.0.0.1"
    assert run_migrations.os.environ["PGUSER"] == "solarisael"
    assert run_migrations.os.environ["PGPORT"] == "5432"


def test_load_dotenv_preserves_existing_environment(tmp_path, monkeypatch):
    env_file = tmp_path / ".env"
    env_file.write_text("PGHOST=from-file\n", encoding="utf-8")
    monkeypatch.setenv("PGHOST", "from-process")

    run_migrations.load_dotenv(env_file)

    assert run_migrations.os.environ["PGHOST"] == "from-process"
