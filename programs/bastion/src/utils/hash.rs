use anchor_lang::prelude::*;

pub const EMPTY_POLICIES_HASH: [u8; 32] = [0u8; 32];

/// Order-independent hash of a set of policy PDA keys.
///
/// Behaviour:
///   - empty input → `EMPTY_POLICIES_HASH` (all zeros) so a fresh Session and
///     a Session with all policies detached share the same sentinel.
///   - non-empty input → `sha256(concat(sort(keys).each().as_ref()))`
///
/// Invariant: the value MUST match `session.policies_hash` on every `execute`.
pub fn compute_policies_hash(keys: &[Pubkey]) -> [u8; 32] {
    if keys.is_empty() {
        return EMPTY_POLICIES_HASH;
    }
    let mut sorted: Vec<Pubkey> = keys.to_vec();
    sorted.sort();

    let mut refs: Vec<&[u8]> = Vec::with_capacity(sorted.len());
    for k in &sorted {
        refs.push(k.as_ref());
    }
    solana_sha256_hasher::hashv(&refs).to_bytes()
}

/// Tamper-evident audit chain: links each `execute` to the previous via
/// `sha256(prev_hash || wrapped_ixs_bytes || nonce_le)`.
pub fn compute_chain_hash(prev: &[u8; 32], batch_bytes: &[u8], nonce: u64) -> [u8; 32] {
    solana_sha256_hasher::hashv(&[prev, batch_bytes, &nonce.to_le_bytes()]).to_bytes()
}

#[cfg(test)]
mod tests {
    use crate::utils::general::pk;

    use super::*;

    #[test]
    fn chain_hash_links_and_changes() {
        let genesis = EMPTY_POLICIES_HASH;
        let h1 = compute_chain_hash(&genesis, b"leg-a", 1);
        let h2 = compute_chain_hash(&h1, b"leg-b", 2);
        // each link depends on the previous + payload + nonce
        assert_ne!(h1, genesis);
        assert_ne!(h2, h1);
        // deterministic
        assert_eq!(h1, compute_chain_hash(&genesis, b"leg-a", 1));
        // a different nonce or payload breaks the link
        assert_ne!(h1, compute_chain_hash(&genesis, b"leg-a", 2));
        assert_ne!(h1, compute_chain_hash(&genesis, b"leg-x", 1));
    }

    #[test]
    fn empty_returns_sentinel() {
        assert_eq!(compute_policies_hash(&[]), EMPTY_POLICIES_HASH);
    }

    #[test]
    fn single_key_is_sha256_of_key_bytes() {
        let k = pk(7);
        let expected = solana_sha256_hasher::hashv(&[k.as_ref()]).to_bytes();
        assert_eq!(compute_policies_hash(&[k]), expected);
    }

    #[test]
    fn order_independent() {
        let a = pk(1);
        let b = pk(2);
        let c = pk(3);
        let h1 = compute_policies_hash(&[a, b, c]);
        let h2 = compute_policies_hash(&[c, b, a]);
        let h3 = compute_policies_hash(&[b, a, c]);
        assert_eq!(h1, h2);
        assert_eq!(h1, h3);
    }

    #[test]
    fn differs_when_key_set_differs() {
        let a = pk(1);
        let b = pk(2);
        let c = pk(3);
        assert_ne!(
            compute_policies_hash(&[a, b]),
            compute_policies_hash(&[a, c])
        );
    }

    #[test]
    fn duplicates_change_hash() {
        let a = pk(1);
        assert_ne!(compute_policies_hash(&[a]), compute_policies_hash(&[a, a]));
    }

    #[test]
    fn known_vector_two_keys() {
        let a = pk(0x11);
        let b = pk(0x22);
        let expected = solana_sha256_hasher::hashv(&[a.as_ref(), b.as_ref()]).to_bytes();
        assert_eq!(compute_policies_hash(&[a, b]), expected);
        assert_eq!(compute_policies_hash(&[b, a]), expected);
    }
}
