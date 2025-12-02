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

    // Convert plancks to native token units and format with token name
    pub fn format_stake(&self, plancks: Balance) -> String {
        match self {
            Chain::Polkadot => {
                let divisor = 10_000_000_000u128;
                let native = plancks as f64 / divisor as f64;
                format!("{} DOT", native)
            },
            Chain::Kusama => {
                let divisor = 1_000_000_000_000u128;
                let native = plancks as f64 / divisor as f64;
                format!("{} KSM", native)
            },
            Chain::Substrate => {
                format!("{} Planck", plancks)
            },
        }
    }
}

#[derive(Debug, PartialEq)]
pub struct ValidatorNomination {
    pub nominator: String,
    pub stake: Balance,
}

#[derive(Debug, PartialEq)]
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
pub struct ValidatorNominationOutput {
    pub nominator: String,
    pub stake: String,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct ValidatorOutput {
    pub stash: String,
    pub self_stake: String,
    pub total_stake: String,
    pub commission: f64,
    pub blocked: bool,
    pub nominations_count: usize,
    pub nominations: Vec<ValidatorNominationOutput>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct StakingConfig {
    pub desired_validators: u32,
    pub max_nominations: u32,
    pub min_nominator_bond: u128,
    pub min_validator_bond: u128,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct SnapshotValidator {
    pub stash: String,
    pub commission: f64,
    pub blocked: bool,
}

#[derive(Debug, PartialEq)]
pub struct SnapshotNominator {
    pub stash: String,
    pub stake: Balance,
    pub nominations: Vec<String>,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct SnapshotNominatorOutput {
    pub stash: String,
    pub stake: String,
    pub nominations: Vec<String>,
}

#[derive(Debug)]
pub struct Snapshot {
    pub validators: Vec<SnapshotValidator>,
    pub nominators: Vec<SnapshotNominator>,
    pub config: StakingConfig,
}

// Output snapshot with formatted stake strings
#[derive(Debug, Serialize)]
pub struct SnapshotOutput {
    pub validators: Vec<SnapshotValidator>,
    pub nominators: Vec<SnapshotNominatorOutput>,
    pub config: StakingConfig,
}

impl Snapshot {
    pub fn to_output(&self, chain: Chain) -> SnapshotOutput {
        SnapshotOutput {
            validators: self.validators.clone(),
            nominators: self.nominators.iter().map(|n| {
                SnapshotNominatorOutput {
                    stash: n.stash.clone(),
                    stake: chain.format_stake(n.stake),
                    nominations: n.nominations.clone(),
                }
            }).collect(),
            config: self.config.clone(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct RunParameters {
    pub algorithm: Algorithm,
    pub iterations: usize,
    pub reduce: bool,
    pub max_nominations: u32,
    pub min_nominator_bond: u128,
    pub min_validator_bond: u128,
    pub desired_validators: u32,
}

#[derive(Debug)]
pub struct SimulationResult {
    pub run_parameters: RunParameters,
    pub active_validators: Vec<Validator>
}

// Output simulation with formatted stake strings
#[derive(Debug, Serialize)]
pub struct SimulationResultOutput {
    pub run_parameters: RunParameters,
    pub active_validators: Vec<ValidatorOutput>
}

impl SimulationResult {
    pub fn to_output(&self, chain: Chain) -> SimulationResultOutput {
        SimulationResultOutput {
            run_parameters: self.run_parameters.clone(),
            active_validators: self.active_validators.iter().map(|v| {
                ValidatorOutput {
                    stash: v.stash.clone(),
                    self_stake: chain.format_stake(v.self_stake),
                    total_stake: chain.format_stake(v.total_stake),
                    commission: v.commission,
                    blocked: v.blocked,
                    nominations_count: v.nominations_count,
                    nominations: v.nominations.iter().map(|n| {
                        ValidatorNominationOutput {
                            nominator: n.nominator.clone(),
                            stake: chain.format_stake(n.stake),
                        }
                    }).collect(),
                }
            }).collect(),
        }
    }
}


