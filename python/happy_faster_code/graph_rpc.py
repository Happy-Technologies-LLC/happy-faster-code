"""Local graph RPC client used by the RLM orchestrator."""

from __future__ import annotations

import json
import socket
import threading
from typing import Any


class GraphRpcRepo:
    """HappyRepo-compatible proxy that queries the live Rust graph over localhost RPC."""

    def __init__(
        self,
        endpoint: str,
        token: str,
        *,
        path: str | None = None,
        timeout_seconds: float = 30.0,
    ) -> None:
        host, port = endpoint.rsplit(":", 1)
        self._address = (host, int(port))
        self._token = token
        self._timeout_seconds = timeout_seconds
        self._socket: socket.socket | None = None
        self._reader = None
        self._writer = None
        self._lock = threading.Lock()
        self.path = path or "."

    def close(self) -> None:
        with self._lock:
            if self._reader is not None:
                self._reader.close()
                self._reader = None
            if self._writer is not None:
                self._writer.close()
                self._writer = None
            if self._socket is not None:
                self._socket.close()
                self._socket = None

    def __del__(self) -> None:
        try:
            self.close()
        except Exception:
            pass

    def _ensure_connection(self) -> None:
        if self._socket is not None:
            return
        sock = socket.create_connection(self._address, timeout=self._timeout_seconds)
        self._socket = sock
        self._reader = sock.makefile("r", encoding="utf-8")
        self._writer = sock.makefile("w", encoding="utf-8")

    def _request(self, method: str, params: dict[str, Any] | None = None) -> Any:
        payload = {
            "token": self._token,
            "method": method,
            "params": params or {},
        }
        with self._lock:
            self._ensure_connection()
            assert self._writer is not None
            assert self._reader is not None
            self._writer.write(json.dumps(payload))
            self._writer.write("\n")
            self._writer.flush()
            response_line = self._reader.readline()

        if not response_line:
            raise RuntimeError("graph RPC connection closed")
        response = json.loads(response_line)
        if not response.get("ok"):
            raise RuntimeError(response.get("error", "graph RPC error"))
        return response.get("result")

    def find_callers(self, symbol: str) -> list[str]:
        return self._request("find_callers", {"symbol": symbol})

    def find_callees(self, symbol: str) -> list[str]:
        return self._request("find_callees", {"symbol": symbol})

    def get_dependencies(self, file_path: str) -> list[str]:
        return self._request("get_dependencies", {"file_path": file_path})

    def get_dependents(self, file_path: str) -> list[str]:
        return self._request("get_dependents", {"file_path": file_path})

    def get_subclasses(self, class_name: str) -> list[str]:
        return self._request("get_subclasses", {"class_name": class_name})

    def get_superclasses(self, class_name: str) -> list[str]:
        return self._request("get_superclasses", {"class_name": class_name})

    def find_path(self, source: str, target: str) -> list[str] | None:
        return self._request("find_path", {"source": source, "target": target})

    def get_related(self, element: str, max_hops: int) -> list[str]:
        return self._request("get_related", {"element": element, "max_hops": max_hops})

    def search(self, query: str, k: int) -> list[tuple[str, float]]:
        result = self._request("search", {"query": query, "k": k}) or []
        return [(row[0], float(row[1])) for row in result]

    def get_source(self, element_id: str) -> str | None:
        return self._request("get_source", {"element_id": element_id})

    def file_tree(self) -> list[str]:
        return self._request("file_tree")

    def stats(self) -> dict:
        return self._request("stats")

    def resolve_symbol(self, symbol: str) -> list[tuple[str, str]]:
        result = self._request("resolve_symbol", {"symbol": symbol}) or []
        return [(row[0], row[1]) for row in result]

    def resolve_module(self, module_name: str) -> str | None:
        return self._request("resolve_module", {"module_name": module_name})
