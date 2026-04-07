use borsh::BorshDeserialize;
use solana_sdk::{bs58, pubkey::Pubkey};
use yellowstone_grpc_proto::{
    geyser::{SubscribeUpdate, subscribe_update::UpdateOneof},
    prelude::{CompiledInstruction, InnerInstruction, Message},
};

use crate::*;

pub fn extract_transaction_data(
    update: &SubscribeUpdate,
) -> Option<(
    Vec<Pubkey>,
    Vec<CompiledInstruction>,
    Vec<InnerInstruction>,
    String,
    Vec<Pubkey>,
)> {
    let transaction_update = match &update.update_oneof {
        Some(UpdateOneof::Transaction(tx_update)) => tx_update,
        _ => return None,
    };

    let tx_info = transaction_update.transaction.as_ref()?;
    let transaction = tx_info.transaction.as_ref()?;
    let meta = tx_info.meta.as_ref()?;
    let tx_msg = transaction.message.as_ref()?;

    let (_, signers) = get_signers(tx_msg.clone());

    let mut account_keys: Vec<Pubkey> = tx_msg
        .account_keys
        .iter()
        .filter_map(|key_bytes| Pubkey::try_from(key_bytes.as_slice()).ok())
        .collect();

    account_keys.extend(
        meta.loaded_writable_addresses
            .iter()
            .filter_map(|key_bytes| Pubkey::try_from(key_bytes.as_slice()).ok()),
    );

    account_keys.extend(
        meta.loaded_readonly_addresses
            .iter()
            .filter_map(|key_bytes| Pubkey::try_from(key_bytes.as_slice()).ok()),
    );

    let ixs: Vec<CompiledInstruction> = tx_msg.instructions.clone();
    let inner_ixs: Vec<InnerInstruction> = meta
        .inner_instructions
        .iter()
        .flat_map(|ix| ix.instructions.clone())
        .collect();

    let signature = tx_info.signature.clone();
    let tx_id = bs58::encode(signature).into_string();

    Some((account_keys, ixs, inner_ixs, tx_id, signers))
}

pub fn get_signers(tx_msg: Message) -> (usize, Vec<Pubkey>) {
    let signer_count = tx_msg
        .header
        .map(|header| header.num_required_signatures as usize)
        .unwrap_or(0);

    let pubkeys: Vec<Pubkey> = tx_msg
        .account_keys
        .iter()
        .filter_map(|key_bytes| Pubkey::try_from(key_bytes.as_slice()).ok())
        .collect();

    let signer_pubkeys = &pubkeys[..signer_count.min(pubkeys.len())];
    (signer_count, signer_pubkeys.to_vec())
}

pub fn filter_by_program_id(
    ixs: Vec<CompiledInstruction>,
    inner_ixs: Vec<InnerInstruction>,
    account_keys: Vec<Pubkey>,
    program_id: Pubkey,
) -> Result<Vec<InstructionRawData>, Box<dyn std::error::Error>> {
    let program_id_index = match account_keys.iter().position(|&pos| pos == program_id) {
        Some(index) => index,
        None => {
            println!("Program not found");
            return Err("program_id not found".into());
        }
    };

    let filtered_ixs = ixs
        .into_iter()
        .filter(|ix| ix.program_id_index == program_id_index as u32)
        .map(|ix| InstructionRawData {
            accounts: ix.accounts,
            data: ix.data,
            program_id_index: program_id_index as u32,
        });

    let filtered_inner_ixs = inner_ixs
        .into_iter()
        .filter(|ix| ix.program_id_index == program_id_index as u32)
        .map(|ix| InstructionRawData {
            accounts: ix.accounts,
            data: ix.data,
            program_id_index: program_id_index as u32,
        });

    Ok(filtered_ixs.chain(filtered_inner_ixs).collect())
}

