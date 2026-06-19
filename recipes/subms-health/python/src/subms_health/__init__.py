"""subms-health - a production health endpoint library, zero runtime deps beyond
the subms-events dispatcher it builds on."""

from subms_events import DispatchMode, Event, EventBridge, EventLevel, EventListener, listener

from .clock import Clock, FixedClock, SystemClock
from .component import ComponentHealth, HealthIndicator, fn_indicator
from .endpoint import health_endpoint
from .env_section import EnvProvider, EnvSection, MapEnv, RedactionPolicy, SystemEnv, fnv1a
from .registry import (
    HEALTH_STATUS_TOPIC,
    HealthConfig,
    HealthRegistry,
    ProbeKind,
    RefreshMode,
    RefreshPolicy,
    ServerIndicator,
)
from .status import HealthStatus, http_status_for

# Sugar: a status-change listener is just an event listener.
on_status_change = listener

__all__ = [
    "HealthStatus",
    "http_status_for",
    "ComponentHealth",
    "HealthIndicator",
    "fn_indicator",
    "EnvProvider",
    "SystemEnv",
    "MapEnv",
    "EnvSection",
    "RedactionPolicy",
    "fnv1a",
    "Clock",
    "SystemClock",
    "FixedClock",
    "ProbeKind",
    "RefreshMode",
    "RefreshPolicy",
    "HealthConfig",
    "HealthRegistry",
    "ServerIndicator",
    "HEALTH_STATUS_TOPIC",
    "health_endpoint",
    "Event",
    "EventLevel",
    "EventBridge",
    "EventListener",
    "DispatchMode",
    "listener",
    "on_status_change",
]
