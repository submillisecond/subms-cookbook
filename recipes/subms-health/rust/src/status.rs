//! Health status taxonomy + worst-wins aggregation: a parent's status is the
//! worst of its parts.

/// Component status. Wire tokens are UPPERCASE. Severity order:
/// Up < Unknown < Warn < Degraded < Down.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HealthStatus {
    Up,
    Unknown,
    /// Advisory: still serving (disk filling, cert expiring). Maps to HTTP 200.
    /// Also where a non-critical indicator's failure is demoted to.
    Warn,
    /// Impaired enough to fail readiness. Maps to HTTP 503.
    Degraded,
    Down,
}

impl HealthStatus {
    /// The UPPERCASE wire token.
    pub const fn as_str(self) -> &'static str {
        match self {
            HealthStatus::Up => "UP",
            HealthStatus::Unknown => "UNKNOWN",
            HealthStatus::Warn => "WARN",
            HealthStatus::Degraded => "DEGRADED",
            HealthStatus::Down => "DOWN",
        }
    }

    /// Severity rank. Higher is worse: Up < Unknown < Warn < Degraded < Down.
    const fn rank(self) -> u8 {
        match self {
            HealthStatus::Up => 0,
            HealthStatus::Unknown => 1,
            HealthStatus::Warn => 2,
            HealthStatus::Degraded => 3,
            HealthStatus::Down => 4,
        }
    }

    /// Return the worse of two statuses.
    pub fn worse(self, other: HealthStatus) -> HealthStatus {
        if other.rank() > self.rank() {
            other
        } else {
            self
        }
    }

    /// Worst-wins fold. An empty iterator is Up.
    pub fn aggregate<I: IntoIterator<Item = HealthStatus>>(it: I) -> HealthStatus {
        let mut acc = HealthStatus::Up;
        for s in it {
            acc = acc.worse(s);
        }
        acc
    }
}

/// UP/UNKNOWN/WARN -> 200, DEGRADED/DOWN -> 503. Bare `u16`, no http dependency.
pub fn http_status_for(s: HealthStatus) -> u16 {
    match s {
        HealthStatus::Up | HealthStatus::Unknown | HealthStatus::Warn => 200,
        HealthStatus::Degraded | HealthStatus::Down => 503,
    }
}
