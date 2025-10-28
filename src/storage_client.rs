use jsonrpsee_core::client::ClientT;
use jsonrpsee_core::traits::ToRpcParams;
use jsonrpsee_core::ClientError;
use jsonrpsee_ws_client::{WsClient, WsClientBuilder};

use parity_scale_codec::{Decode, Encode};
use pallet_staking::{ActiveEraInfo, Exposure, ValidatorPrefs, slashing::SlashingSpans};
use sp_staking::{PagedExposureMetadata, ExposurePage};
use pallet_election_provider_multi_phase::{RoundSnapshot};
use sp_npos_elections::{VoteWeight};
use frame_support::{BoundedVec, pallet_prelude::ConstU32};

use serde_json::to_value;

use sp_core::{H256};
use sp_core::storage::{StorageData, StorageKey};
use sp_core::hashing::{twox_128};
use frame_support::{Twox64Concat, StorageHasher};
use subxt::utils::AccountId32;
use sp_version::RuntimeVersion;

use crate::primitives::{AccountId, Balance, EraIndex};


#[derive(Debug, Clone, Decode)]
struct StakingLedger {
    pub stash: AccountId32,
    pub total: Balance,
    pub active: Balance,
    pub unlocking: BoundedVec<UnlockChunk<Balance>, ConstU32<32>>,
    pub legacy_claimed_rewards: BoundedVec<u32, ConstU32<64>>,
}

#[derive(Debug, Clone, Decode)]
struct UnlockChunk<Balance> {
    pub value: Balance,
    pub era: EraIndex,
}

/// Trait for jsonrpsee client operations to enable dependency injection for testing
#[async_trait::async_trait]
pub trait RpcClient: Send + Sync {
    async fn rpc_request<T, P>(&self, method: &str, params: P) -> Result<T, ClientError>
    where
        T: serde::de::DeserializeOwned + 'static,
        P: ToRpcParams + Send + 'static;
}

/// Implementation of RpcClient for WsClient
#[async_trait::async_trait]
impl RpcClient for WsClient {
    async fn rpc_request<T, P>(&self, method: &str, params: P) -> Result<T, ClientError>
    where
        T: serde::de::DeserializeOwned + 'static,
        P: ToRpcParams + Send + 'static
    {
        self.request(method, params).await
    }
}

pub struct StorageClient<C: RpcClient> {
    client: C,
}

impl StorageClient<WsClient> {
    pub async fn new(node_url: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let client = WsClientBuilder::default()
            .max_response_size(20 * 1024 * 1024)     // 20MB
            .build(node_url)
            .await?;
        Ok(StorageClient { client })
    }
}

#[allow(dead_code)]
impl<C: RpcClient> StorageClient<C> {
    fn module_prefix(&self, module: &[u8], storage: &[u8]) -> Vec<u8> {
        let module_hash = twox_128(module);
        let storage_hash = twox_128(storage);
        let mut final_key = Vec::with_capacity(module_hash.len() + storage_hash.len());
        final_key.extend_from_slice(&module_hash);
        final_key.extend_from_slice(&storage_hash);
        final_key
    }

    fn value_key(&self, module: &[u8], storage: &[u8]) -> StorageKey {
        StorageKey(self.module_prefix(module, storage))
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
            .rpc_request("state_getStorage", (serialized_key, at_val))
            .await;

        if raw.is_err() {
            // TODO log
            println!("Error: {:?}", raw.err().unwrap());
            return Ok(None);
        }

        match 
        raw.unwrap() {
            None => Ok(None),
            Some(data) => {
                let encoded = data.0;
                Ok(<T as Decode>::decode(&mut encoded.as_slice()).ok())
            }
        }
    }

    // pub async fn get_total_issuance_at(&self, at: Option<H256>) -> Result<u128, Box<dyn std::error::Error>> {
    //     let key = self.value_key(b"Balances", b"TotalIssuance");
    //     let result = self.read::<Balance>(key, at).await?;
    //     Ok(result.unwrap_or(0))
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
        // First get the overview to know how many pages exist
        let overview = match self.get_validator_overview(era, validator.clone(), at).await? {
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
            if let Some(exposure_page) = self.get_validator_exposure_page(era, validator.clone(), page, at).await? {
                all_nominators.extend(exposure_page.others.into_iter());
            }
        }

