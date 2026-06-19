"""Env/deploy provider: select env vars by explicit key + prefix/glob, redact
secrets, group them into a named ComponentHealth. Mirrors the Rust port exactly
(including the cross-language JSON fixture)."""

from __future__ import annotations

import os
from enum import Enum
from typing import Dict, List, Optional, Tuple

from .component import ComponentHealth, HealthIndicator
from .status import HealthStatus


class EnvProvider:
    def get(self, key: str) -> Optional[str]:  # pragma: no cover - abstract
        raise NotImplementedError

    def keys(self) -> List[str]:  # pragma: no cover - abstract
        raise NotImplementedError


class SystemEnv(EnvProvider):
    def get(self, key: str) -> Optional[str]:
        return os.environ.get(key)

    def keys(self) -> List[str]:
        return list(os.environ.keys())


class MapEnv(EnvProvider):
    def __init__(self) -> None:
        self._vars: Dict[str, str] = {}

    def with_(self, key: str, value: str) -> "MapEnv":
        self._vars[key] = value
        return self

    def get(self, key: str) -> Optional[str]:
        return self._vars.get(key)

    def keys(self) -> List[str]:
        return list(self._vars.keys())


def fnv1a(data: bytes) -> int:
    h = 0xCBF29CE484222325
    for b in data:
        h ^= b
        h = (h * 0x100000001B3) & 0xFFFFFFFFFFFFFFFF
    return h


class RedactionPolicy(Enum):
    MASK = "MASK"
    LAST4 = "LAST4"
    HASH = "HASH"
    FINGERPRINT = "FINGERPRINT"

    def apply(self, value: str) -> str:
        if self == RedactionPolicy.MASK:
            return "***"
        if self == RedactionPolicy.LAST4:
            return "***" + value[-4:] if len(value) > 4 else "***"
        if self == RedactionPolicy.HASH:
            return "fnv1a:%016x" % fnv1a(value.encode("utf-8"))
        return "fp_%06x" % (fnv1a(value.encode("utf-8")) & 0xFFFFFF)


_SECRET_NEEDLES = ["SECRET", "TOKEN", "KEY", "PASSWORD", "PASS", "CREDENTIAL"]


def _glob_match(pattern: str, s: str) -> bool:
    star = pattern.find("*")
    if star < 0:
        return pattern == s
    pre, post = pattern[:star], pattern[star + 1 :]
    return len(s) >= len(pre) + len(post) and s.startswith(pre) and s.endswith(post)


class EnvSection:
    def __init__(self, name: str) -> None:
        self._name = name
        self._explicit: List[str] = []
        self._prefixes: List[str] = []
        self._globs: List[str] = []
        self._redactions: List[Tuple[str, RedactionPolicy]] = []
        self._redact_substrings: List[Tuple[str, RedactionPolicy]] = []
        self._remap: Dict[str, str] = {}
        self._strip_prefix_in_key = False
        self._lowercase_keys = False
        self._status = HealthStatus.UP
        self._include_empty = False

    @property
    def name(self) -> str:
        return self._name

    def key(self, k: str) -> "EnvSection":
        self._explicit.append(k)
        return self

    def keys(self, ks) -> "EnvSection":
        self._explicit.extend(ks)
        return self

    def prefix(self, p: str) -> "EnvSection":
        self._prefixes.append(p)
        return self

    def glob(self, pattern: str) -> "EnvSection":
        self._globs.append(pattern)
        return self

    def redact(self, key: str, policy: RedactionPolicy) -> "EnvSection":
        self._redactions.append((key, policy))
        return self

    def redact_substring(self, needle: str, policy: RedactionPolicy) -> "EnvSection":
        self._redact_substrings.append((needle, policy))
        return self

    def redact_secrets(self) -> "EnvSection":
        for n in _SECRET_NEEDLES:
            self._redact_substrings.append((n, RedactionPolicy.MASK))
        return self

    def remap(self, frm: str, to: str) -> "EnvSection":
        self._remap[frm] = to
        return self

    def strip_prefix_in_key(self, yes: bool = True) -> "EnvSection":
        self._strip_prefix_in_key = yes
        return self

    def lowercase_keys(self, yes: bool = True) -> "EnvSection":
        self._lowercase_keys = yes
        return self

    def status(self, status: HealthStatus) -> "EnvSection":
        self._status = status
        return self

    def include_empty(self, yes: bool = True) -> "EnvSection":
        self._include_empty = yes
        return self

    def _matches(self, key: str) -> bool:
        return (
            key in self._explicit
            or any(key.startswith(p) for p in self._prefixes)
            or any(_glob_match(g, key) for g in self._globs)
        )

    def _detail_key(self, raw: str) -> str:
        if raw in self._remap:
            return self._remap[raw]
        dk = raw
        if self._strip_prefix_in_key:
            best = ""
            for p in self._prefixes:
                if raw.startswith(p) and len(p) > len(best):
                    best = p
            if best:
                dk = raw[len(best) :]
        if self._lowercase_keys:
            dk = dk.lower()
        return dk

    def _policy_for(self, raw: str, detail: str) -> Optional[RedactionPolicy]:
        for k, p in self._redactions:
            if k == raw or k == detail:
                return p
        rl, dl = raw.lower(), detail.lower()
        for s, p in self._redact_substrings:
            sl = s.lower()
            if sl in rl or sl in dl:
                return p
        return None

    def render(self, env: EnvProvider) -> ComponentHealth:
        candidates = set(self._explicit)
        for k in env.keys():
            if self._matches(k):
                candidates.add(k)
        details: Dict[str, object] = {}
        for raw in sorted(candidates):
            val = env.get(raw)
            if val is None:
                continue
            if val == "" and not self._include_empty:
                continue
            dk = self._detail_key(raw)
            policy = self._policy_for(raw, dk)
            details[dk] = policy.apply(val) if policy else val
        return ComponentHealth(self._status, details)

    def into_indicator(self, env: Optional[EnvProvider] = None) -> HealthIndicator:
        return _EnvSectionIndicator(self, env or SystemEnv())


class _EnvSectionIndicator(HealthIndicator):
    def __init__(self, section: EnvSection, env: EnvProvider) -> None:
        self._section = section
        self._env = env

    def name(self) -> str:
        return self._section.name

    def check(self) -> ComponentHealth:
        return self._section.render(self._env)
