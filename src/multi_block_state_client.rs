use crate::{primitives::Storage, subxt_client::Client};
use crate::raw_state_client::{NominationsLight, StakingLedger};
use pallet_staking::ValidatorPrefs;
use parity_scale_codec::{Decode, Encode};
use parity_scale_codec as codec;
use frame_support::BoundedVec;
use frame_election_provider_support::Voter;
use pallet_election_provider_multi_block::{unsigned::miner::MinerConfig};
use sp_core::Get;
use subxt::dynamic::Value;

use crate::primitives::{AccountId, Hash};
use subxt::ext::{scale_value};
use std::marker::PhantomData;

// Trait for chain client operations to enable dependency injection for testing
#[async_trait::async_trait]
pub trait ChainClientTrait: Send + Sync {
    async fn get_storage(&self, block: Option<Hash>) -> Result<Storage, Box<dyn std::error::Error>>;
    async fn fetch_constant<T: serde::de::DeserializeOwned>(
        &self,
        pallet: &str,
        constant_name: &str,
    ) -> Result<T, Box<dyn std::error::Error>>;
}

// Implementation of ChainClientTrait for Client
#[async_trait::async_trait]
impl ChainClientTrait for Client {
    async fn get_storage(&self, block: Option<Hash>) -> Result<Storage, Box<dyn std::error::Error>> {
        if let Some(block) = block {
            Ok(self.chain_api().storage().at(block))
        } else {
            Ok(self.chain_api().storage().at_latest().await?)
        }
    }

    async fn fetch_constant<T: serde::de::DeserializeOwned>(
        &self,
        pallet: &str,
        constant_name: &str,
    ) -> Result<T, Box<dyn std::error::Error>> {
        // Call the inherent method on Client using fully qualified syntax to avoid recursion
        crate::subxt_client::Client::fetch_constant(self, pallet, constant_name).await
    }
}

/// Phase enum matching the structure from pallet_election_provider_multi_block
#[derive(Debug, Clone, Copy, PartialEq, Eq, Decode, Encode)]
pub enum Phase {
	/// Nothing is happening, but it might.
	Off,
	/// Signed phase is open. The inner value is the number of blocks left in this phase.
	Signed(u32),
	/// We are validating results. The inner value is the number of blocks left in this phase.
	SignedValidation(u32),
	/// Unsigned phase. The inner value is the number of blocks left in this phase.
	Unsigned(u32),
	/// Snapshot is being created. The inner value is the remaining number of pages left to be fetched.
	Snapshot(u32),
	/// Snapshot is done, and we are waiting for `Export` to kick in.
	Done,
	/// Exporting has begun, and the given page was the last one received.
	Export(u32),
	/// The emergency phase. This locks the pallet such that only governance can change the state.
	Emergency,
}

impl Phase {
	/// Check if snapshots are available in this phase.
	/// 
	/// Snapshots are available in:
	/// - `Snapshot(0)` - when snapshot creation is complete (all pages fetched)
	/// - `Done` - snapshot is done, waiting for export
	/// - `Signed` - signed phase is open
	/// - `SignedValidation` - validating signed results
	/// - `Unsigned` - unsigned phase is open
	/// - `Export` - exporting has begun (snapshot still available but solutions no longer accepted)
	/// 
	/// Snapshots are NOT available in:
	/// - `Off` - election hasn't started
	/// - `Snapshot(n)` where n > 0 - snapshot is still being created
	/// - `Emergency` - emergency phase locks the pallet
	pub fn has_snapshot(&self) -> bool {
		match self {
			Phase::Snapshot(0) => true,  // Snapshot complete
			Phase::Done => true,
			Phase::Signed(_) => true,
			Phase::SignedValidation(_) => true,
			Phase::Unsigned(_) => true,
			Phase::Export(_) => true,
			Phase::Snapshot(_) => false,  // Still being created (n > 0)
			Phase::Off => false,
			Phase::Emergency => false,
		}
	}
}

// Generic voter type for use with MinerConfig
pub type VoterData<MC> = Voter<AccountId, <MC as MinerConfig>::MaxVotesPerVoter>;

pub type VoterSnapshotPage<MC> = 
	BoundedVec<VoterData<MC>, <MC as MinerConfig>::VoterSnapshotPerBlock>;

