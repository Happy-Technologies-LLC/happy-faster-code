"""Tests for the RLM orchestration layer (config, tools, orchestrator)."""

import sys
import tempfile
import types
from pathlib import Path
from unittest.mock import MagicMock


class TestConfig:
    def test_load_config_defaults(self, monkeypatch):
        """With no TOML and no env vars, should return Anthropic defaults."""
        monkeypatch.delenv("HAPPY_PROVIDER", raising=False)
        monkeypatch.delenv("HAPPY_MODEL", raising=False)
        monkeypatch.delenv("HAPPY_WORKER_MODEL", raising=False)
        monkeypatch.delenv("HAPPY_MODE", raising=False)
        monkeypatch.delenv("ANTHROPIC_API_KEY", raising=False)
        monkeypatch.delenv("OPENAI_API_KEY", raising=False)

        from happy_faster_code.config import load_config

        config = load_config("/nonexistent/path")
        assert config["provider"] == "anthropic"
        assert config["model"] == "claude-sonnet-4-6"
        assert config["api_key"] == ""
        assert config["api_base"] is None
        assert config["worker_model"] is None
        assert config["mode"] == "all-in-one"
        assert "litellm_model" in config

    def test_load_config_env_overrides(self, monkeypatch):
        """Env vars should override defaults."""
        monkeypatch.setenv("HAPPY_PROVIDER", "openai")
        monkeypatch.setenv("HAPPY_MODEL", "gpt-4o")
        monkeypatch.setenv("HAPPY_WORKER_MODEL", "gpt-4o-mini")
        monkeypatch.setenv("HAPPY_MODE", "mcp")
        monkeypatch.setenv("OPENAI_API_KEY", "sk-test-key")
        monkeypatch.delenv("ANTHROPIC_API_KEY", raising=False)

        from happy_faster_code.config import load_config

        config = load_config("/nonexistent/path")
        assert config["provider"] == "openai"
        assert config["model"] == "gpt-4o"
        assert config["worker_model"] == "gpt-4o-mini"
        assert config["mode"] == "mcp"
        assert config["api_key"] == "sk-test-key"

    def test_load_config_from_toml(self, monkeypatch):
        """Should parse .happy/agent.toml correctly."""
        monkeypatch.delenv("HAPPY_PROVIDER", raising=False)
        monkeypatch.delenv("HAPPY_MODEL", raising=False)
        monkeypatch.delenv("HAPPY_WORKER_MODEL", raising=False)
        monkeypatch.delenv("HAPPY_MODE", raising=False)
        monkeypatch.delenv("ANTHROPIC_API_KEY", raising=False)
        monkeypatch.delenv("OPENAI_API_KEY", raising=False)

        with tempfile.TemporaryDirectory() as tmpdir:
            happy_dir = Path(tmpdir) / ".happy"
            happy_dir.mkdir()
            toml_path = happy_dir / "agent.toml"
            toml_path.write_text(
                'provider = "openai"\n'
                'model = "gpt-4o"\n'
                'api_key = "sk-from-toml"\n'
                'worker_model = "gpt-4o-mini"\n'
                'mode = "skills"\n'
            )

            from happy_faster_code.config import load_config

            config = load_config(tmpdir)
            assert config["provider"] == "openai"
            assert config["model"] == "gpt-4o"
            assert config["api_key"] == "sk-from-toml"
            assert config["worker_model"] == "gpt-4o-mini"
            assert config["mode"] == "skills"

    def test_invalid_mode_falls_back(self, monkeypatch):
        monkeypatch.setenv("HAPPY_MODE", "unknown-mode")

        from happy_faster_code.config import load_config

        config = load_config("/nonexistent/path")
        assert config["mode"] == "all-in-one"

    def test_litellm_model_anthropic(self, monkeypatch):
        """Anthropic provider should produce 'anthropic/model' litellm string."""
        monkeypatch.delenv("HAPPY_PROVIDER", raising=False)
        monkeypatch.delenv("HAPPY_MODEL", raising=False)
        monkeypatch.delenv("ANTHROPIC_API_KEY", raising=False)
        monkeypatch.delenv("OPENAI_API_KEY", raising=False)

        from happy_faster_code.config import load_config

        config = load_config("/nonexistent/path")
        assert config["litellm_model"] == "anthropic/claude-sonnet-4-6"

    def test_litellm_model_openai(self, monkeypatch):
        """OpenAI provider should pass model directly."""
        monkeypatch.setenv("HAPPY_PROVIDER", "openai")
        monkeypatch.setenv("HAPPY_MODEL", "gpt-4o")
        monkeypatch.delenv("ANTHROPIC_API_KEY", raising=False)
        monkeypatch.delenv("OPENAI_API_KEY", raising=False)

        from happy_faster_code.config import load_config

        config = load_config("/nonexistent/path")
        assert config["litellm_model"] == "gpt-4o"


