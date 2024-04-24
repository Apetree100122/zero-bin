use std::io::Write;

use anyhow::Result;
use clap::Parser;
use cli::Commands;
use rpc::{fetch_previous_block_hashes, fetch_prover_input, FetchProverInputRequest};

mod cli;
mod init;
mod rpc;

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
            let prev_hashes = fetch_previous_block_hashes(&rpc_url, block_number).await?;

            let prover_input = fetch_prover_input(FetchProverInputRequest {
                rpc_url: &rpc_url,
                block_number,
                checkpoint_block_number,
                prev_hashes: &prev_hashes,
            })
            .await?;
            std::io::stdout().write_all(&serde_json::to_vec(&prover_input)?)?;
        }
    }
    Ok(())
}
