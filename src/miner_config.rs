use crate::{
	multi_block_storage_client::ChainClientTrait,
	primitives::{AccountId, Hash},
};
use frame_support::pallet_prelude::ConstU32;
use pallet_election_provider_multi_block as multi_block;
use frame_election_provider_support::{self, SequentialPhragmen};
use sp_runtime::{PerU16, Percent, Perbill};
use sp_npos_elections;

/// Constants fetched from chain
#[derive(Debug, Clone)]
pub struct MinerConstants {
	pub pages: u32,
	pub max_winners_per_page: u32,
	pub max_backers_per_winner: u32,
	pub voter_snapshot_per_block: u32,
	pub target_snapshot_per_block: u32,
	pub max_length: u32,
	pub max_votes_per_voter: u32,
}

/// Helper function to fetch miner constants from chain API
pub async fn fetch_miner_constants<C: ChainClientTrait>(
	client: &C,
) -> Result<MinerConstants, Box<dyn std::error::Error>> {
	let pages = client
		.fetch_constant::<u32>("MultiBlockElection", "Pages")
		.await?;
	let max_winners_per_page = client
		.fetch_constant::<u32>("MultiBlockElection", "MaxWinnersPerPage")
		.await
		.unwrap_or(256);
	let max_backers_per_winner = client
		.fetch_constant::<u32>("MultiBlockElection", "MaxBackersPerWinner")
		.await
		.unwrap_or(u32::MAX);
	let voter_snapshot_per_block = client
		.fetch_constant::<u32>("MultiBlockElection", "VoterSnapshotPerBlock")
		.await
		.unwrap_or(100);
	let target_snapshot_per_block = client
		.fetch_constant::<u32>("MultiBlockElection", "TargetSnapshotPerBlock")
		.await
		.unwrap_or(100);
	let max_length = client
		.fetch_constant::<u32>("MultiBlockElection", "MaxLength")
		.await
		.unwrap_or(22500);
	let max_votes_per_voter = client
		.fetch_constant::<u32>("Staking", "MaxNominations")
		.await
		.unwrap_or(16);

	Ok(MinerConstants {
		pages,
		max_winners_per_page,
		max_backers_per_winner,
		voter_snapshot_per_block,
		target_snapshot_per_block,
		max_length,
		max_votes_per_voter,
	})
}

// Runtime configuration holder - stores values fetched from chain
use std::sync::{OnceLock, Mutex};

static RUNTIME_CONFIG: OnceLock<MinerConstants> = OnceLock::new();
static BALANCING_ITERATIONS: Mutex<usize> = Mutex::new(0);

/// Set the runtime miner constants (should be called once at startup)
pub fn set_runtime_constants(constants: MinerConstants) {
	RUNTIME_CONFIG.set(constants).expect("Runtime constants already set");
}

/// Set balancing iterations from args
pub fn set_balancing_iterations(iterations: usize) {
	*BALANCING_ITERATIONS.lock().unwrap() = iterations;
}

/// Get the runtime miner constants
pub fn get_runtime_constants() -> &'static MinerConstants {
	RUNTIME_CONFIG.get().expect("Runtime constants not set - call set_runtime_constants first")
}

/// Get balancing iterations
pub fn get_balancing_iterations() -> usize {
	*BALANCING_ITERATIONS.lock().unwrap()
}

// Simple type aliases for constants 
pub struct Pages;
pub struct MaxWinnersPerPage;
pub struct MaxBackersPerWinner;
pub struct VoterSnapshotPerBlock;
pub struct TargetSnapshotPerBlock;
pub struct MaxLength;
pub struct BalancingIterations;

// Implement Get for constants
impl sp_core::Get<u32> for Pages {
	fn get() -> u32 { 
		get_runtime_constants().pages
	}
}

impl sp_core::Get<u32> for MaxWinnersPerPage {
	fn get() -> u32 { 
		get_runtime_constants().max_winners_per_page
	}
}

impl sp_core::Get<u32> for MaxBackersPerWinner {
	fn get() -> u32 { 
		get_runtime_constants().max_backers_per_winner
	}
}

impl sp_core::Get<u32> for VoterSnapshotPerBlock {
	fn get() -> u32 { 
		get_runtime_constants().voter_snapshot_per_block
	}
}

impl sp_core::Get<u32> for TargetSnapshotPerBlock {
	fn get() -> u32 { 
		get_runtime_constants().target_snapshot_per_block
	}
}

impl sp_core::Get<u32> for MaxLength {
	fn get() -> u32 { 
		get_runtime_constants().max_length
	}
}