class TestRlmTools:
    def test_build_rlm_namespace(self):
        """Namespace should contain repo, read_file, and list_files."""
        mock_repo = MagicMock()

        from happy_faster_code.rlm_tools import build_rlm_namespace

        ns = build_rlm_namespace(mock_repo, "/tmp")
        assert "repo" in ns
        assert ns["repo"] is mock_repo
        assert callable(ns["read_file"])
        assert callable(ns["list_files"])

    def test_read_file_in_namespace(self):
        """read_file should read actual files."""
        mock_repo = MagicMock()

        from happy_faster_code.rlm_tools import build_rlm_namespace

        with tempfile.TemporaryDirectory() as tmpdir:
            test_file = Path(tmpdir) / "hello.txt"
            test_file.write_text("hello world")

            ns = build_rlm_namespace(mock_repo, tmpdir)
            content = ns["read_file"]("hello.txt")
            assert content == "hello world"

    def test_read_file_nonexistent(self):
        """read_file should return error string for missing files."""
        mock_repo = MagicMock()

        from happy_faster_code.rlm_tools import build_rlm_namespace

        ns = build_rlm_namespace(mock_repo, "/tmp")
        result = ns["read_file"]("does_not_exist_xyz123.txt")
        assert "Error" in result

    def test_list_files_in_namespace(self):
        """list_files should find files matching glob pattern."""
        mock_repo = MagicMock()

        from happy_faster_code.rlm_tools import build_rlm_namespace

        with tempfile.TemporaryDirectory() as tmpdir:
            (Path(tmpdir) / "a.py").write_text("x")
            (Path(tmpdir) / "b.py").write_text("y")
            (Path(tmpdir) / "c.txt").write_text("z")

            ns = build_rlm_namespace(mock_repo, tmpdir)
            py_files = ns["list_files"]("*.py")
            assert len(py_files) == 2
            assert all(f.endswith(".py") for f in py_files)

    def test_build_system_prompt(self):
        """System prompt should document HappyRepo methods."""
        mock_repo = MagicMock()
        mock_repo.stats.return_value = {"nodes": 100, "files": 10}

        from happy_faster_code.rlm_tools import build_system_prompt

        prompt = build_system_prompt(mock_repo)
        assert "find_callers" in prompt
        assert "find_callees" in prompt
        assert "get_source" in prompt
        assert "search" in prompt
        assert "read_file" in prompt
        assert "list_files" in prompt
        assert "delegate" in prompt
        assert "rlm_query" in prompt
        assert "100" in prompt  # node count
        assert "10" in prompt  # file count


class TestWorker:
    def test_build_delegate_returns_callable(self):
        """build_delegate should return a callable function."""
        mock_repo = MagicMock()

        from happy_faster_code.worker import build_delegate

        delegate = build_delegate(mock_repo, "/tmp", "gpt-4o-mini")
        assert callable(delegate)


class TestOrchestratorImport:
    def test_rlm_run_importable(self):
        """rlm_run should be importable from the package."""
        from happy_faster_code import rlm_run

        assert callable(rlm_run)

    def test_load_config_importable(self):
        """load_config should be importable from the package."""
        from happy_faster_code import load_config

        assert callable(load_config)


