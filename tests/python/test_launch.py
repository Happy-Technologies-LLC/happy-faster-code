"""Tests for mode-aware launcher wiring."""

from __future__ import annotations

import json


def test_build_launch_command_all_in_one():
    from happy_faster_code.launch import build_launch_command

    cmd, env = build_launch_command("all-in-one", ["--help"])
    assert cmd == ["happycode", "--help"]
    assert env == {}


def test_build_launch_command_mcp():
    from happy_faster_code.launch import build_launch_command

    cmd, env = build_launch_command("mcp", [])
    assert cmd == ["happycode", "mcp-server"]
    assert env == {}


def test_build_launch_command_skills():
    from happy_faster_code.launch import build_launch_command

    cmd, env = build_launch_command("skills", ["exec", "--help"])
    assert cmd == ["happycode", "exec", "--help"]
    assert env == {"HAPPY_INTEGRATION_MODE": "skills"}


def test_main_uses_mode_from_config(monkeypatch, capsys):
    from happy_faster_code import launch

    def fake_load_config(_):
        return {"mode": "mcp"}

    monkeypatch.setattr(launch, "load_config", fake_load_config)

    code = launch.main(["--print-only"])
    assert code == 0
    payload = json.loads(capsys.readouterr().out)
    assert payload["mode"] == "mcp"
    assert payload["command"] == ["happycode", "mcp-server"]


def test_main_explicit_mode_overrides_config(monkeypatch, capsys):
    from happy_faster_code import launch

    def fake_load_config(_):
        return {"mode": "mcp"}

    monkeypatch.setattr(launch, "load_config", fake_load_config)

    code = launch.main(["--mode", "skills", "--print-only", "--", "exec"])
    assert code == 0
    payload = json.loads(capsys.readouterr().out)
    assert payload["mode"] == "skills"
    assert payload["command"] == ["happycode", "exec"]
