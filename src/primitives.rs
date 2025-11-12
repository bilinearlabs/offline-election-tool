
pub type AccountId = sp_runtime::AccountId32;
pub type Balance = u128;
pub type EraIndex = u32;

/// Subxt client used by the staking miner on all chains.
pub type ChainClient = subxt::OnlineClient<subxt::PolkadotConfig>;
/// Config used by the staking-miner
pub type Config = subxt::PolkadotConfig;
/// Storage from subxt client
pub type Storage = subxt::storage::Storage<Config, ChainClient>;
pub type Hash = subxt::utils::H256;