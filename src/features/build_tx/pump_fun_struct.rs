use crate::*;
use crate::{MintEvent, MintInstructionAccounts};
use borsh::BorshDeserialize;
use solana_sdk::instruction::AccountMeta;
use solana_sdk::instruction::Instruction;
use solana_sdk::pubkey::Pubkey;
use solana_sdk_ids::system_program;
use spl_associated_token_account::get_associated_token_address_with_program_id;
use spl_associated_token_account::instruction::create_associated_token_account_idempotent;

#[derive(Debug, Clone, BorshDeserialize)]
pub struct PumpFunSwapAccounts {
    pub global: Pubkey,
    pub fee_recipient: Pubkey,
    pub mint: Pubkey,
    pub bonding_curve: Pubkey,
    pub associated_bonding_curve: Pubkey,
    pub associated_user: Pubkey,
    pub user: Pubkey,
    pub system_program: Pubkey,
    pub token_program: Pubkey,
    pub creator_vault: Pubkey,
    pub event_authority: Pubkey,
    pub program: Pubkey,
    pub global_volume_accumulator: Pubkey,
    pub user_volume_accumulator: Pubkey,
    pub fee_config: Pubkey,
    pub fee_program: Pubkey,
    pub bonding_curve_v2_pda: Pubkey,
}

impl PumpFunSwapAccounts {
    pub fn from_mint(
        mint_instruction_account: &MintInstructionAccounts,
        mint_event: &MintEvent,
    ) -> Self {
        let associated_user = get_associated_token_address_with_program_id(
            &WALLET_PUB_KEY,
            &mint_instruction_account.mint,
            &mint_instruction_account.token_program,
        );
        let (creator_vault, _) = Pubkey::find_program_address(
            &[CREATOR_VAULT_SEED, &mint_event.creator.as_ref()],
            &PUMPFUN_PROGRAM_ID,
        );

        let fee_recipient = if mint_event.is_mayhem_mode {
            MAYHEM_FEE_RECIPIENT
        } else {
            PUMPFUN_FEE_RECIPIENT
        };

        let (bonding_curve_v2_pda, _) = Pubkey::find_program_address(
            &[
                BONDING_CURVE_V2_PDA_SEED,
                mint_instruction_account.mint.as_ref(),
            ],
            &PUMPFUN_PROGRAM_ID,
        );

        Self {
            global: PUMPFUN_GLOBAL,
            fee_recipient: fee_recipient,
            mint: mint_instruction_account.mint,
            bonding_curve: mint_instruction_account.bonding_curve,
            associated_bonding_curve: mint_instruction_account.associated_bonding_curve,
            associated_user: associated_user,
            user: *WALLET_PUB_KEY,
            system_program: system_program::ID,
            token_program: mint_instruction_account.token_program,
            creator_vault: creator_vault,
            event_authority: mint_instruction_account.event_authority,
            program: PUMPFUN_PROGRAM_ID,
            global_volume_accumulator: PUMPFUN_GLOBAL_VOLUME_ACCUMULATOR,
            user_volume_accumulator: *PUMPFUN_USER_VOLUME_ACCUMULATOR,
            fee_config: PUMPFUN_FEE_CONFIG,
            fee_program: PUMPFUN_FEE_PROGRAM,
            bonding_curve_v2_pda: bonding_curve_v2_pda,
        }
    }

    pub fn from_target_buy(buy_instruction_accounts: BuyInstructionAccounts) -> Self {
        let associated_user = get_associated_token_address_with_program_id(
            &WALLET_PUB_KEY,
            &buy_instruction_accounts.mint,
            &buy_instruction_accounts.token_program,
        );

        let (bonding_curve_v2_pda, _) = Pubkey::find_program_address(
            &[
                BONDING_CURVE_V2_PDA_SEED,
                buy_instruction_accounts.mint.as_ref(),
            ],
            &PUMPFUN_PROGRAM_ID,
        );

        Self {
            global: buy_instruction_accounts.global,
            fee_recipient: buy_instruction_accounts.fee_recipient,
            mint: buy_instruction_accounts.mint,
            bonding_curve: buy_instruction_accounts.bonding_curve,
            associated_bonding_curve: buy_instruction_accounts.associated_bonding_curve,
            associated_user: associated_user,
            user: *WALLET_PUB_KEY,
            system_program: buy_instruction_accounts.system_program,
            token_program: buy_instruction_accounts.token_program,
            creator_vault: buy_instruction_accounts.creator_vault,
            event_authority: buy_instruction_accounts.event_authority,
            program: buy_instruction_accounts.program,
            global_volume_accumulator: PUMPFUN_GLOBAL_VOLUME_ACCUMULATOR,
            user_volume_accumulator: *PUMPFUN_USER_VOLUME_ACCUMULATOR,
            fee_config: buy_instruction_accounts.fee_config,
            fee_program: buy_instruction_accounts.fee_program,
            bonding_curve_v2_pda: bonding_curve_v2_pda,
        }
    }

    pub fn update_creator_vault(&mut self, creator: &Pubkey) {
        let (creator_vault, _) = Pubkey::find_program_address(
            &[CREATOR_VAULT_SEED, creator.as_ref()],
            &PUMPFUN_PROGRAM_ID,
        );
        self.creator_vault = creator_vault;
    }