// Type aliases for snapshot pages using MinerConfig
pub type TargetSnapshotPage<MC> =
	BoundedVec<AccountId, <MC as MinerConfig>::TargetSnapshotPerBlock>;

pub struct ElectionSnapshotPage<MC: MinerConfig> {
	pub voters: Vec<VoterSnapshotPage<MC>>,
	pub targets: TargetSnapshotPage<MC>,
}

pub struct MultiBlockClient<C: ChainClientTrait, MC: MinerConfig> {
    client: C,
    _phantom: PhantomData<MC>,
}

impl<MC: MinerConfig> MultiBlockClient<Client, MC> {
    pub fn new(client: Client) -> Self {
        Self { client, _phantom: PhantomData }
    }
}

impl<C: ChainClientTrait, MC: MinerConfig> MultiBlockClient<C, MC> {
    pub async fn get_storage(&self, block: Option<Hash>) -> Result<Storage, Box<dyn std::error::Error>> {
        self.client.get_storage(block).await
    }

    // Get block-specific details for a given block.
    pub async fn get_block_details(&self, block: Option<Hash>) -> Result<BlockDetails, Box<dyn std::error::Error>> {
        let storage = self.get_storage(block).await?;
		let phase = self.get_phase(&storage).await?;
        let round = self.get_round(&storage).await?;
        let desired_targets = self.get_desired_targets(&storage, round).await.unwrap_or(600);
		let n_pages = MC::Pages::get();
		let block_number = self.get_block_number(&storage).await?;
		let block_hash = block;
        Ok(BlockDetails { 
			storage, 
			phase, 
			n_pages, 
			round, 
			desired_targets, 
			_block_number: block_number,
			block_hash,
		})
    }

    pub async fn get_phase(&self, storage: &Storage) -> Result<Phase, Box<dyn std::error::Error>> {
        let phase_key = subxt::dynamic::storage("MultiBlockElection", "CurrentPhase", vec![]);
        let phase = storage.fetch_or_default(&phase_key).await?;
        let phase: Phase = codec::Decode::decode(&mut phase.encoded())?;
        Ok(phase)
    }

    pub async fn get_round(&self, storage: &Storage) -> Result<u32, Box<dyn std::error::Error>> {
        let storage_key = subxt::dynamic::storage("MultiBlockElection", "Round", vec![]);
        let round = storage.fetch_or_default(&storage_key).await?;
        let round: u32 = codec::Decode::decode(&mut round.encoded())?;
        Ok(round)
    }

    pub async fn get_desired_targets(&self, storage: &Storage, round: u32) -> Result<u32, Box<dyn std::error::Error>> {
        let storage_key = subxt::dynamic::storage(
            "MultiBlockElection",
            "DesiredTargets",
            vec![Value::from(round)],
        );
        let desired_targets_entry = storage
            .fetch(&storage_key)
            .await?
            .ok_or("DesiredTargets not found")?;
        let desired_targets: u32 = codec::Decode::decode(&mut desired_targets_entry.encoded())?;
        Ok(desired_targets)
    }

    pub async fn get_block_number(&self, storage: &Storage) -> Result<u32, Box<dyn std::error::Error>> {
        let storage_key = subxt::dynamic::storage("System", "Number", vec![]);
        let block_number_entry = storage.fetch(&storage_key)
            .await?
            .ok_or("Block number not found")?;
        let block_number: u32 = codec::Decode::decode(&mut block_number_entry.encoded())?;
        Ok(block_number)
    }

    pub async fn get_min_nominator_bond(&self, storage: &Storage) -> Result<u128, Box<dyn std::error::Error>> {
        let storage_key = subxt::dynamic::storage("Staking", "MinNominatorBond", vec![]);
        let min_nominator_bond_entry = storage.fetch(&storage_key)
            .await?
            .ok_or("MinNominatorBond not found")?;
        let min_nominator_bond: u128 = codec::Decode::decode(&mut min_nominator_bond_entry.encoded())?;
        Ok(min_nominator_bond)
    }

    pub async fn get_min_validator_bond(&self, storage: &Storage) -> Result<u128, Box<dyn std::error::Error>> {
        let storage_key = subxt::dynamic::storage("Staking", "MinValidatorBond", vec![]);
        let min_validator_bond_entry = storage.fetch(&storage_key)
            .await?
            .ok_or("MinValidatorBond not found")?;
        let min_validator_bond: u128 = codec::Decode::decode(&mut min_validator_bond_entry.encoded())?;
        Ok(min_validator_bond)
    }

