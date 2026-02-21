"""Integration tests for HappyRepo Python bindings."""

import os
import tempfile
from pathlib import Path

import pytest

# The native extension must be built via `maturin develop --features python`
from happy_faster_code import HappyRepo


@pytest.fixture(scope="module")
def repo():
    """Build and index a small synthetic repo fixture."""
    with tempfile.TemporaryDirectory() as tmpdir:
        root = Path(tmpdir)
        (root / "helpers.py").write_text(
            "def helper(query):\n"
            "    return query.strip().lower()\n"
        )
        (root / "base.py").write_text(
            "class BaseRetriever:\n"
            "    pass\n"
        )
        (root / "hybrid.py").write_text(
            "from base import BaseRetriever\n"
            "from helpers import helper\n\n"
            "class HybridRetriever(BaseRetriever):\n"
            "    def search(self, query):\n"
            "        return helper(query)\n"
        )
        (root / "main.py").write_text(
            "from hybrid import HybridRetriever\n\n"
            "def run(query):\n"
            "    retriever = HybridRetriever()\n"
            "    return retriever.search(query)\n"
        )
        yield HappyRepo(tmpdir)


class TestIndexing:
    def test_stats_has_expected_keys(self, repo):
        s = repo.stats()
        assert "nodes" in s
        assert "edges" in s
        assert "files" in s
        assert "elements" in s
        assert "bm25_docs" in s
        assert "has_vectors" in s

    def test_node_count_positive(self, repo):
        s = repo.stats()
        assert s["nodes"] > 0
        assert s["edges"] > 0
        assert s["files"] > 0

    def test_file_tree_not_empty(self, repo):
        tree = repo.file_tree()
        assert len(tree) > 0
        assert all(isinstance(f, str) for f in tree)

    def test_path_property(self, repo):
        assert os.path.isdir(repo.path)


class TestGraphQueries:
    def test_find_callers_returns_list(self, repo):
        result = repo.find_callers("search")
        assert isinstance(result, list)

    def test_find_callees_returns_list(self, repo):
        result = repo.find_callees("search")
        assert isinstance(result, list)

    def test_get_dependencies_returns_list(self, repo):
        tree = repo.file_tree()
        if tree:
            result = repo.get_dependencies(tree[0])
            assert isinstance(result, list)

    def test_get_dependents_returns_list(self, repo):
        tree = repo.file_tree()
        if tree:
            result = repo.get_dependents(tree[0])
            assert isinstance(result, list)

    def test_get_subclasses(self, repo):
        result = repo.get_subclasses("BaseRetriever")
        assert isinstance(result, list)

    def test_get_superclasses(self, repo):
        result = repo.get_superclasses("HybridRetriever")
        assert isinstance(result, list)

    def test_get_related(self, repo):
        result = repo.get_related("search", 2)
        assert isinstance(result, list)

    def test_find_path(self, repo):
        result = repo.find_path("nonexistent_a", "nonexistent_b")
        assert result is None or isinstance(result, list)


class TestSearch:
    def test_bm25_search(self, repo):
        results = repo.search("retriever", 5)
        assert isinstance(results, list)
        assert len(results) > 0
        for eid, score in results:
            assert isinstance(eid, str)
            assert isinstance(score, float)
            assert score > 0

    def test_bm25_search_no_results(self, repo):
        results = repo.search("xyznonexistent123", 5)
        assert isinstance(results, list)


class TestSource:
    def test_get_source_existing(self, repo):
        results = repo.search("class", 1)
        if results:
            eid = results[0][0]
            source = repo.get_source(eid)
            assert source is not None
            assert len(source) > 0

    def test_get_source_nonexistent(self, repo):
        result = repo.get_source("nonexistent_element_id_12345")
        assert result is None


class TestVectorSearch:
    def test_add_and_search_vectors(self, repo):
        results = repo.search("search", 3)
        if len(results) >= 2:
            ids = [r[0] for r in results]
            vectors = [[1.0, 0.0, 0.0] for _ in ids]
            repo.add_embeddings(ids, vectors)
            vresults = repo.search_by_vector([1.0, 0.0, 0.0], 2)
            assert len(vresults) > 0
            for eid, score in vresults:
                assert isinstance(eid, str)
                assert isinstance(score, float)

    def test_search_by_vector_without_embeddings(self):
        """A fresh repo has no vectors, so search should return empty."""
        test_dir = os.path.dirname(__file__)
        fresh = HappyRepo(test_dir)
        results = fresh.search_by_vector([1.0, 0.0], 5)
        assert results == []


class TestResolvers:
    def test_resolve_module(self, repo):
        result = repo.resolve_module("os")
        assert result is None or isinstance(result, str)

    def test_resolve_symbol(self, repo):
        result = repo.resolve_symbol("search")
        assert isinstance(result, list)