        Ok(Some(Exposure {
            total: overview.total,
            own: overview.own,
            others: all_nominators,
        }))
    }

    pub async fn get_active_era(&self, at: Option<H256>) -> Result<Option<ActiveEraInfo>, Box<dyn std::error::Error>> {
        let active_era_key = self.value_key(b"Staking", b"ActiveEra");
        self.read::<ActiveEraInfo>(active_era_key, at).await
    }

    // Get complete exposure data for all validators in an era
    pub async fn get_all_validators_complete_exposure(&self, at: Option<H256>) -> Result<(EraIndex, Vec<(AccountId, Exposure<AccountId, Balance>)>), Box<dyn std::error::Error>> {
        let validators_key = self.value_key(b"Session", b"Validators");
        let validators = self.read::<Vec<AccountId>>(validators_key, at).await?
            .ok_or("Validators not found")?;

        let active_era = self.get_active_era(at).await?
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

    // get balances
    // pub async fn get_balance(&self, account: AccountId, at: Option<H256>) -> Result<Option<AccountData>, Box<dyn std::error::Error>> {
    //     let balance_key = self.map_key(
    //         b"Balances",
    //         b"Account",
    //         &account.encode(),
    //     );
    //     self.read::<AccountData>(balance_key, at).await
    // }

    // Get controller account for a given stash account
    pub async fn get_controller_from_stash(&self, stash: AccountId, at: Option<H256>) -> Result<Option<AccountId>, Box<dyn std::error::Error>> {
        let bonded_key = self.map_key(
            b"Staking",
            b"Bonded",
            &stash.encode(),
        );
        self.read::<AccountId>(bonded_key, at).await
    }

    pub async fn ledger(&self, controller: AccountId, at: Option<H256>) -> Result<Option<StakingLedger>, Box<dyn std::error::Error>> {
        let ledger_key = self.map_key(
            b"Staking",
            b"Ledger",
            &controller.encode(),
        );
        self.read::<StakingLedger>(ledger_key, at).await
    }

    // Only when snapshot is present
    pub async fn get_snapshot(&self, at: Option<H256>) -> Result<Option<RoundSnapshot<AccountId, (AccountId, VoteWeight, BoundedVec<AccountId, ConstU32<16>>)>>, Box<dyn std::error::Error>> {
        let snapshot_key = self.value_key(b"ElectionProviderMultiPhase", b"Snapshot");
        self.read::<RoundSnapshot<AccountId, (AccountId, VoteWeight, BoundedVec<AccountId, ConstU32<16>>)>>(
            snapshot_key,
            at
        ).await
    }

    /// Check the current election phase
    pub async fn get_election_phase(&self, at: Option<H256>) -> Result<Option<String>, Box<dyn std::error::Error>> {
        let phase_key = self.value_key(b"ElectionProviderMultiPhase", b"CurrentPhase");
        let phase = self.read::<u8>(phase_key, at).await?;
        
        let phase_name = match phase {
            Some(0) => "Off",
            Some(1) => "Signed",
            Some(2) => "Unsigned",
            Some(3) => "Emergency",
            _ => "Unknown",
        };
        
        Ok(Some(phase_name.to_string()))
    }

    // Only when snapshot is present
    pub async fn get_desired_targets(&self, at: Option<H256>) -> Result<Option<u32>, Box<dyn std::error::Error>> {
        let desired_targets = self.read::<u32>(self.value_key(b"ElectionProviderMultiPhase", b"DesiredTargets"), at).await?;
        Ok(desired_targets)
    }

    pub async fn get_validator_count(&self, at: Option<H256>) -> Result<u32, Box<dyn std::error::Error>> {
        let validator_count = self.read::<u32>(self.value_key(b"Staking", b"ValidatorCount"), at).await?;
        Ok(validator_count.unwrap_or(0))
    }

    pub async fn get_max_nominations(&self, _at: Option<H256>) -> Result<u32, Box<dyn std::error::Error>> {
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

    pub async fn get_runtime_version(&self, at: Option<H256>) -> Result<RuntimeVersion, Box<dyn std::error::Error>> {
        let at_val = to_value(at).expect("Block hash serialization infallible");
        let data: Result<RuntimeVersion, ClientError>  = self.client
            .rpc_request("state_getRuntimeVersion", (at_val,))
            .await;

        if data.is_err() {
            return Err("Error getting runtime version".into());
        }
        let data = data.unwrap();
        Ok(data)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mockall::mock;
    use mockall::predicate::*;
    use sp_core::storage::StorageData;
    use sp_runtime::Perbill;

    // Mock the RpcClient trait
    mock! {
        RpcClient {}

        #[async_trait::async_trait]
        impl RpcClient for RpcClient {
            async fn rpc_request<T, P>(&self, method: &str, params: P) -> Result<T, ClientError>
            where
                T: serde::de::DeserializeOwned + 'static,
                P: ToRpcParams + Send + 'static;
        }
    }

    fn create_test_account_id() -> AccountId {
        AccountId::from([1u8; 32])
    }

    #[tokio::test]
    async fn test_module_prefix() {
        let mock_client = MockRpcClient::new();
        let client = StorageClient { client: mock_client };
        let result = client.module_prefix(b"TestModule", b"TestStorage");
        let prefix = "69667818617339ad409c359884450f004348b9f44e633139d8a8187f4eead460";
        let prefix_bytes = hex::decode(prefix);
        assert_eq!(prefix_bytes.unwrap(), result);
    }

    #[tokio::test]
    async fn test_value_key() {
        let mock_client = MockRpcClient::new();
        let client = StorageClient { client: mock_client };
        let result = client.value_key(b"TestModule", b"TestStorage");
            
        let value_key = "69667818617339ad409c359884450f004348b9f44e633139d8a8187f4eead460";
        let value_key_storage = StorageKey(hex::decode(value_key).unwrap());
        assert_eq!(result, value_key_storage);
    }

    #[tokio::test]
    async fn test_map_key() {
        let mock_client = MockRpcClient::new();
        let client = StorageClient { client: mock_client };
        let account_id = create_test_account_id();
        let key = client.map_key(b"TestModule", b"TestStorage", &account_id.encode());
        
        let prefix = hex::decode("69667818617339ad409c359884450f004348b9f44e633139d8a8187f4eead460").unwrap();
        let key_hash = hex::decode("0d052d00259f2a8f0101010101010101010101010101010101010101010101010101010101010101").unwrap();

        let mut final_key = Vec::with_capacity(prefix.len() + key_hash.len());
        final_key.extend_from_slice(&prefix);
        final_key.extend_from_slice(&key_hash);
        let final_key_storage = StorageKey(final_key);
        assert_eq!(key, final_key_storage);
    }

    #[tokio::test]
    async fn test_double_map_key() {
        let mock_client = MockRpcClient::new();
        let client = StorageClient { client: mock_client };
        let account_id = create_test_account_id();
        let key = client.double_map_key(b"TestModule", b"TestStorage", &account_id.encode(), &account_id.encode());
        
        let prefix = hex::decode("69667818617339ad409c359884450f004348b9f44e633139d8a8187f4eead460").unwrap();
        let key_hash = hex::decode("0d052d00259f2a8f0101010101010101010101010101010101010101010101010101010101010101").unwrap();
        let mut final_key = Vec::with_capacity(prefix.len() + key_hash.len()*2);
        final_key.extend_from_slice(&prefix);
        final_key.extend_from_slice(&key_hash);
        final_key.extend_from_slice(&key_hash);
        let final_key_storage = StorageKey(final_key);
        assert_eq!(key, final_key_storage);
    }

    #[tokio::test]
    async fn test_triple_map_key() {
        let mock_client = MockRpcClient::new();
        let client = StorageClient { client: mock_client };
        let account_id = create_test_account_id();
        let key = client.triple_map_key(b"TestModule", b"TestStorage", &account_id.encode(), &account_id.encode(), &account_id.encode());
        
        let prefix = hex::decode("69667818617339ad409c359884450f004348b9f44e633139d8a8187f4eead460").unwrap();
        let key_hash = hex::decode("0d052d00259f2a8f0101010101010101010101010101010101010101010101010101010101010101").unwrap();
        let mut final_key = Vec::with_capacity(prefix.len() + key_hash.len()*3);
        final_key.extend_from_slice(&prefix);
        final_key.extend_from_slice(&key_hash);
        final_key.extend_from_slice(&key_hash);
        final_key.extend_from_slice(&key_hash);
        let final_key_storage = StorageKey(final_key);
        assert_eq!(key, final_key_storage);
    }

    #[tokio::test]
    async fn test_read_success() {
        let mut mock_client = MockRpcClient::new();
        
        // Create properly SCALE-encoded data
        let test_data = vec![1u8, 2u8, 3u8];
        let test_data_for_mock = test_data.clone();
        
        let key = StorageKey(vec![1u8; 32]);
        let params = (to_value(key.clone()).unwrap(), to_value(None::<H256>).unwrap());
        mock_client
            .expect_rpc_request()
            .with(eq("state_getStorage"), eq(params))
            .times(1)
            .returning(move |_, _| Ok(Some(StorageData(test_data_for_mock.encode()))));

        let client = StorageClient { client: mock_client };
        
        let result = client.read::<Vec<u8>>(key, None).await;

        println!("Result: {:?}", result);

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), Some(test_data));
    }

    #[tokio::test]
    async fn test_get_validator_prefs() {
        let mut mock_client = MockRpcClient::new();
        let account_id = create_test_account_id();
        
        mock_client
            .expect_rpc_request()
            .with(eq("state_getStorage"), mockall::predicate::always())
            .times(1)
            .returning(move |_: &str, _: (serde_json::Value, serde_json::Value)| Ok(Some(StorageData(ValidatorPrefs { commission: Perbill::from_percent(10), blocked: false }.encode()))));
        
        let client = StorageClient { client: mock_client };
        let result = client.get_validator_prefs(account_id, None).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), Some(ValidatorPrefs { commission: Perbill::from_percent(10), blocked: false }));
    }

    #[tokio::test]
    async fn test_get_snapshot() {
        let mut mock_client = MockRpcClient::new();
        let snapshot_repsonse = RoundSnapshot::<AccountId, (AccountId, VoteWeight, BoundedVec<AccountId, ConstU32<16>>)> {
            voters: vec![],
            targets: vec![],
        };
        let snapshot_repsonse_for_mock = snapshot_repsonse.clone();
        mock_client
            .expect_rpc_request()
            .with(eq("state_getStorage"), mockall::predicate::always())
            .times(1)
            .returning(move |_: &str, _: (serde_json::Value, serde_json::Value)| Ok(Some(StorageData(snapshot_repsonse_for_mock.encode()))));
        let client = StorageClient { client: mock_client };
        let result = client.get_snapshot(None).await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), Some(snapshot_repsonse));
    }
}

