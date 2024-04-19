use std::{
    collections::HashMap,
    fs::{create_dir_all, File},
    io::Write,
    path::PathBuf,
};

use anyhow::Result;
use ethereum_types::H256;
use paladin::runtime::Runtime;
use proof_gen::types::PlonkyProofIntern;

/// The main function for the jerigon mode.
pub(crate) async fn jerigon_main(
    runtime: Runtime,
    rpc_url: &str,
    block_numbers_range: [u64; 2],
    checkpoint_block_number: u64,
    previous: Option<PlonkyProofIntern>,
    proof_output_dir_opt: Option<PathBuf>,
) -> Result<()> {
    let mut block_number = block_numbers_range[0];
    let mut previous = previous;
    let mut block_hash_cache: HashMap<u64, H256> = HashMap::new();
    while block_number <= block_numbers_range[1] {
        let prover_input = rpc::fetch_prover_input(rpc::FetchProverInputRequest {
            rpc_url,
            block_number,
            checkpoint_block_number,
            block_hash_cache: &mut block_hash_cache,
        })
        .await?;

        let proof = prover_input.prove(&runtime, previous).await?;

        let proof_json = serde_json::to_vec(&proof.intern)?;
        write_proof(
            proof_json,
            proof_output_dir_opt
                .as_ref()
                .map(|p| p.join(format!("b{}.zkproof", block_number))),
        )?;

        previous = Some(proof.intern);
        block_number += 1;
    }
    runtime.close().await?;

    Ok(())
}

fn write_proof(proof: Vec<u8>, proof_output_path_opt: Option<PathBuf>) -> Result<()> {
    match proof_output_path_opt {
        Some(p) => {
            if let Some(parent) = p.parent() {
                create_dir_all(parent)?;
            }

            let mut f = File::create(p)?;
            f.write_all(&proof)?;
        }
        None => std::io::stdout().write_all(&proof)?,
    }

    Ok(())
}
