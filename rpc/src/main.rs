use std::io::Write;

use anyhow::Result;
use clap::Parser;
use cli::Commands;
use rpc::{fetch_prover_input, FetchProverInputRequest};
use trace_decoder::{processed_block_trace::ProcessingMeta, types::CodeHash};

mod cli;
mod init;
mod rpc;

fn resolve_code_hash_fn(_: &CodeHash) -> Vec<u8> {
    todo!()
}

#[tokio::main]
async fn main() -> Result<()> {
    init::tracing();
    let args = cli::Cli::parse();

    match args.command {
        Commands::Fetch {
            rpc_url,
            block_number,
            checkpoint_block_number,
        } => {
            let prover_input = fetch_prover_input(FetchProverInputRequest {
                rpc_url: &rpc_url,
                block_number,
                checkpoint_block_number,
            })
            .await?;
            let txs = prover_input.block_trace.into_txns_proof_gen_ir(
                &ProcessingMeta::new(resolve_code_hash_fn),
                prover_input.other_data.clone(),
                2,
            )?;
            // std::io::stdout().write_all(&serde_json::to_vec(&prover_input)?)?;
            std::io::stdout().write_all(&serde_json::to_vec(&txs)?)?;
        }
    }
    Ok(())
}
