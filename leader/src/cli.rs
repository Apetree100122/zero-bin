use std::path::PathBuf;
use std::{
    ops::RangeInclusive,
    str::{FromStr, Split},
};

use anyhow::{anyhow, Context, Result};
use clap::{Parser, Subcommand, ValueHint};
use common::prover_state::cli::CliProverStateConfig;

/// zero-bin leader config
#[derive(Parser)]
pub(crate) struct Cli {
    #[command(subcommand)]
    pub(crate) command: Command,

    #[clap(flatten)]
    pub(crate) paladin: paladin::config::Config,

    // Note this is only relevant for the leader when running in in-memory
    // mode.
    #[clap(flatten)]
    pub(crate) prover_state_config: CliProverStateConfig,
}

#[derive(Clone, Debug)]
pub(super) enum BlockNumbers {
    Single(u64),
    RangeInclusive(RangeInclusive<u64>),
}

impl FromStr for BlockNumbers {
    type Err = anyhow::Error;

    /// Parses input block numbers to determine a specific block or a range of
    /// blocks.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::from_str_intern(s)
            .with_context(|| {
                format!(
                    "Expected a single value or a range, but instead got \"{}\".",
                    s
                )
            })
            .map_err(|e| anyhow!("{e:#}"))
    }
}

impl BlockNumbers {
    fn from_str_intern(s: &str) -> anyhow::Result<Self> {
        // Did we get passed a single value?
        if let Ok(v) = s.parse::<u64>() {
            return Ok(Self::Single(v));
        }

        // Check if it's a range.
        let mut range_vals = s.split("..=");

        let start = Self::next_and_try_parse(&mut range_vals)?;
        let end = Self::next_and_try_parse(&mut range_vals)?;

        if range_vals.count() > 0 {
            return Err(anyhow!(
                "Parsed a range but there were unexpected characters afterwards!"
            ));
        }

        Ok(Self::RangeInclusive(start..=end))
    }

    fn next_and_try_parse(range_vals: &mut Split<&str>) -> anyhow::Result<u64> {
        let unparsed_val = range_vals
            .next()
            .with_context(|| "Parsing a value as a `RangeInclusive`")?;
        let res = unparsed_val
            .parse()
            .with_context(|| format!("Parsing the range val \"{}\" into a usize", unparsed_val))?;

        Ok(res)
    }
}

#[derive(Subcommand)]
pub(crate) enum Command {
    /// Reads input from stdin and writes output to stdout.
    Stdio {
        /// The previous proof output.
        #[arg(long, short = 'f', value_hint = ValueHint::FilePath)]
        previous_proof: Option<PathBuf>,
        /// If true, save the public inputs to disk on error.
        #[arg(short, long, default_value_t = false)]
        save_inputs_on_error: bool,
    },
    /// Reads input from a Jerigon node and writes output to stdout.
    Jerigon {
        // The Jerigon RPC URL.
        #[arg(long, short = 'u', value_hint = ValueHint::Url)]
        rpc_url: String,
        /// The block numbers for which to generate a proof.
        #[arg(short, long)]
        block_numbers: BlockNumbers,
        /// The checkpoint block number.
        #[arg(short, long, default_value_t = 0)]
        checkpoint_block_number: u64,
        /// The previous proof output.
        #[arg(long, short = 'f', value_hint = ValueHint::FilePath)]
        previous_proof: Option<PathBuf>,
        /// If provided, write the generated proof to this file instead of
        /// stdout.
        #[arg(long, short = 'o', value_hint = ValueHint::DirPath)]
        proof_output_dir: Option<PathBuf>,
        /// If true, save the public inputs to disk on error.
        #[arg(short, long, default_value_t = false)]
        save_inputs_on_error: bool,
    },
    /// Reads input from HTTP and writes output to a directory.
    Http {
        /// The port on which to listen.
        #[arg(short, long, default_value_t = 8080)]
        port: u16,
        /// The directory to which output should be written.
        #[arg(short, long, value_hint = ValueHint::DirPath)]
        output_dir: PathBuf,
        /// If true, save the public inputs to disk on error.
        #[arg(short, long, default_value_t = false)]
        save_inputs_on_error: bool,
    },
}
