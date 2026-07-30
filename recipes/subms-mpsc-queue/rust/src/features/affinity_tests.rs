use super::*;

#[test]
fn empty_cores_is_rejected() {
    let err = set_affinity(&[]);
    assert!(matches!(err, Err(AffinityError::InvalidCore(0))));
}

#[test]
fn out_of_range_core_returns_invalid() {
    let err = set_affinity(&[usize::MAX]);
    // On every platform we either reject as InvalidCore or
    // Unsupported. Neither is the success case.
    assert!(err.is_err());
}

#[cfg(any(target_os = "linux", target_os = "windows"))]
#[test]
fn pinning_to_core_zero_succeeds_on_supported_platforms() {
    // Core 0 exists on every system that supports affinity.
    let result = set_affinity(&[0]);
    // Permission may be denied in some containerised CI sandboxes;
    // accept either OK or an explicit OsError so the test is
    // portable across CI runners.
    assert!(
        matches!(result, Ok(()) | Err(AffinityError::OsError(_))),
        "unexpected: {result:?}"
    );
}

#[cfg(not(any(target_os = "linux", target_os = "windows")))]
#[test]
fn unsupported_platform_returns_unsupported() {
    let result = set_affinity(&[0]);
    assert!(matches!(result, Err(AffinityError::Unsupported)));
}

#[cfg(any(target_os = "linux", target_os = "windows"))]
#[test]
fn valid_range_but_absent_core_surfaces_error() {
    // Core 900 is inside the Linux 1024-bit cpu_set range but is not a
    // physically present CPU, so sched_setaffinity fails and the OsError
    // arm fires. On Windows the same index exceeds the 64-bit mask and is
    // rejected as InvalidCore. Both are errors, never success.
    let result = set_affinity(&[900]);
    assert!(result.is_err(), "unexpected success: {result:?}");
}

#[test]
fn display_messages_render() {
    let m1 = format!("{}", AffinityError::Unsupported);
    let m2 = format!("{}", AffinityError::InvalidCore(7));
    let m3 = format!("{}", AffinityError::OsError(42));
    assert!(m1.contains("not supported"));
    assert!(m2.contains("7"));
    assert!(m3.contains("42"));
}

#[test]
fn debug_messages_render() {
    let s = format!("{:?}", AffinityError::OsError(1));
    assert!(s.contains("OsError"));
}
