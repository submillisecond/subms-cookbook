"""Clock seam: wall-clock millis (for staleness) + an RFC3339 stamp (for the
report's refreshed_at). Tests freeze it to pin cross-language fixtures."""

from __future__ import annotations

import time


class Clock:
    def now_ms(self) -> int:  # pragma: no cover - abstract
        raise NotImplementedError

    def now_rfc3339(self) -> str:  # pragma: no cover - abstract
        raise NotImplementedError


class SystemClock(Clock):
    def now_ms(self) -> int:
        return int(time.time() * 1000)

    def now_rfc3339(self) -> str:
        return time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())


class FixedClock(Clock):
    def __init__(self, ms: int, rfc3339: str) -> None:
        self._ms = ms
        self._stamp = rfc3339

    def set(self, ms: int) -> None:
        self._ms = ms

    def now_ms(self) -> int:
        return self._ms

    def now_rfc3339(self) -> str:
        return self._stamp
