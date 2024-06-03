use std::{fs::File, path::PathBuf};

use anyhow::Result;
use clap::Parser;
use cli::Command;
use common::prover_state::TableLoadStrategy;
use dotenvy::dotenv;
use ops::register;
use paladin::runtime::Runtime;
use proof_gen::types::PlonkyProofIntern;

mod cli;
mod http;
mod init;
mod jerigon;
mod stdio;

fn get_previous_proof(path: Option<PathBuf>) -> Result<Option<PlonkyProofIntern>> {
    if path.is_none() {
        return Ok(None);
    }

    let path = path.unwrap();
    let file = File::open(path)?;
    let des = &mut serde_json::Deserializer::from_reader(&file);
    let proof: PlonkyProofIntern = serde_path_to_error::deserialize(des)?;
    Ok(Some(proof))
}

#[tokio::main]
async fn main() -> Result<()> {
    dotenv().ok();
    init::tracing();

    let args = cli::Cli::parse();
    if let paladin::config::Runtime::InMemory = args.paladin.runtime {
        // If running in emulation mode, we'll need to initialize the prover
        // state here.
        args.prover_state_config
            .into_prover_state_manager()
            // Use the monolithic load strategy for the prover state when running in
            // emulation mode.
            .with_load_strategy(TableLoadStrategy::Monolithic)
            .initialize()?;
    }

    let runtime = Runtime::from_config(&args.paladin, register()).await?;

    match args.command {
        Command::Stdio {
            previous_proof,
            max_cpu_len_log,
            batch_size,
        } => {
            let previous_proof = get_previous_proof(previous_proof)?;
            stdio::stdio_main(runtime, previous_proof, max_cpu_len_log, batch_size).await?;
        }
        Command::Http {
            port,
            output_dir,
            max_cpu_len_log,
            batch_size,
        } => {
            // check if output_dir exists, is a directory, and is writable
            let output_dir_metadata = std::fs::metadata(&output_dir);
            if output_dir_metadata.is_err() {
                // Create output directory
                std::fs::create_dir(&output_dir)?;
            } else if !output_dir.is_dir() || output_dir_metadata?.permissions().readonly() {
                panic!("output-dir is not a writable directory");
            }

            http::http_main(runtime, port, output_dir, max_cpu_len_log, batch_size).await?;
        }
        Command::Jerigon {
            rpc_url,
            block_number,
            checkpoint_block_number,
            previous_proof,
            proof_output_path,
            max_cpu_len_log,
            batch_size,
        } => {
            let previous_proof = get_previous_proof(previous_proof)?;

            jerigon::jerigon_main(
                runtime,
                &rpc_url,
                block_number,
                checkpoint_block_number,
                max_cpu_len_log,
                previous_proof,
                proof_output_path,
                batch_size,
            )
            .await?;
        }
    }

    Ok(())
}
