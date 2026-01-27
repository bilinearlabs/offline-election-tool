use serde::{Serialize, Deserialize};
use sp_core::crypto::{Ss58AddressFormat};

use crate::primitives::{Balance};

#[derive(Debug, Clone, Copy, clap::ValueEnum)]
pub enum Chain {
    Polkadot,  // SS58 version 0
    Kusama,    // SS58 version 2
    Substrate, // SS58 version 42
}

#[derive(Debug, Clone, Copy, PartialEq, clap::ValueEnum, Deserialize, Serialize)]
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
    pub staking_stats: StakingStats,
    pub active_validators: Vec<Validator>,
}

#[derive(Debug)]
pub struct StakingStats {
    pub total_staked: Balance,
    pub lowest_staked: Balance,
    pub avg_staked: Balance,
}

#[derive(Debug, Serialize)]
pub struct StakingStatsOutput {
    pub total_staked: String,
    pub lowest_staked: String,
    pub avg_staked: String,
}

// Output simulation with formatted stake strings
#[derive(Debug, Serialize)]
pub struct SimulationResultOutput {
    pub run_parameters: RunParameters,
    pub staking_stats: StakingStatsOutput,
    pub active_validators: Vec<ValidatorOutput>,
}

impl SimulationResult {
    pub fn to_output(&self, chain: Chain) -> SimulationResultOutput {
        SimulationResultOutput {
            run_parameters: self.run_parameters.clone(),
            staking_stats: StakingStatsOutput {
                total_staked: chain.format_stake(self.staking_stats.total_staked),
                lowest_staked: chain.format_stake(self.staking_stats.lowest_staked),
                avg_staked: chain.format_stake(self.staking_stats.avg_staked),
            },
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chain_ss58_address_format() {
        assert_eq!(Chain::Polkadot.ss58_address_format(), Ss58AddressFormat::custom(0));
        assert_eq!(Chain::Kusama.ss58_address_format(), Ss58AddressFormat::custom(2));
        assert_eq!(Chain::Substrate.ss58_address_format(), Ss58AddressFormat::custom(42));
    }

    #[test]
    fn test_chain_format_stake() {
        assert!(Chain::Polkadot.format_stake(10_000_000_000).starts_with("1 DOT"));
        assert!(Chain::Kusama.format_stake(1_000_000_000_000).starts_with("1 KSM"));
        assert_eq!(Chain::Substrate.format_stake(123), "123 Planck");
    }

    #[test]
    fn test_snapshot_to_output_polkadot() {
        let snapshot = Snapshot {
            validators: vec![],
            nominators: vec![SnapshotNominator {stash: "x".to_string(), stake: 10_000_000_000, nominations: vec![]}],
            config: StakingConfig {desired_validators: 1, max_nominations: 16, min_nominator_bond: 0, min_validator_bond: 0},
        };
        let out = snapshot.to_output(Chain::Polkadot);
        assert_eq!(out.nominators[0].stake, "1 DOT");
    }

    #[test]
    fn test_snapshot_to_output_kusama() {
        let s = Snapshot {
            validators: vec![],
            nominators: vec![SnapshotNominator { stash: "x".to_string(), stake: 1_000_000_000_000, nominations: vec![] }],
            config: StakingConfig { desired_validators: 1, max_nominations: 24, min_nominator_bond: 0, min_validator_bond: 0 },
        };
        let out = s.to_output(Chain::Kusama);
        assert!(out.nominators[0].stake.starts_with("1 KSM"));
    }

    #[test]
    fn test_snapshot_to_output_substrate() {
        let snapshot = Snapshot {
            validators: vec![],
            nominators: vec![SnapshotNominator { stash: "x".to_string(), stake: 999, nominations: vec![] }],
            config: StakingConfig { desired_validators: 1, max_nominations: 16, min_nominator_bond: 0, min_validator_bond: 0 },
        };
        let out = snapshot.to_output(Chain::Substrate);
        assert_eq!(out.nominators[0].stake, "999 Planck");
    }

    #[test]
    fn test_simulation_result_to_output_all_chains() {
        let result = SimulationResult {
            run_parameters: RunParameters {
                algorithm: Algorithm::SeqPhragmen,
                iterations: 0,
                reduce: false,
                max_nominations: 16,
                min_nominator_bond: 0,
                min_validator_bond: 0,
                desired_validators: 1,
            },
            staking_stats: StakingStats { total_staked: 1_000_000_000_000, lowest_staked: 100, avg_staked: 500 },
            active_validators: vec![Validator {
                stash: "x".to_string(),
                self_stake: 100,
                total_stake: 1000,
                commission: 0.0,
                blocked: false,
                nominations_count: 0,
                nominations: vec![],
            }],
        };
        let out_dot = result.to_output(Chain::Polkadot);
        assert!(out_dot.staking_stats.total_staked.starts_with("100 DOT"));
        let out_ksm = result.to_output(Chain::Kusama);
        assert!(out_ksm.staking_stats.total_staked.starts_with("1 KSM"));
        let out_sub = result.to_output(Chain::Substrate);
        assert_eq!(out_sub.staking_stats.total_staked, "1000000000000 Planck");
    }
}

