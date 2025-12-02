use serde::{Serialize, Deserialize};
use sp_core::crypto::{Ss58AddressFormat};

use crate::primitives::{Balance};

#[derive(Debug, Clone, Copy, clap::ValueEnum)]
pub enum Chain {
    Polkadot,  // SS58 version 0
    Kusama,    // SS58 version 2
    Substrate, // SS58 version 42
}

#[derive(Debug, Clone, Copy, clap::ValueEnum, Deserialize, Serialize)]
pub enum Algorithm {
    SeqPhragmen,
    Phragmms,
}

impl Chain {
    pub fn ss58_address_format(&self) -> Ss58AddressFormat {
        match self {
            Chain::Polkadot => Ss58AddressFormat::custom(0),
            Chain::Kusama => Ss58AddressFormat::custom(2),
            Chain::Substrate => Ss58AddressFormat::custom(42),
        }
    }
}

#[derive(Debug, Serialize, PartialEq)]
pub struct ValidatorNomination {
    pub nominator: String,
    pub stake: Balance,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct Validator {
    pub stash: String,
    pub self_stake: Balance,
    pub total_stake: Balance,
    pub commission: f64,
    pub blocked: bool,
    pub nominations_count: usize,
    pub nominations: Vec<ValidatorNomination>,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct StakingConfig {
    pub desired_validators: u32,
    pub max_nominations: u32,
    pub min_nominator_bond: u128,
    pub min_validator_bond: u128,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct SnapshotValidator {
    pub stash: String,
    pub commission: f64,
    pub blocked: bool,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct SnapshotNominator {
    pub stash: String,
    pub stake: Balance,
    pub nominations: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct Snapshot {
    pub validators: Vec<SnapshotValidator>,
    pub nominators: Vec<SnapshotNominator>,
    pub config: StakingConfig,
}


