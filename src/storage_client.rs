use jsonrpsee_core::client::ClientT;
use jsonrpsee_core::ClientError;
use jsonrpsee_ws_client::{WsClient, WsClientBuilder};

use parity_scale_codec::{Decode, Encode};
use pallet_staking::{ActiveEraInfo, Exposure, ValidatorPrefs};
use sp_staking::{PagedExposureMetadata, ExposurePage};
use pallet_election_provider_multi_phase::{self, RoundSnapshot};

use serde_json::to_value;

use sp_core::{H256};
use sp_core::storage::{StorageData, StorageKey};
use sp_core::hashing::{twox_128};
use frame_support::{Twox64Concat, StorageHasher};

use crate::primitives::{AccountId, Balance, EraIndex};

pub struct StorageClient {
    client: WsClient,
}

impl StorageClient {
    pub async fn new(node_url: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let client = WsClientBuilder::default().build(node_url).await?;
        Ok(StorageClient { client })
    }

    fn value_key(&self, module: &[u8], storage: &[u8]) -> StorageKey {
        StorageKey(self.module_prefix(module, storage))
    }
    fn module_prefix(&self, module: &[u8], storage: &[u8]) -> Vec<u8> {
        let module_hash = twox_128(module);
        let storage_hash = twox_128(storage);
        let mut final_key = Vec::with_capacity(module_hash.len() + storage_hash.len());
        final_key.extend_from_slice(&module_hash);
        final_key.extend_from_slice(&storage_hash);
        final_key
    }

    fn map_key(&self, module: &[u8], storage: &[u8], key: &[u8]) -> StorageKey {
        let prefix = self.module_prefix(module, storage);
        let key_hash = Twox64Concat::hash(key);
        let mut final_key = Vec::with_capacity(prefix.len() + key_hash.len());
        final_key.extend_from_slice(&prefix);
        final_key.extend_from_slice(&key_hash);
        StorageKey(final_key)
    }

    fn double_map_key(&self, module: &[u8], storage: &[u8], key1: &[u8], key2: &[u8]) -> StorageKey {
        let prefix = self.module_prefix(module, storage);
        let key1_hash = Twox64Concat::hash(key1);
        let key2_hash = Twox64Concat::hash(key2);
        let mut final_key = Vec::with_capacity(prefix.len() + key1_hash.len() + key2_hash.len());
        final_key.extend_from_slice(&prefix);
        final_key.extend_from_slice(&key1_hash);
        final_key.extend_from_slice(&key2_hash);
        StorageKey(final_key)
    }

    fn triple_map_key(&self, module: &[u8], storage: &[u8], key1: &[u8], key2: &[u8], key3: &[u8]) -> StorageKey {
        let prefix = self.module_prefix(module, storage);
        let key1_hash = Twox64Concat::hash(key1);
        let key2_hash = Twox64Concat::hash(key2);
        let key3_hash = Twox64Concat::hash(key3);
        let mut final_key = Vec::with_capacity(prefix.len() + key1_hash.len() + key2_hash.len() + key3_hash.len());
        final_key.extend_from_slice(&prefix);
        final_key.extend_from_slice(&key1_hash);
        final_key.extend_from_slice(&key2_hash);
        final_key.extend_from_slice(&key3_hash);
        StorageKey(final_key)
    }

    pub async fn read<T: Decode>(&self, key: StorageKey, at: Option<H256>) -> Result<Option<T>, Box<dyn std::error::Error>> {
        let serialized_key = to_value(key).expect("StorageKey serialization infallible");
        let at_val = to_value(at).expect("Block hash serialization infallible");
        let raw: Result<Option<StorageData>, ClientError> = self.client
            .request("state_getStorage", (serialized_key, at_val))
            .await;

        if raw.is_err() {
            // TODO log
            println!("Error: {:?}", raw.err().unwrap());
            return Ok(None);
        }

        match raw.unwrap() {
            None => Ok(None),
            Some(data) => {
                let encoded = data.0;
                Ok(<T as Decode>::decode(&mut encoded.as_slice()).ok())
            }
        }
    }

    pub async fn get_total_issuance_at(&self, at: Option<H256>) -> Result<u128, Box<dyn std::error::Error>> {
        let key = self.value_key(b"Balances", b"TotalIssuance");
        let result = self.read::<Balance>(key, at).await?;
        Ok(result.unwrap_or(0))
    }

    // pub async fn get_validators_and_expo_at(&self, at: H256) -> Result<(EraIndex, Vec<(AccountId, PagedExposureMetadata<Balance>)>), Box<dyn std::error::Error>> {
    //     let validators_key = self.value_key(b"Session", b"Validators");
    //     let validators = self.read::<Vec<AccountId>>(validators_key, at).await?
    //         .ok_or("Validators not found")?;

    //     let active_era_key = self.value_key(b"Staking", b"ActiveEra");
    //     let active_era = self.read::<ActiveEraInfo>(active_era_key, at).await?
    //         .ok_or("Active era not found")?;
    //     let era = active_era.index;

    //     let mut validators_and_expo = vec![];

    //     for validator in validators {
    //         let exposure_key = self.double_map_key(
    //             b"Staking",
    //             b"ErasStakersOverview", 
    //             &era.encode(),
    //             &validator.encode(),
    //         );
    //         let exposure_metadata = self.read::<PagedExposureMetadata<Balance>>(exposure_key, at).await?
    //             .ok_or("Staker exposure not found")?;

    //         validators_and_expo.push((validator, exposure_metadata));
    //     }

    //     Ok((era, validators_and_expo))
    // }

