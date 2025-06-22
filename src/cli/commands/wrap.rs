use crate::core::{Config, SwapError, SwapResult};
use clap::Parser;
use log::info;
use solana_client::rpc_client::RpcClient;
use solana_sdk::{
    commitment_config::CommitmentConfig,
    native_token::LAMPORTS_PER_SOL,
    program_pack::Pack,
    pubkey::Pubkey,
    signature::Keypair,
    signer::Signer,
    system_instruction,
    transaction::Transaction,
};
use spl_associated_token_account::{get_associated_token_address, instruction::create_associated_token_account};

#[derive(Parser, Debug)]
pub struct WrapCommand {
    /// Amount of SOL to wrap
    pub amount: f64,
    
    /// Unwrap wSOL back to SOL
    #[clap(long)]
    pub unwrap: bool,
}

impl WrapCommand {
    pub async fn execute(self, config: Config) -> SwapResult<()> {
        let keypair = config.get_keypair()?;
        let rpc_client = RpcClient::new_with_commitment(
            config.rpc_url.clone(),
            CommitmentConfig::confirmed(),
        );
        
        let user_pubkey = keypair.pubkey();
        let native_mint = spl_token::native_mint::ID;
        let wsol_ata = get_associated_token_address(&user_pubkey, &native_mint);
        
        if self.unwrap {
            // Unwrap wSOL to SOL
            self.unwrap_sol(&rpc_client, &keypair, &wsol_ata).await
        } else {
            // Wrap SOL to wSOL
            self.wrap_sol(&rpc_client, &keypair, &wsol_ata, self.amount).await
        }
    }
    
    async fn wrap_sol(
        &self,
        rpc_client: &RpcClient,
        keypair: &Keypair,
        wsol_ata: &Pubkey,
        amount: f64,
    ) -> SwapResult<()> {
        let user_pubkey = keypair.pubkey();
        let amount_lamports = (amount * LAMPORTS_PER_SOL as f64) as u64;
        
        info!("Wrapping {} SOL to wSOL", amount);
        
        let mut instructions = vec![];
        
        // Check if wSOL ATA exists
        let wsol_account = rpc_client.get_account(wsol_ata);
        if wsol_account.is_err() {
            info!("Creating wSOL associated token account");
            let create_ata_ix = create_associated_token_account(
                &user_pubkey,
                &user_pubkey,
                &spl_token::native_mint::ID,
                &spl_token::ID,
            );
            instructions.push(create_ata_ix);
        }
        
        // Transfer SOL to wSOL account
        let transfer_ix = system_instruction::transfer(
            &user_pubkey,
            wsol_ata,
            amount_lamports,
        );
        instructions.push(transfer_ix);
        
        // Sync native account
        let sync_ix = spl_token::instruction::sync_native(
            &spl_token::ID,
            wsol_ata,
        ).unwrap();
        instructions.push(sync_ix);
        
        // Create and send transaction
        let recent_blockhash = rpc_client.get_latest_blockhash()?;
        let mut transaction = Transaction::new_with_payer(
            &instructions,
            Some(&user_pubkey),
        );
        transaction.sign(&[keypair], recent_blockhash);
        
        let signature = rpc_client.send_and_confirm_transaction(&transaction)?;
        
        info!("✅ Successfully wrapped {} SOL to wSOL", amount);
        info!("Transaction signature: {}", signature);
        info!("wSOL address: {}", wsol_ata);
        
        // Show new balance
        if let Ok(account) = rpc_client.get_account(wsol_ata) {
            if let Ok(token_account) = spl_token::state::Account::unpack(&account.data) {
                info!("New wSOL balance: {} SOL", token_account.amount as f64 / LAMPORTS_PER_SOL as f64);
            }
        }
        
        Ok(())
    }
    
    async fn unwrap_sol(
        &self,
        rpc_client: &RpcClient,
        keypair: &Keypair,
        wsol_ata: &Pubkey,
    ) -> SwapResult<()> {
        let user_pubkey = keypair.pubkey();
        
        info!("Unwrapping wSOL to SOL");
        
        // Check wSOL balance
        let wsol_account = rpc_client.get_account(wsol_ata)
            .map_err(|_| SwapError::Other("wSOL account not found".to_string()))?;
            
        let token_account = spl_token::state::Account::unpack(&wsol_account.data)
            .map_err(|_| SwapError::Other("Failed to parse wSOL account".to_string()))?;
            
        let balance = token_account.amount as f64 / LAMPORTS_PER_SOL as f64;
        info!("Current wSOL balance: {} SOL", balance);
        
        if balance == 0.0 {
            return Err(SwapError::Other("No wSOL to unwrap".to_string()));
        }
        
        // Close account to unwrap
        let close_ix = spl_token::instruction::close_account(
            &spl_token::ID,
            wsol_ata,
            &user_pubkey,
            &user_pubkey,
            &[],
        ).unwrap();
        
        // Create and send transaction
        let recent_blockhash = rpc_client.get_latest_blockhash()?;
        let mut transaction = Transaction::new_with_payer(
            &[close_ix],
            Some(&user_pubkey),
        );
        transaction.sign(&[keypair], recent_blockhash);
        
        let signature = rpc_client.send_and_confirm_transaction(&transaction)?;
        
        info!("✅ Successfully unwrapped {} SOL", balance);
        info!("Transaction signature: {}", signature);
        
        Ok(())
    }
}