"""Optional Volt/LCM memory retrieval integration for RLM orchestration."""

from __future__ import annotations

import json
from typing import Callable
from urllib import error as urlerror
from urllib import request as urlrequest


class VoltMemoryClient:
    """Thin HTTP client for Volt memory search endpoints."""

    def __init__(
        self,
        api_base: str,
        *,
        api_key: str | None = None,
        search_path: str = "/api/memory/search",
        timeout_seconds: float = 8.0,
    ) -> None:
        self.api_base = api_base.rstrip("/")
        self.api_key = api_key
        self.search_path = search_path if search_path.startswith("/") else f"/{search_path}"
        self.timeout_seconds = timeout_seconds

    def search(
        self,
        query: str,
        *,
        conversation_id: str | None = None,
        top_k: int = 8,
    ) -> list[dict]:
        payload = {"query": query, "top_k": top_k}
        if conversation_id:
            payload["conversation_id"] = conversation_id

        body = json.dumps(payload).encode("utf-8")
        url = f"{self.api_base}{self.search_path}"
        headers = {"Content-Type": "application/json"}
        if self.api_key:
            headers["Authorization"] = f"Bearer {self.api_key}"

        req = urlrequest.Request(url, data=body, method="POST", headers=headers)
        with urlrequest.urlopen(req, timeout=self.timeout_seconds) as response:
            text = response.read().decode("utf-8", errors="replace")
        return _normalize_search_response(text)


def _normalize_search_response(raw_text: str) -> list[dict]:
    try:
        data = json.loads(raw_text)
    except json.JSONDecodeError:
        return [{"content": raw_text}]

    if isinstance(data, list):
        return [_coerce_memory_item(item) for item in data]

    if isinstance(data, dict):
        for key in ("items", "memories", "results", "data"):
            value = data.get(key)
            if isinstance(value, list):
                return [_coerce_memory_item(item) for item in value]
    return [_coerce_memory_item(data)]


def _coerce_memory_item(item) -> dict:
    if isinstance(item, dict):
        return {
            "content": str(
                item.get("content")
                or item.get("text")
                or item.get("summary")
                or item.get("value")
                or ""
            ),
            "score": item.get("score"),
            "id": item.get("id"),
            "metadata": item.get("metadata"),
        }
    return {"content": str(item)}


def _format_memories(memories: list[dict], *, max_items: int) -> str:
    if not memories:
        return "No relevant memory entries found."

    lines: list[str] = []
    for idx, memory in enumerate(memories[:max_items], start=1):
        content = (memory.get("content") or "").strip()
        if not content:
            continue
        score = memory.get("score")
        if isinstance(score, (int, float)):
            lines.append(f"{idx}. {content} (score={score:.3f})")
        else:
            lines.append(f"{idx}. {content}")

    if not lines:
        return "No relevant memory entries found."
    return "\n".join(lines)


def build_volt_memory_hooks(
    config: dict,
    initial_query: str,
) -> tuple[str | None, Callable[[str], str] | None]:
    """Create optional memory context + tool for RLM namespace."""
    if not config.get("volt_enabled"):
        return None, None

    api_base = config.get("volt_api_base")
    if not api_base:
        return None, None

    client = VoltMemoryClient(
        api_base=api_base,
        api_key=config.get("volt_api_key"),
        search_path=config.get("volt_search_path") or "/api/memory/search",
    )
    conversation_id = config.get("volt_conversation_id")
    top_k = max(1, int(config.get("volt_top_k") or 8))

    def recall_memory(query: str) -> str:
        try:
            items = client.search(
                query,
                conversation_id=conversation_id,
                top_k=top_k,
            )
            return _format_memories(items, max_items=top_k)
        except (urlerror.URLError, TimeoutError, OSError, ValueError):
            return "Volt memory lookup unavailable."

    bootstrap_query = config.get("volt_bootstrap_query") or initial_query
    context = recall_memory(bootstrap_query)
    if context == "Volt memory lookup unavailable.":
        context = None

    return context, recall_memory