    // Get the overview metadata for a validator at a specific era (contains page_count)
    pub async fn get_validator_overview(&self, era: EraIndex, validator: AccountId, at: Option<H256>) -> Result<Option<PagedExposureMetadata<Balance>>, Box<dyn std::error::Error>> {
        let overview_key = self.double_map_key(
            b"Staking",
            b"ErasStakersOverview", 
            &era.encode(),
            &validator.encode(),
        );
        self.read::<PagedExposureMetadata<Balance>>(overview_key, at).await
    }

    // Get exposure data for a specific page of a validator's exposure
    pub async fn get_validator_exposure_page(&self, era: EraIndex, validator: AccountId, page: u32, at: Option<H256>) -> Result<Option<ExposurePage<AccountId, Balance>>, Box<dyn std::error::Error>> {
        let exposure_key = self.triple_map_key(
            b"Staking",
            b"ErasStakersPaged", 
            &era.encode(),
            &validator.encode(),
            &page.encode(),
        );
        self.read::<ExposurePage<AccountId, Balance>>(exposure_key, at).await
    }

    // Get complete exposure data for a validator by reading all pages
    pub async fn get_complete_validator_exposure(&self, era: EraIndex, validator: AccountId, at: Option<H256>) -> Result<Option<Exposure<AccountId, Balance>>, Box<dyn std::error::Error>> {
        // Clone validator to avoid ownership issues
        let validator_clone = validator.clone();
        
        // First get the overview to know how many pages exist
        let overview = match self.get_validator_overview(era, validator_clone.clone(), at).await? {
            Some(overview) => overview,
            None => return Ok(None),
        };

        let page_count = overview.page_count;
        if page_count == 0 {
            return Ok(None);
        }

        // Read all pages and combine them
        let mut all_nominators = Vec::new();

        for page in 0..page_count {
            if let Some(exposure_page) = self.get_validator_exposure_page(era, validator_clone.clone(), page, at).await? {
                all_nominators.extend(exposure_page.others);
            }
        }

        Ok(Some(Exposure {
            total: overview.total,
            own: overview.own,
            others: all_nominators,
        }))
    }

    // Get complete exposure data for all validators in an era
    pub async fn get_all_validators_complete_exposure(&self, at: Option<H256>) -> Result<(EraIndex, Vec<(AccountId, Exposure<AccountId, Balance>)>), Box<dyn std::error::Error>> {
        let validators_key = self.value_key(b"Session", b"Validators");
        let validators = self.read::<Vec<AccountId>>(validators_key, at).await?
            .ok_or("Validators not found")?;

        let active_era_key = self.value_key(b"Staking", b"ActiveEra");
        let active_era = self.read::<ActiveEraInfo>(active_era_key, at).await?
            .ok_or("Active era not found")?;
        let era = active_era.index;

        let mut validators_and_expo = vec![];

        for validator in validators {
            if let Some(complete_exposure) = self.get_complete_validator_exposure(era, validator.clone(), at).await? {
                validators_and_expo.push((validator, complete_exposure));
            }
        }

        Ok((era, validators_and_expo))
    }

    // Get validator preferences (commission and blocked status) for a specific validator
    pub async fn get_validator_prefs(&self, validator: AccountId, at: Option<H256>) -> Result<Option<ValidatorPrefs>, Box<dyn std::error::Error>> {
        let validators_key = self.map_key(
            b"Staking",
            b"Validators",
            &validator.encode(),
        );
        self.read::<ValidatorPrefs>(validators_key, at).await
    }

    /// Get stash account for a given controller account
    pub async fn get_stash_for_controller(&self, controller: AccountId, at: Option<H256>) -> Result<Option<AccountId>, Box<dyn std::error::Error>> {
        let bonded_key = self.map_key(
            b"Staking",
            b"Bonded",
            &controller.encode(),
        );
        self.read::<AccountId>(bonded_key, at).await
    }

    // Only when snapshot is present
    pub async fn get_snapshot(&self, at: Option<H256>) -> Result<Option<RoundSnapshot<AccountId, (AccountId, Balance, Vec<u32>)>>, Box<dyn std::error::Error>> {
        let snapshot = self.read::<Option<RoundSnapshot<AccountId, (AccountId, Balance, Vec<u32>)>>>(
            self.value_key(b"ElectionProviderMultiPhase", b"Snapshot"),
            at
        ).await?;
        Ok(snapshot.flatten())
    }

    // Only when snapshot is present
    pub async fn get_desired_targets(&self, at: Option<H256>) -> Result<Option<u32>, Box<dyn std::error::Error>> {
        let desired_targets = self.read::<Option<u32>>(self.value_key(b"ElectionProviderMultiPhase", b"DesiredTargets"), at).await?;
        Ok(desired_targets.flatten())
    }

    pub async fn get_validator_count(&self, at: Option<H256>) -> Result<u32, Box<dyn std::error::Error>> {
        let validator_count = self.read::<u32>(self.value_key(b"Staking", b"ValidatorCount"), at).await?;
        Ok(validator_count.unwrap_or(0))
    }

    pub async fn get_max_nominations(&self, at: Option<H256>) -> Result<u32, Box<dyn std::error::Error>> {
        // TODO not found in storage
        return Ok(16);
    }

    pub async fn get_min_nominator_bond(&self, at: Option<H256>) -> Result<Option<u128>, Box<dyn std::error::Error>> {
        let min_nominator_bond = self.read::<u128>(self.value_key(b"Staking", b"MinNominatorBond"), at).await?;
        Ok(min_nominator_bond)
    }

    pub async fn get_min_validator_bond(&self, at: Option<H256>) -> Result<Option<u128>, Box<dyn std::error::Error>> {
        let min_validator_bond = self.read::<u128>(self.value_key(b"Staking", b"MinValidatorBond"), at).await?;
        Ok(min_validator_bond)
    }
}