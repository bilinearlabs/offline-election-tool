use jsonrpsee_core::client::ClientT;
use jsonrpsee_core::traits::ToRpcParams;
use jsonrpsee_core::ClientError;
use jsonrpsee_ws_client::{WsClient, WsClientBuilder};
use tracing::error;

use parity_scale_codec::{Decode, Encode};
use serde_json::to_value;

use sp_core::{H256};
use sp_core::storage::{StorageData, StorageKey};
use sp_core::hashing::{twox_128};
use frame_support::{Twox64Concat, StorageHasher};
use sp_version::RuntimeVersion;

use crate::primitives::{AccountId, EraIndex};


#[derive(Debug, Clone, Decode, Encode)]
pub struct UnlockChunk<Balance> {
    #[codec(compact)]
    pub value: Balance,
    #[codec(compact)]
    pub era: u32,
}

#[derive(Debug, Clone, Decode, Encode)]
pub struct StakingLedger {
    pub stash: AccountId,
    #[codec(compact)]
    pub total: u128,
    #[codec(compact)]
    pub active: u128,
    pub unlocking: Vec<UnlockChunk<u128>>,
}

#[derive(Debug, Clone, Decode)]
pub struct NominationsLight<AccountId> {
    pub targets: Vec<AccountId>,
    pub _submitted_in: EraIndex,
    pub suppressed: bool,
}

// Trait for jsonrpsee client operations to enable dependency injection for testing
#[async_trait::async_trait]
pub trait RpcClient: Send + Sync {
    async fn rpc_request<T, P>(&self, method: &str, params: P) -> Result<T, ClientError>
    where
        T: serde::de::DeserializeOwned + 'static,
        P: ToRpcParams + Send + 'static;
}

// Implementation of RpcClient for WsClient
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

#[derive(Clone)]
pub struct RawClient<C: RpcClient> {
    client: C,
}

impl RawClient<WsClient> {
    pub async fn new(node_url: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let client = WsClientBuilder::default()
            .max_response_size(20 * 1024 * 1024)     // 20MB
            .build(node_url)
            .await?;
        Ok(RawClient { client })
    }
}

#[allow(dead_code)]
impl<C: RpcClient> RawClient<C> {
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

    fn map_key<H: StorageHasher>(&self, module: &[u8], storage: &[u8], key: &[u8]) -> StorageKey {
        let prefix = self.module_prefix(module, storage);
        let key_hash = H::hash(key);
        let mut final_key = Vec::with_capacity(prefix.len() + key_hash.as_ref().len());
        final_key.extend_from_slice(&prefix);
        final_key.extend_from_slice(key_hash.as_ref());
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
            error!("Storage read error: {:?}", raw.err().unwrap());
            return Err("Storage read error".into());
        }

