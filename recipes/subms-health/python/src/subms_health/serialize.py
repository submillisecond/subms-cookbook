"""Hand-rolled deterministic JSON. The byte layout is the cross-language
contract - it matches the Rust + Java ports exactly (a fixture pins it).

`json.dumps` is used only to escape individual scalars (its escaping matches our
Rust serializer: named escapes + lowercase ``\\u00xx``); the object/key ordering
is built by hand because the report's field order is fixed, not alphabetical.
"""

from __future__ import annotations

import json
from typing import Any, Dict

from .component import ComponentHealth


def fnv1a(data: bytes) -> int:
    h = 0xCBF29CE484222325
    for b in data:
        h ^= b
        h = (h * 0x100000001B3) & 0xFFFFFFFFFFFFFFFF
    return h


def enc_str(s: str) -> str:
    return json.dumps(s, ensure_ascii=False)


def enc_value(v: Any) -> str:
    if isinstance(v, bool):
        return "true" if v else "false"
    if isinstance(v, int):
        return str(v)
    if isinstance(v, float):
        return json.dumps(v)
    return enc_str(v if isinstance(v, str) else str(v))


def _push_map(d: Dict[str, Any]) -> str:
    inner = ",".join(f"{enc_str(k)}:{enc_value(d[k])}" for k in sorted(d))
    return "{" + inner + "}"


def _push_component(c: ComponentHealth) -> str:
    out = '{"status":' + enc_str(c.status.value)
    if c.details:
        out += ',"details":' + _push_map(c.details)
    if c.components:
        out += ',"components":' + _push_component_map(c.components)
    return out + "}"


def _push_component_map(m: Dict[str, ComponentHealth]) -> str:
    inner = ",".join(f"{enc_str(k)}:{_push_component(m[k])}" for k in sorted(m))
    return "{" + inner + "}"


def component_to_json(c: ComponentHealth) -> str:
    return _push_component(c)


def report_to_json(status_token: str, refreshed_at: str, entries) -> str:
    """Serialize the registry report. `entries` is a list of
    (name, status_token, age_ms, stale, ComponentHealth) sorted by name. Field
    order is fixed (status, refreshed_at, components) and matches the Rust port."""
    out = '{"status":' + enc_str(status_token) + ',"refreshed_at":' + enc_str(refreshed_at)
    if entries:
        parts = []
        for name, st, age_ms, stale, comp in entries:
            seg = enc_str(name) + ':{"status":' + enc_str(st)
            seg += ',"age_ms":' + str(age_ms)
            seg += ',"stale":' + ("true" if stale else "false")
            if comp.details:
                seg += ',"details":' + _push_map(comp.details)
            if comp.components:
                seg += ',"components":' + _push_component_map(comp.components)
            seg += "}"
            parts.append(seg)
        out += ',"components":{' + ",".join(parts) + "}"
    return out + "}"
