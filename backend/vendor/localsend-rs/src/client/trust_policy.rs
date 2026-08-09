/// Explicit policy for trusting self-signed LocalSend peer certificates.
///
/// `PinnedFingerprint` accepts one exact normalized SHA-256 fingerprint.
/// `InsecureForTests` is test-only and must never be selected by production
/// application code.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TlsTrustPolicy {
    PinnedFingerprint(String),
    InsecureForTests,
}

impl TlsTrustPolicy {
    pub fn new<I, S>(trusted: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let fingerprint = trusted
            .into_iter()
            .map(Into::into)
            .find_map(|fingerprint: String| normalize_fingerprint(&fingerprint));

        Self::PinnedFingerprint(fingerprint.unwrap_or_default())
    }

    pub fn insecure_for_tests() -> Self {
        Self::InsecureForTests
    }

    pub fn allows(&self, fingerprint: &str) -> bool {
        match self {
            Self::InsecureForTests => true,
            Self::PinnedFingerprint(expected) => {
                normalize_fingerprint(fingerprint).is_some_and(|actual| actual == *expected)
            }
        }
    }

    /// Returns `true` when the policy permits connecting to peers that present
    /// an invalid (for example self-signed) certificate. Mirrors the historical
    /// `danger_accept_invalid_certs(true)` semantics for `Insecure` policies.
    pub fn allows_insecure(&self) -> bool {
        matches!(self, Self::InsecureForTests)
    }

    pub fn pinned_fingerprint(&self) -> Option<&str> {
        match self {
            Self::PinnedFingerprint(fingerprint) if !fingerprint.is_empty() => Some(fingerprint),
            Self::PinnedFingerprint(_) | Self::InsecureForTests => None,
        }
    }
}

pub(crate) fn normalize_fingerprint(fingerprint: &str) -> Option<String> {
    let normalized: String = fingerprint
        .chars()
        .filter(|character| character.is_ascii_hexdigit())
        .map(|character| character.to_ascii_lowercase())
        .collect();

    (normalized.len() == 64).then_some(normalized)
}

#[cfg(test)]
mod tests {
    use super::TlsTrustPolicy;

    #[test]
    fn trust_policy_accepts_matching_fingerprint() {
        let fingerprint = "a".repeat(64);
        let policy = TlsTrustPolicy::new(vec![fingerprint.clone()]);
        assert!(policy.allows(&fingerprint));
    }

    #[test]
    fn trust_policy_rejects_unknown_fingerprint() {
        let policy = TlsTrustPolicy::new(vec!["a".repeat(64)]);
        assert!(!policy.allows(&"b".repeat(64)));
    }

    #[test]
    fn trust_policy_rejects_empty_fingerprints() {
        let policy = TlsTrustPolicy::new(vec!["a".repeat(64)]);
        assert!(!policy.allows(""));
    }

    #[test]
    fn insecure_test_policy_accepts_anything() {
        let policy = TlsTrustPolicy::insecure_for_tests();
        assert!(policy.allows(""));
        assert!(policy.allows("unknown-fp"));
    }
}