pub fn get_trade_info(
    ix_infos: Vec<InstructionRawData>,
    account_keys: Vec<Pubkey>,
) -> (
    Vec<MintEvent>,
    Vec<BuyEvent>,
    Vec<SellEvent>,
    Vec<MintInstructionAccounts>,
    Vec<BuyInstructionAccounts>,
    Vec<SellInstructionAccounts>,
) {
    let mut mint_instruction_accounts: Vec<MintInstructionAccounts> = Vec::new();
    let mut buy_instruction_accounts: Vec<BuyInstructionAccounts> = Vec::new();
    let mut sell_instruction_accounts: Vec<SellInstructionAccounts> = Vec::new();
    let mut mint_events: Vec<MintEvent> = Vec::new();
    let mut _trade_events: Vec<TradeEvent> = Vec::new();
    let mut buy_events: Vec<BuyEvent> = Vec::new();
    let mut sell_events: Vec<SellEvent> = Vec::new();
    ix_infos.iter().for_each(|info| {
        if info.data.starts_with(&PUMP_FUN_MINT_DISCRIMINATOR) {
            let mint_accounts = MintInstructionAccounts {
                mint: account_keys[info.accounts[0] as usize],
                bonding_curve: account_keys[info.accounts[2] as usize],
                associated_bonding_curve: account_keys[info.accounts[3] as usize],
                user: account_keys[info.accounts[7] as usize],
                system_program: account_keys[info.accounts[8] as usize],
                token_program: account_keys[info.accounts[9] as usize],
                associated_token_program: account_keys[info.accounts[10] as usize],
                event_authority: account_keys[info.accounts[12] as usize],
            };
            mint_instruction_accounts.push(mint_accounts);
        } else if info.data.starts_with(&PUMP_FUN_MINT_SPL_DISCRIMINATOR) {
            let mint_accounts = MintInstructionAccounts {
                mint: account_keys[info.accounts[0] as usize],
                bonding_curve: account_keys[info.accounts[2] as usize],
                associated_bonding_curve: account_keys[info.accounts[3] as usize],
                user: account_keys[info.accounts[5] as usize],
                system_program: account_keys[info.accounts[6] as usize],
                token_program: account_keys[info.accounts[7] as usize],
                associated_token_program: account_keys[info.accounts[8] as usize],
                event_authority: account_keys[info.accounts[14] as usize],
            };
            mint_instruction_accounts.push(mint_accounts);
        } else if info.data.starts_with(&PUMP_FUN_BUY_DISCRIMINATOR) || info.data.starts_with(&PUMP_FUN_BUY_EXACT_SOL_IN_DISCRIMINATOR) {
            let buy_accounts = BuyInstructionAccounts {
                global: account_keys[info.accounts[0] as usize],
                fee_recipient: account_keys[info.accounts[1] as usize],
                mint: account_keys[info.accounts[2] as usize],
                bonding_curve: account_keys[info.accounts[3] as usize],
                associated_bonding_curve: account_keys[info.accounts[4] as usize],
                associated_user: account_keys[info.accounts[5] as usize],
                user: account_keys[info.accounts[6] as usize],
                system_program: account_keys[info.accounts[7] as usize],
                token_program: account_keys[info.accounts[8] as usize],
                creator_vault: account_keys[info.accounts[9] as usize],
                event_authority: account_keys[info.accounts[10] as usize],
                program: account_keys[info.accounts[11] as usize],
                global_volume_accumulator: account_keys[info.accounts[12] as usize],
                user_volume_accumulator: account_keys[info.accounts[13] as usize],
                fee_config: account_keys[info.accounts[14] as usize],
                fee_program: account_keys[info.accounts[15] as usize],
            };
            buy_instruction_accounts.push(buy_accounts);
        } else if info.data.starts_with(&PUMP_FUN_SELL_DISCRIMINATOR) {
            let sell_accounts = SellInstructionAccounts {
                global: account_keys[info.accounts[0] as usize],
                fee_recipient: account_keys[info.accounts[1] as usize],
                mint: account_keys[info.accounts[2] as usize],
                bonding_curve: account_keys[info.accounts[3] as usize],
                associated_bonding_curve: account_keys[info.accounts[4] as usize],
                associated_user: account_keys[info.accounts[5] as usize],
                user: account_keys[info.accounts[6] as usize],
                system_program: account_keys[info.accounts[7] as usize],
                creator_vault: account_keys[info.accounts[8] as usize],
                token_program: account_keys[info.accounts[9] as usize],
                event_authority: account_keys[info.accounts[10] as usize],
                program: account_keys[info.accounts[11] as usize],
                fee_config: account_keys[info.accounts[12] as usize],
                fee_program: account_keys[info.accounts[13] as usize],
                // 15 accounts = bonding_curve_v2_pda only (no cashback)
                // 16 accounts = user_volume_accumulator (#15) + bonding_curve_v2_pda (#16) = cashback
                cashback_enabled: info.accounts.len() >= 16,
            };
            sell_instruction_accounts.push(sell_accounts);
        } else if info.data.starts_with(
            &[
                PUMP_FUN_EVENT_LOG_DISCRIMINATOR,
                PUMP_FUN_MINT_EVENT_DISCRIMINATOR,
            ]
            .concat(),
        ) {
            let mut data = &info.data[16..];
            let mint_event: MintEvent = MintEvent::deserialize(&mut data).unwrap();
            mint_events.push(mint_event);
        } else if info.data.starts_with(
            &[
                PUMP_FUN_EVENT_LOG_DISCRIMINATOR,
                PUMP_FUN_TRADE_EVENT_DISCRIMINATOR,
            ]
            .concat(),
        ) {
            let mut data = &info.data[16..];
            let trade_event = TradeEvent::deserialize(&mut data).unwrap();
            if trade_event.is_buy {
                let buy_event = BuyEvent {
                    mint: trade_event.mint,
                    sol_amount: trade_event.sol_amount,
                    token_amount: trade_event.token_amount,
                    user: trade_event.user,
                    timestamp: trade_event.timestamp,
                    virtual_sol_reserves: trade_event.virtual_sol_reserves,
                    virtual_token_reserves: trade_event.virtual_token_reserves,
                    real_sol_reserves: trade_event.real_sol_reserves,
                    real_token_reserves: trade_event.real_sol_reserves,
                    fee_recipient: trade_event.fee_recipient,
                    fee_basis_points: trade_event.fee_basis_points,
                    fee: trade_event.fee,
                    creator: trade_event.creator,
                    creator_fee_basis_points: trade_event.creator_fee_basis_points,
                    creator_fee: trade_event.creator_fee,
                };
                buy_events.push(buy_event);
            } else {
                let sell_event = SellEvent {
                    mint: trade_event.mint,
                    sol_amount: trade_event.sol_amount,
                    token_amount: trade_event.token_amount,
                    user: trade_event.user,
                    timestamp: trade_event.timestamp,
                    virtual_sol_reserves: trade_event.virtual_sol_reserves,
                    virtual_token_reserves: trade_event.virtual_token_reserves,
                    real_sol_reserves: trade_event.real_sol_reserves,
                    real_token_reserves: trade_event.real_sol_reserves,
                    fee_recipient: trade_event.fee_recipient,
                    fee_basis_points: trade_event.fee_basis_points,
                    fee: trade_event.fee,
                    creator: trade_event.creator,
                    creator_fee_basis_points: trade_event.creator_fee_basis_points,
                    creator_fee: trade_event.creator_fee,
                };
                sell_events.push(sell_event);
            }
        }
    });

    (
        mint_events,
        buy_events,
        sell_events,
        mint_instruction_accounts,
        buy_instruction_accounts,
        sell_instruction_accounts,
    )
}