class TestOrchestratorRun:
    def test_run_prefers_graph_rpc_when_configured(self, monkeypatch):
        """run() should use GraphRpcRepo when endpoint+token are provided."""
        from happy_faster_code import orchestrator

        repo_instance = MagicMock()
        graph_rpc_cls = MagicMock(return_value=repo_instance)
        happy_repo_cls = MagicMock()

        class FakeResult:
            response = "ok"

        class FakeRLM:
            def __init__(self, backend, backend_kwargs):
                self.backend = backend
                self.backend_kwargs = backend_kwargs

            def completion(self, **kwargs):
                return FakeResult()

        monkeypatch.setitem(sys.modules, "rlm", types.SimpleNamespace(RLM=FakeRLM))
        monkeypatch.setattr("happy_faster_code.HappyRepo", happy_repo_cls)
        monkeypatch.setattr("happy_faster_code.graph_rpc.GraphRpcRepo", graph_rpc_cls)
        monkeypatch.setattr(
            "happy_faster_code.config.load_config",
            lambda _path: {
                "litellm_model": "anthropic/claude-sonnet-4-6",
                "api_key": "",
                "provider": "anthropic",
                "worker_model": None,
            },
        )
        monkeypatch.setattr(
            "happy_faster_code.rlm_tools.build_rlm_namespace",
            lambda repo, _path: {"repo": repo},
        )
        monkeypatch.setattr(
            "happy_faster_code.rlm_tools.build_system_prompt",
            lambda _repo: "system",
        )
        monkeypatch.setattr(
            "happy_faster_code.worker.build_delegate",
            lambda _repo, _path, _model: (lambda prompt: prompt),
        )

        result = orchestrator.run(
            path="/repo",
            query="analyze",
            graph_rpc_endpoint="127.0.0.1:1234",
            graph_rpc_token="token",
            elements_file="/tmp/elements.bin",
            verbose=False,
        )

        assert result == "ok"
        graph_rpc_cls.assert_called_once_with(
            "127.0.0.1:1234",
            "token",
            path="/repo",
        )
        happy_repo_cls.from_elements_file.assert_not_called()
        happy_repo_cls.assert_not_called()

    def test_run_falls_back_to_snapshot_when_rpc_unavailable(self, monkeypatch):
        """run() should fallback to elements snapshot if graph RPC init fails."""
        from happy_faster_code import orchestrator

        repo_instance = MagicMock()
        graph_rpc_cls = MagicMock(side_effect=RuntimeError("rpc down"))
        happy_repo_cls = MagicMock()
        happy_repo_cls.from_elements_file.return_value = repo_instance

        class FakeResult:
            response = "ok"

        class FakeRLM:
            def __init__(self, backend, backend_kwargs):
                self.backend = backend
                self.backend_kwargs = backend_kwargs

            def completion(self, **kwargs):
                return FakeResult()

        monkeypatch.setitem(sys.modules, "rlm", types.SimpleNamespace(RLM=FakeRLM))
        monkeypatch.setattr("happy_faster_code.HappyRepo", happy_repo_cls)
        monkeypatch.setattr("happy_faster_code.graph_rpc.GraphRpcRepo", graph_rpc_cls)
        monkeypatch.setattr(
            "happy_faster_code.config.load_config",
            lambda _path: {
                "litellm_model": "anthropic/claude-sonnet-4-6",
                "api_key": "",
                "provider": "anthropic",
                "worker_model": None,
            },
        )
        monkeypatch.setattr(
            "happy_faster_code.rlm_tools.build_rlm_namespace",
            lambda repo, _path: {"repo": repo},
        )
        monkeypatch.setattr(
            "happy_faster_code.rlm_tools.build_system_prompt",
            lambda _repo: "system",
        )
        monkeypatch.setattr(
            "happy_faster_code.worker.build_delegate",
            lambda _repo, _path, _model: (lambda prompt: prompt),
        )

        result = orchestrator.run(
            path="/repo",
            query="analyze",
            graph_rpc_endpoint="127.0.0.1:1234",
            graph_rpc_token="token",
            elements_file="/tmp/elements.bin",
            verbose=False,
        )

        assert result == "ok"
        graph_rpc_cls.assert_called_once_with(
            "127.0.0.1:1234",
            "token",
            path="/repo",
        )
        happy_repo_cls.from_elements_file.assert_called_once_with(
            "/tmp/elements.bin",
            "/repo",
        )

    def test_run_uses_elements_snapshot_when_provided(self, monkeypatch):
        """run() should build HappyRepo from snapshot when elements_file is passed."""
        from happy_faster_code import orchestrator

        repo_instance = MagicMock()
        repo_cls = MagicMock()
        repo_cls.from_elements_file.return_value = repo_instance

        class FakeResult:
            response = "ok"

        class FakeRLM:
            def __init__(self, backend, backend_kwargs):
                self.backend = backend
                self.backend_kwargs = backend_kwargs

            def completion(self, **kwargs):
                return FakeResult()

        monkeypatch.setitem(sys.modules, "rlm", types.SimpleNamespace(RLM=FakeRLM))
        monkeypatch.setattr("happy_faster_code.HappyRepo", repo_cls)
        monkeypatch.setattr(
            "happy_faster_code.config.load_config",
            lambda _path: {
                "litellm_model": "anthropic/claude-sonnet-4-6",
                "api_key": "",
                "provider": "anthropic",
                "worker_model": None,
            },
        )
        monkeypatch.setattr(
            "happy_faster_code.rlm_tools.build_rlm_namespace",
            lambda repo, _path: {"repo": repo},
        )
        monkeypatch.setattr(
            "happy_faster_code.rlm_tools.build_system_prompt",
            lambda _repo: "system",
        )
        monkeypatch.setattr(
            "happy_faster_code.worker.build_delegate",
            lambda _repo, _path, _model: (lambda prompt: prompt),
        )

        result = orchestrator.run(
            path="/repo",
            query="analyze",
            elements_file="/tmp/elements.bin",
            verbose=False,
        )

        assert result == "ok"
        repo_cls.from_elements_file.assert_called_once_with("/tmp/elements.bin", "/repo")
        repo_cls.assert_not_called()

    def test_run_uses_path_indexing_without_snapshot(self, monkeypatch):
        """run() should build HappyRepo from path when elements_file is missing."""
        from happy_faster_code import orchestrator

        repo_instance = MagicMock()
        repo_cls = MagicMock(return_value=repo_instance)

        class FakeResult:
            response = "ok"

        class FakeRLM:
            def __init__(self, backend, backend_kwargs):
                self.backend = backend
                self.backend_kwargs = backend_kwargs

            def completion(self, **kwargs):
                return FakeResult()

        monkeypatch.setitem(sys.modules, "rlm", types.SimpleNamespace(RLM=FakeRLM))
        monkeypatch.setattr("happy_faster_code.HappyRepo", repo_cls)
        monkeypatch.setattr(
            "happy_faster_code.config.load_config",
            lambda _path: {
                "litellm_model": "anthropic/claude-sonnet-4-6",
                "api_key": "",
                "provider": "anthropic",
                "worker_model": None,
            },
        )
        monkeypatch.setattr(
            "happy_faster_code.rlm_tools.build_rlm_namespace",
            lambda repo, _path: {"repo": repo},
        )
        monkeypatch.setattr(
            "happy_faster_code.rlm_tools.build_system_prompt",
            lambda _repo: "system",
        )
        monkeypatch.setattr(
            "happy_faster_code.worker.build_delegate",
            lambda _repo, _path, _model: (lambda prompt: prompt),
        )

        result = orchestrator.run(path="/repo", query="analyze", verbose=False)

        assert result == "ok"
        repo_cls.assert_called_once_with("/repo")
