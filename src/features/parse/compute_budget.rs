use solana_sdk::pubkey::Pubkey;
use yellowstone_grpc_proto::prelude::CompiledInstruction;

/// Solana Compute Budget program ID
const COMPUTE_BUDGET_PROGRAM_ID: Pubkey =
    solana_sdk::pubkey!("ComputeBudget111111111111111111111111111111");

/// Represents the Compute Budget settings extracted from a transaction.
#[derive(Debug, Clone, Copy, Default)]
pub struct ComputeBudgetInfo {
    /// The compute unit limit (from SetComputeUnitLimit instruction, discriminator 0x02).
    pub unit_limit: u32,
    /// The compute unit price in micro-lamports (from SetComputeUnitPrice instruction, discriminator 0x03).
    pub unit_price: u64,
}

/// Extract Compute Budget (SetComputeUnitLimit + SetComputeUnitPrice) from a transaction's
/// top-level instructions. Returns `ComputeBudgetInfo` with default 0 values for any
/// instruction not found.
pub fn extract_compute_budget(
    ixs: &[CompiledInstruction],
    account_keys: &[Pubkey],
) -> ComputeBudgetInfo {
    let mut info = ComputeBudgetInfo::default();

    // Find the Compute Budget program index in account keys
    let program_index = match account_keys
        .iter()
        .position(|key| *key == COMPUTE_BUDGET_PROGRAM_ID)
    {
        Some(idx) => idx as u32,
        None => return info, // No Compute Budget instructions in this tx
    };

    for ix in ixs.iter() {
        if ix.program_id_index != program_index {
            continue;
        }

        if ix.data.is_empty() {
            continue;
        }

        match ix.data[0] {
            // SetComputeUnitLimit: discriminator 0x02, followed by u32 (4 bytes LE)
            0x02 if ix.data.len() >= 5 => {
                let bytes: [u8; 4] = ix.data[1..5].try_into().unwrap_or([0; 4]);
                info.unit_limit = u32::from_le_bytes(bytes);
            }
            // SetComputeUnitPrice: discriminator 0x03, followed by u64 (8 bytes LE)
            0x03 if ix.data.len() >= 9 => {
                let bytes: [u8; 8] = ix.data[1..9].try_into().unwrap_or([0; 8]);
                info.unit_price = u64::from_le_bytes(bytes);
            }
            _ => {}
        }
    }

    info
}
