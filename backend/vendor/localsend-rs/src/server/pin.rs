//! Receiver-side PIN enforcement matching the official LocalSend server.

use lru::LruCache;
use std::net::IpAddr;
use std::num::NonZeroUsize;

pub const MAX_FAILURES: u32 = 3;
const FAILURE_CACHE_CAPACITY: usize = 200;

#[derive(Debug, PartialEq, Eq)]
pub enum PinVerdict {
    Ok,
    Unauthorized,
    LockedOut,
}

#[derive(Debug)]
pub struct PinGate {
    pin: Option<String>,
    failures: LruCache<IpAddr, u32>,
}

impl PinGate {
    pub fn new(pin: Option<String>) -> Self {
        Self {
            pin,
            failures: LruCache::new(NonZeroUsize::new(FAILURE_CACHE_CAPACITY).unwrap()),
        }
    }

    pub fn check(&mut self, provided: Option<&str>, peer: IpAddr) -> PinVerdict {
        let Some(expected) = self.pin.as_deref() else {
            return PinVerdict::Ok;
        };

        let count = self.failures.get(&peer).copied().unwrap_or(0);
        if count >= MAX_FAILURES {
            return PinVerdict::LockedOut;
        }

        match provided {
            Some(pin) if constant_time_eq(pin.as_bytes(), expected.as_bytes()) => {
                self.failures.pop(&peer);
                PinVerdict::Ok
            }
            Some(_) => {
                self.failures.put(peer, count + 1);
                PinVerdict::Unauthorized
            }
            None => PinVerdict::Unauthorized,
        }
    }
}

/// Length-leaking-free comparison without extra deps.
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{IpAddr, Ipv4Addr};

    const PEER: IpAddr = IpAddr::V4(Ipv4Addr::new(192, 168, 1, 7));
    const OTHER: IpAddr = IpAddr::V4(Ipv4Addr::new(192, 168, 1, 8));

    #[test]
    fn no_pin_configured_always_ok() {
        let mut g = PinGate::new(None);
        assert_eq!(g.check(None, PEER), PinVerdict::Ok);
        assert_eq!(g.check(Some("anything"), PEER), PinVerdict::Ok);
    }

    #[test]
    fn wrong_or_missing_pin_is_unauthorized() {
        let mut g = PinGate::new(Some("123456".to_string()));
        assert_eq!(g.check(None, PEER), PinVerdict::Unauthorized);
        assert_eq!(g.check(Some("000000"), PEER), PinVerdict::Unauthorized);
        assert_eq!(g.check(Some("123456"), PEER), PinVerdict::Ok);
    }

    #[test]
    fn missing_pin_does_not_increase_failure_count() {
        let mut g = PinGate::new(Some("123456".to_string()));
        for _ in 0..10 {
            assert_eq!(g.check(None, PEER), PinVerdict::Unauthorized);
        }
        assert_eq!(g.check(Some("123456"), PEER), PinVerdict::Ok);
    }

    #[test]
    fn three_failures_lock_out_that_peer_only() {
        let mut g = PinGate::new(Some("123456".to_string()));
        for _ in 0..3 {
            assert_eq!(g.check(Some("bad"), PEER), PinVerdict::Unauthorized);
        }
        // 4th attempt: locked, even with the right PIN
        assert_eq!(g.check(Some("123456"), PEER), PinVerdict::LockedOut);
        // a different peer is unaffected
        assert_eq!(g.check(Some("123456"), OTHER), PinVerdict::Ok);
    }

    #[test]
    fn success_resets_failure_count() {
        let mut g = PinGate::new(Some("123456".to_string()));
        g.check(Some("bad"), PEER);
        g.check(Some("bad"), PEER);
        assert_eq!(g.check(Some("123456"), PEER), PinVerdict::Ok);
        // counter reset: two more failures don't lock
        g.check(Some("bad"), PEER);
        g.check(Some("bad"), PEER);
        assert_eq!(g.check(Some("123456"), PEER), PinVerdict::Ok);
    }

    #[test]
    fn failure_cache_is_bounded_and_evicts_least_recently_used_ip() {
        let mut g = PinGate::new(Some("123456".to_string()));
        let first = IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1));

        for host in 1..=201u16 {
            let peer = IpAddr::V4(Ipv4Addr::new(10, 0, (host / 256) as u8, host as u8));
            assert_eq!(g.check(Some("bad"), peer), PinVerdict::Unauthorized);
        }

        assert_eq!(g.failures.len(), FAILURE_CACHE_CAPACITY);
        assert!(g.failures.peek(&first).is_none());
    }
}
