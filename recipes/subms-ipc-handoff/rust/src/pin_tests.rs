use super::*;

#[test]
fn guard_without_cpu_is_a_no_op() {
    let g = PinGuard::pin(None);
    assert!(!g.pinned());
}

#[test]
fn peer_cpus_are_distinct_when_present() {
    if let Some((a, b)) = peer_cpus() {
        assert_ne!(a, b);
    }
}

#[cfg(all(
    target_os = "linux",
    any(target_arch = "x86_64", target_arch = "aarch64")
))]
#[test]
fn pin_restores_the_previous_mask() {
    let before = imp::allowed().unwrap();
    {
        let g = PinGuard::pin(Some(before[0]));
        assert!(g.pinned());
        assert_eq!(imp::allowed().unwrap(), vec![before[0]]);
    }
    assert_eq!(imp::allowed().unwrap(), before);
    assert!(!PinGuard::pin(Some(usize::MAX)).pinned());
}