impl sp_core::Get<Option<sp_npos_elections::BalancingConfig>> for BalancingIterations {
	fn get() -> Option<sp_npos_elections::BalancingConfig> {
		let iterations = *BALANCING_ITERATIONS.lock().unwrap();
		if iterations > 0 {
			Some(sp_npos_elections::BalancingConfig { iterations, tolerance: 0 })
		} else {
			None
		}
	}
}

pub mod polkadot {
	use super::*;

	frame_election_provider_support::generate_solution_type!(
		#[compact]
		pub struct NposSolution16::<
			VoterIndex = u32,
			TargetIndex = u16,
			Accuracy = PerU16,
			MaxVoters = ConstU32::<22500>
		>(16)
	);

	#[derive(Debug, Clone)]
	pub struct MinerConfig;

	impl multi_block::unsigned::miner::MinerConfig for MinerConfig {
		type AccountId = AccountId;
		type Solution = NposSolution16;
		type Solver = SequentialPhragmen<AccountId, Perbill, BalancingIterations>;
		type Pages = Pages;
		type MaxVotesPerVoter = ConstU32<16>;
		type MaxWinnersPerPage = MaxWinnersPerPage;
		type MaxBackersPerWinner = MaxBackersPerWinner;
		type MaxBackersPerWinnerFinal = ConstU32<{ u32::MAX }>;
		type VoterSnapshotPerBlock = VoterSnapshotPerBlock;
		type TargetSnapshotPerBlock = TargetSnapshotPerBlock;
		type MaxLength = MaxLength;
		type Hash = Hash;
	}
}

pub mod kusama {
	use super::*;

	frame_election_provider_support::generate_solution_type!(
		#[compact]
		pub struct NposSolution24::<
			VoterIndex = u32,
			TargetIndex = u16,
			Accuracy = PerU16,
			MaxVoters = ConstU32::<12500>
		>(24)
	);

	#[derive(Debug, Clone)]
	pub struct MinerConfig;

	impl multi_block::unsigned::miner::MinerConfig for MinerConfig {
		type AccountId = AccountId;
		type Solution = NposSolution24;
		type Solver = SequentialPhragmen<AccountId, Perbill, BalancingIterations>;
		type Pages = Pages;
		type MaxVotesPerVoter = ConstU32<24>;
		type MaxWinnersPerPage = MaxWinnersPerPage;
		type MaxBackersPerWinner = MaxBackersPerWinner;
		type MaxBackersPerWinnerFinal = ConstU32<{ u32::MAX }>;
		type VoterSnapshotPerBlock = VoterSnapshotPerBlock;
		type TargetSnapshotPerBlock = TargetSnapshotPerBlock;
		type MaxLength = MaxLength;
		type Hash = Hash;
	}
}

pub mod substrate {
    use super::*;

    frame_election_provider_support::generate_solution_type!(
        #[compact]
        pub struct NposSolution16::<
            VoterIndex = u16,
            TargetIndex = u16,
            Accuracy = Percent,
            MaxVoters = ConstU32::<22500>
        >(16)
    );

    #[derive(Debug, Clone)]
    pub struct MinerConfig;

    impl multi_block::unsigned::miner::MinerConfig for MinerConfig {
        type AccountId = AccountId;
        type Solution = NposSolution16;
        type Solver = SequentialPhragmen<AccountId, Perbill, BalancingIterations>;
        type Pages = Pages;
        type MaxVotesPerVoter = ConstU32<16>;
        type MaxWinnersPerPage = MaxWinnersPerPage;
        type MaxBackersPerWinner = MaxBackersPerWinner;
        type MaxBackersPerWinnerFinal = ConstU32<{ u32::MAX }>;
        type VoterSnapshotPerBlock = VoterSnapshotPerBlock;
        type TargetSnapshotPerBlock = TargetSnapshotPerBlock;
        type MaxLength = MaxLength;
        type Hash = Hash;
    }
}

/// Simple macro to select the appropriate MinerConfig based on chain
/// Usage: with_miner_config!(chain, { code that uses MinerConfig })
#[macro_export]
macro_rules! with_miner_config {
	($chain:expr, $code:block) => {
		match $chain {
			$crate::models::Chain::Polkadot => {
				use $crate::miner_config::polkadot::MinerConfig;
				$code
			},
			$crate::models::Chain::Kusama => {
				use $crate::miner_config::kusama::MinerConfig;
				$code
			},
            $crate::models::Chain::Substrate => {
                use $crate::miner_config::substrate::MinerConfig;
                $code
            },
		}
	};
}