        match raw.unwrap() {
            None => Ok(None),
            Some(data) => {
                let encoded = data.0;
                match <T as Decode>::decode(&mut encoded.as_slice()) {
                    Ok(value) => Ok(Some(value)),
                    Err(e) => {
                        error!("Decode error: {:?}", e);
                        Err("Decode error".into())
                    }
                }
            }
        }
    }
    
    pub async fn get_runtime_version(&self) -> Result<RuntimeVersion, Box<dyn std::error::Error>> {
        let data: Result<RuntimeVersion, ClientError>  = self.client
            .rpc_request("state_getRuntimeVersion", (None::<()>,))
            .await;

        if data.is_err() {
            return Err("Error getting runtime version".into());
        }
        let data = data.unwrap();
        Ok(data)
    }

    // Get all targets when no snapshot
    // Get paged keys
    pub async fn get_keys_paged(&self, prefix: StorageKey, count: u32, start_key: Option<StorageKey>, at: Option<H256>) -> Result<Vec<StorageKey>, Box<dyn std::error::Error>> {
        let serialized_prefix = to_value(prefix).expect("StorageKey serialization infallible");
        let serialized_start = start_key.map(|k| to_value(k).expect("StorageKey serialization infallible"));
        let at_val = to_value(at).expect("Block hash serialization infallible");
        
        let keys: Result<Vec<StorageKey>, ClientError> = self.client
            .rpc_request("state_getKeysPaged", (serialized_prefix, count, serialized_start, at_val))
            .await;
        
        keys.map_err(|e| format!("Error getting keys paged: {}", e).into())
    }

    /// Get all keys from a storage map by paginating through results
    pub async fn get_all_keys(&self, prefix: StorageKey, at: Option<H256>) -> Result<Vec<StorageKey>, Box<dyn std::error::Error>> {
        let mut all_keys = Vec::new();
        let mut start_key: Option<StorageKey> = None;
        let page_size = 1000u32;

        loop {
            let keys = self.get_keys_paged(prefix.clone(), page_size, start_key.clone(), at).await?;
            
            if keys.is_empty() {
                break;
            }
            
            all_keys.extend(keys.clone());
            
            if keys.len() < page_size as usize {
                break;
            }
            
            start_key = keys.last().cloned();
        }
        
        Ok(all_keys)
    }

    fn extract_key<T: Decode>(&self, key: &StorageKey, prefix_len: usize) -> Option<T> {
        if key.0.len() > prefix_len + 8 {
            let mut bytes = &key.0[prefix_len + 8..];
            T::decode(&mut bytes).ok()
        } else {
            None
        }
    }

    // Enumerate all AccountId keys of a Twox64Concat map
    async fn enumerate_accounts(&self, module: &[u8], storage: &[u8], at: Option<H256>) -> Result<Vec<AccountId>, Box<dyn std::error::Error>> {
        let prefix_key = self.value_key(module, storage);
        let keys = self.get_all_keys(prefix_key.clone(), at).await?;
        let mut accounts = Vec::new();
        for key in keys {
            if let Some(account) = self.extract_key::<AccountId>(&key, prefix_key.0.len()) {
                accounts.push(account);
            }
        }
        accounts.sort();
        accounts.dedup();
        Ok(accounts)
    }

    // Get all validator stash accounts by enumerating Staking.Validators
    pub async fn get_validators(&self, at: Option<H256>) -> Result<Vec<AccountId>, Box<dyn std::error::Error>> {
        self.enumerate_accounts(b"Staking", b"Validators", at).await
    }

    // Get all nominator stash accounts by enumerating Staking.Nominators
    pub async fn get_nominators(&self, at: Option<H256>) -> Result<Vec<AccountId>, Box<dyn std::error::Error>> {
        self.enumerate_accounts(b"Staking", b"Nominators", at).await
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
        let client = RawClient { client: mock_client };
        let result = client.module_prefix(b"TestModule", b"TestStorage");
        let prefix = "69667818617339ad409c359884450f004348b9f44e633139d8a8187f4eead460";
        let prefix_bytes = hex::decode(prefix);
        assert_eq!(prefix_bytes.unwrap(), result);
    }

    #[tokio::test]
    async fn test_value_key() {
        let mock_client = MockRpcClient::new();
        let client = RawClient { client: mock_client };
        let result = client.value_key(b"TestModule", b"TestStorage");
            
        let value_key = "69667818617339ad409c359884450f004348b9f44e633139d8a8187f4eead460";
        let value_key_storage = StorageKey(hex::decode(value_key).unwrap());
        assert_eq!(result, value_key_storage);
    }

    #[tokio::test]
    async fn test_map_key() {
        let mock_client = MockRpcClient::new();
        let client = RawClient { client: mock_client };
        let account_id = create_test_account_id();
        let key = client.map_key::<Twox64Concat>(b"TestModule", b"TestStorage", &account_id.encode());
        
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
        let client = RawClient { client: mock_client };
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
        let client = RawClient { client: mock_client };
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

        let client = RawClient { client: mock_client };
        
        let result = client.read::<Vec<u8>>(key, None).await;

        println!("Result: {:?}", result);

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), Some(test_data));
    }

    #[tokio::test]
    async fn test_get_runtime_version() {
        let mut mock_client = MockRpcClient::new();
        let runtime_version = RuntimeVersion {
            spec_name: "test".into(),
            impl_name: "test".into(),
            authoring_version: 1,
            spec_version: 1,
            impl_version: 1,
            apis: vec![].into(),
            transaction_version: 1,
            system_version: 1,
        };
        let runtime_version_for_mock = runtime_version.clone();
        mock_client
            .expect_rpc_request::<RuntimeVersion, (Option<()>,)>()
            .with(eq("state_getRuntimeVersion"), mockall::predicate::always())
            .returning(move |_, _| Ok(runtime_version_for_mock.clone()));
        let client = RawClient { client: mock_client };
        let result = client.get_runtime_version().await;
        assert_eq!(result.unwrap(), runtime_version);
    }
}