    pub async fn fetch_paged_voter_snapshot(&self, storage: &Storage, round: u32, page: u32) -> Result<VoterSnapshotPage<MC>, Box<dyn std::error::Error>> {
        let storage_key = subxt::dynamic::storage(
            "MultiBlockElection",
            "PagedVoterSnapshot",
            vec![Value::from(round), Value::from(page)],
        );
        let voter_snapshot_entry = storage.fetch(&storage_key)
            .await?
            .ok_or("Voter snapshot not found")?;

        let voter_snapshot: VoterSnapshotPage<MC> = codec::Decode::decode(&mut voter_snapshot_entry.encoded())?;

        Ok(voter_snapshot)
    }

    pub async fn fetch_paged_target_snapshot(&self, storage: &Storage, round: u32, page: u32) -> Result<TargetSnapshotPage<MC>, Box<dyn std::error::Error>> {
        let storage_key = subxt::dynamic::storage(
            "MultiBlockElection",
            "PagedTargetSnapshot",
            vec![Value::from(round), Value::from(page)],
        );
        let target_snapshot_entry = storage.fetch(&storage_key)
            .await?
            .ok_or("Target snapshot not found")?;
        let target_snapshot: TargetSnapshotPage<MC> = codec::Decode::decode(&mut target_snapshot_entry.encoded())?;
        Ok(target_snapshot)
    }
    
    pub async fn get_validator_prefs(&self, storage: &Storage, validator: AccountId) -> Result<ValidatorPrefs, Box<dyn std::error::Error>> {
        let encoded_validator = validator.encode();
        let storage_key = subxt::dynamic::storage("Staking", "Validators", vec![scale_value::Value::from(encoded_validator)]);
        let validator_prefs_entry = storage.fetch(&storage_key)
            .await?
            .ok_or("ValidatorPrefs not found")?;
        let validator_prefs: ValidatorPrefs = codec::Decode::decode(&mut validator_prefs_entry.encoded())?;
        Ok(validator_prefs)
    }

    pub async fn get_nominator(&self, storage: &Storage, nominator: AccountId) -> Result<Option<NominationsLight<AccountId>>, Box<dyn std::error::Error>> {
        let encoded_nominator = nominator.encode();
        let storage_key = subxt::dynamic::storage("Staking", "Nominators", vec![scale_value::Value::from(encoded_nominator)]);
        match storage.fetch(&storage_key).await? {
            Some(entry) => {
                let nominations: NominationsLight<AccountId> = codec::Decode::decode(&mut entry.encoded())?;
                Ok(Some(nominations))
            }
            None => Ok(None),
        }
    }

    // Get controller account for a given stash account
    pub async fn get_controller_from_stash(&self, storage: &Storage, stash: AccountId) -> Result<Option<AccountId>, Box<dyn std::error::Error>> {
        let encoded_stash = stash.encode();
        let storage_key = subxt::dynamic::storage("Staking", "Bonded", vec![scale_value::Value::from(encoded_stash)]);
        match storage.fetch(&storage_key).await? {
            Some(entry) => {
                let controller: AccountId = codec::Decode::decode(&mut entry.encoded())?;
                Ok(Some(controller))
            }
            None => Ok(None),
        }
    }

    pub async fn ledger(&self, storage: &Storage, account: AccountId) -> Result<Option<StakingLedger>, Box<dyn std::error::Error>> {
        let encoded_account = account.encode();
        let storage_key = subxt::dynamic::storage("Staking", "Ledger", vec![scale_value::Value::from(encoded_account)]);
        match storage.fetch(&storage_key).await? {
            Some(entry) => {
                let ledger: StakingLedger = codec::Decode::decode(&mut entry.encoded())?;
                Ok(Some(ledger))
            }
            None => Ok(None),
        }
    }
}

/// Block-specific details for a given block.
/// Contains the storage snapshot and metadata for that specific block.
/// Created via `MultiBlockClient::get_block_details()`.
pub struct BlockDetails {
	pub storage: Storage,
	pub phase: Phase,
	pub n_pages: u32,
	pub round: u32,
	pub desired_targets: u32,
	pub _block_number: u32,
	pub block_hash: Option<Hash>,
}
