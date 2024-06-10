use std::io::{Read, Write};

use anyhow::Result;
use paladin::runtime::Runtime;
use proof_gen::proof_types::GeneratedBlockProof;
use prover::BlockProverInput;

/// The main function for the stdio mode.
pub(crate) async fn stdio_main(
    runtime: Runtime,
    max_cpu_len_log: usize,
    previous: Option<GeneratedBlockProof>,
    batch_size: usize,
    save_inputs_on_error: bool,
) -> Result<()> {
    let mut buffer = String::new();
    std::io::stdin().read_to_string(&mut buffer)?;

    let des = &mut serde_json::Deserializer::from_str(&buffer);
    let input: BlockProverInput = serde_path_to_error::deserialize(des)?;
    let proof = input
        .prove(
            &runtime,
            max_cpu_len_log,
            previous.map(futures::future::ok),
            batch_size,
            save_inputs_on_error,
        )
        .await;
    runtime.close().await?;
    let proof = proof?;

    std::io::stdout().write_all(&serde_json::to_vec(&proof)?)?;

    Ok(())
}