    pub fn get_sell_ix(&mut self, sell_amount: u64, cashback_enabled: bool) -> Instruction {
        let mut data = Vec::new();

        let min_sol_out: u64 = 1;

        data.extend_from_slice(&PUMP_FUN_SELL_DISCRIMINATOR);
        data.extend_from_slice(&sell_amount.to_le_bytes());
        data.extend_from_slice(&min_sol_out.to_le_bytes());

        let accounts = if !cashback_enabled {
            vec![
                AccountMeta::new_readonly(self.global, false), // #1 - Global
                AccountMeta::new(self.fee_recipient, false),   // #2 - Fee Recipient
                AccountMeta::new_readonly(self.mint, false),   // #3 - Mint
                AccountMeta::new(self.bonding_curve, false),   // #4 - BondingCurve
                AccountMeta::new(self.associated_bonding_curve, false), // #5 - Quote Mint (TSFart)
                AccountMeta::new(self.associated_user, false), // #6 - Associated User
                AccountMeta::new(self.user, true),             // #7 - User
                AccountMeta::new_readonly(self.system_program, false), // #8 - System Program
                AccountMeta::new(self.creator_vault, false),   // #9 - Creator Vault
                AccountMeta::new_readonly(self.token_program, false), // #10 - Token Program
                AccountMeta::new_readonly(self.event_authority, false), // #11 - Event authority
                AccountMeta::new_readonly(self.program, false), // #12 - Pump.fun program
                AccountMeta::new_readonly(self.fee_config, false), // #13 - Fee Config
                AccountMeta::new_readonly(self.fee_program, false), //#14 - Fee Program
                AccountMeta::new_readonly(self.bonding_curve_v2_pda, false), // #15 - Bonding Curve V2 PDA
            ]
        } else {
            vec![
                AccountMeta::new_readonly(self.global, false), // #1 - Global
                AccountMeta::new(self.fee_recipient, false),   // #2 - Fee Recipient
                AccountMeta::new_readonly(self.mint, false),   // #3 - Mint
                AccountMeta::new(self.bonding_curve, false),   // #4 - BondingCurve
                AccountMeta::new(self.associated_bonding_curve, false), // #5 - Quote Mint (TSFart)
                AccountMeta::new(self.associated_user, false), // #6 - Associated User
                AccountMeta::new(self.user, true),             // #7 - User
                AccountMeta::new_readonly(self.system_program, false), // #8 - System Program
                AccountMeta::new(self.creator_vault, false),   // #9 - Creator Vault
                AccountMeta::new_readonly(self.token_program, false), // #10 - Token Program
                AccountMeta::new_readonly(self.event_authority, false), // #11 - Event authority
                AccountMeta::new_readonly(self.program, false), // #12 - Pump.fun program
                AccountMeta::new_readonly(self.fee_config, false), // #13 - Fee Config
                AccountMeta::new_readonly(self.fee_program, false), //#14 - Fee Program
                AccountMeta::new(self.user_volume_accumulator, false), // #15 - User volume accumulator PDA
                AccountMeta::new_readonly(self.bonding_curve_v2_pda, false), // #16 - Bonding Curve V2 PDA
            ]
        };

        Instruction {
            program_id: PUMPFUN_PROGRAM_ID,
            accounts,
            data,
        }
    }

    pub fn get_buy_ix(&mut self, sol_amount: f64, token_price: f64) -> Instruction {
        let mut data = Vec::new();

        // Pump.fun takes a ~1% protocol fee from the SOL input.
        // Gross up the amount so the effective buy is exactly what the user configured.
        let pumpfun_fee_rate: f64 = 0.01; // 1%
        let gross_sol_amount = sol_amount / (1.0 - pumpfun_fee_rate);
        let exact_sol_in: u64 = gross_sol_amount.round() as u64;
        let expected_tokens_out: f64 = ((sol_amount / 10f64.powi(9)) / token_price) * 10f64.powi(6);
        let min_tokens_out: u64 = (expected_tokens_out / *SLIPPAGE).trunc() as u64;

        data.extend_from_slice(&PUMP_FUN_BUY_EXACT_SOL_IN_DISCRIMINATOR);
        data.extend_from_slice(&exact_sol_in.to_le_bytes());
        data.extend_from_slice(&min_tokens_out.to_le_bytes());

        let accounts = vec![
            AccountMeta::new_readonly(self.global, false), // #1 - Global
            AccountMeta::new(self.fee_recipient, false),   // #2 - Fee Recipient
            AccountMeta::new_readonly(self.mint, false),   // #3 - Mint
            AccountMeta::new(self.bonding_curve, false),   // #4 - BondingCurve
            AccountMeta::new(self.associated_bonding_curve, false), // #5 - Quote Mint (TSFart)
            AccountMeta::new(self.associated_user, false), // #6 - Associated User
            AccountMeta::new(self.user, true),             // #7 - User
            AccountMeta::new_readonly(self.system_program, false), // #8 - System Program
            AccountMeta::new_readonly(self.token_program, false), // #9 - Token Program
            AccountMeta::new(self.creator_vault, false),   // #10 - Creator Vault
            AccountMeta::new_readonly(self.event_authority, false), // #11 - Event authority
            AccountMeta::new_readonly(self.program, false), // #12 - Pump.fun program
            AccountMeta::new(self.global_volume_accumulator, false), // #13 - Global volume accumulator
            AccountMeta::new(self.user_volume_accumulator, false), // #14 - User volume accumulator
            AccountMeta::new_readonly(self.fee_config, false),              // #15 - Fee Config
            AccountMeta::new_readonly(self.fee_program, false),             //#16 - Fee Program
            AccountMeta::new_readonly(self.bonding_curve_v2_pda, false), // #17 - Bonding Curve V2 PDA
        ];

        Instruction {
            program_id: PUMPFUN_PROGRAM_ID,
            accounts,
            data,
        }
    }

    pub fn get_create_ata_idempotent_ix(&self) -> Instruction {
        let create_token_ata = create_associated_token_account_idempotent(
            &*WALLET_PUB_KEY,
            &*WALLET_PUB_KEY,
            &self.mint,
            &self.token_program,
        );
        create_token_ata
    }
}
