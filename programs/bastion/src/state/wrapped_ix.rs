use anchor_lang::prelude::*;

/// Compact account reference used inside `WrappedInstruction`.
///
/// `index` points into the slice of accounts referenced by the wrapped ix,
/// which lives at `remaining_accounts[policy_count + 1 ..]`.
/// `flags` packs solana `AccountMeta` flags:
///   bit 0 = `is_signer`
///   bit 1 = `is_writable`
/// All other bits MUST be zero.
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, Debug, PartialEq, Eq, InitSpace)]
pub struct CompactAccountMeta {
    pub index: u8,
    pub flags: u8,
}

impl CompactAccountMeta {
    pub const SIGNER_BIT: u8 = 1 << 0;
    pub const WRITABLE_BIT: u8 = 1 << 1;
    pub const KNOWN_BITS: u8 = Self::SIGNER_BIT | Self::WRITABLE_BIT;

    pub fn new(index: u8, is_signer: bool, is_writable: bool) -> Self {
        let mut flags = 0u8;
        if is_signer {
            flags |= Self::SIGNER_BIT;
        }
        if is_writable {
            flags |= Self::WRITABLE_BIT;
        }
        Self { index, flags }
    }

    pub fn is_signer(&self) -> bool {
        self.flags & Self::SIGNER_BIT != 0
    }

    pub fn is_writable(&self) -> bool {
        self.flags & Self::WRITABLE_BIT != 0
    }

    pub fn flags_well_formed(&self) -> bool {
        self.flags & !Self::KNOWN_BITS == 0
    }
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Debug, PartialEq, Eq)]
pub struct WrappedInstruction {
    pub program_id: Pubkey,
    pub accounts: Vec<CompactAccountMeta>,
    pub data: Vec<u8>,
}

#[cfg(test)]
mod tests {
    use crate::utils::general::pk;

    use super::*;

    #[test]
    fn compact_meta_flags_packed_correctly() {
        let none = CompactAccountMeta::new(0, false, false);
        assert!(!none.is_signer() && !none.is_writable());
        assert!(none.flags_well_formed());

        let signer = CompactAccountMeta::new(1, true, false);
        assert!(signer.is_signer() && !signer.is_writable());
        assert_eq!(signer.flags, 0b01);

        let writable = CompactAccountMeta::new(2, false, true);
        assert!(!writable.is_signer() && writable.is_writable());
        assert_eq!(writable.flags, 0b10);

        let both = CompactAccountMeta::new(3, true, true);
        assert!(both.is_signer() && both.is_writable());
        assert_eq!(both.flags, 0b11);
    }

    #[test]
    fn reserved_bits_rejected_by_well_formed() {
        let bad = CompactAccountMeta {
            index: 0,
            flags: 0b1000_0000,
        };
        assert!(!bad.flags_well_formed());
    }

    #[test]
    fn wrapped_ix_roundtrip_empty() {
        let original = WrappedInstruction {
            program_id: pk(7),
            accounts: vec![],
            data: vec![],
        };
        let bytes = borsh::to_vec(&original).unwrap();
        let decoded = WrappedInstruction::try_from_slice(&bytes).unwrap();
        assert_eq!(original, decoded);
    }

    #[test]
    fn wrapped_ix_roundtrip_full() {
        let original = WrappedInstruction {
            program_id: pk(1),
            accounts: vec![
                CompactAccountMeta::new(0, false, true),
                CompactAccountMeta::new(1, true, true),
                CompactAccountMeta::new(2, false, false),
            ],
            data: vec![0xde, 0xad, 0xbe, 0xef, 0x00, 0x01, 0x02],
        };
        let bytes = borsh::to_vec(&original).unwrap();
        let decoded = WrappedInstruction::try_from_slice(&bytes).unwrap();
        assert_eq!(original, decoded);
    }

    #[test]
    fn wrapped_ix_rejects_truncated() {
        let original = WrappedInstruction {
            program_id: pk(1),
            accounts: vec![CompactAccountMeta::new(0, false, true)],
            data: vec![1, 2, 3, 4],
        };
        let bytes = borsh::to_vec(&original).unwrap();
        for trunc in 0..bytes.len() {
            let result = WrappedInstruction::try_from_slice(&bytes[..trunc]);
            assert!(
                result.is_err(),
                "expected error on truncated buffer of len {}",
                trunc
            );
        }
    }

    #[test]
    fn wrapped_ix_rejects_garbage_after_valid() {
        let original = WrappedInstruction {
            program_id: pk(1),
            accounts: vec![],
            data: vec![1, 2, 3],
        };
        let mut bytes = borsh::to_vec(&original).unwrap();
        bytes.extend_from_slice(&[0xff, 0xff]);
        // try_from_slice MUST refuse trailing bytes (strict Borsh contract)
        let result = WrappedInstruction::try_from_slice(&bytes);
        assert!(result.is_err());
    }

    #[test]
    fn wrapped_ix_with_oversize_accounts_vec_length_prefix_rejected() {
        // Claim 1_000_000 CompactAccountMetas in the Borsh length prefix but provide none.
        // try_from_slice should fail (out-of-bounds during read).
        let program_id = pk(1);
        let mut bytes = Vec::new();
        bytes.extend_from_slice(program_id.as_ref());
        bytes.extend_from_slice(&1_000_000u32.to_le_bytes());
        // no actual meta entries + no data prefix
        let result = WrappedInstruction::try_from_slice(&bytes);
        assert!(result.is_err());
    }
}
