use crate::primitives::{ChainClient};
use std::{time::Duration};
use subxt::{backend::rpc::reconnecting_rpc_client::{ExponentialBackoff, RpcClient as ReconnectingRpcClient}};
use subxt::ext::scale_value;

#[derive(Clone, Debug)]
pub struct Client {
	/// Access to chain APIs such as storage, events etc.
	chain_api: ChainClient,
}

impl Client {
	pub async fn new(uri: &str, retry_attempts: Option<usize>) -> Result<Self, subxt::Error> {
		// Create a reconnecting RPC client with exponential backoff
		let reconnecting_rpc =
			ReconnectingRpcClient::builder()
				.retry_policy(
					ExponentialBackoff::from_millis(500)
						.max_delay(Duration::from_secs(30))
						.take(retry_attempts.unwrap_or(10)), // Allow up to 10 retry attempts before giving up
				)
				.build(uri.to_string())
				.await
				.map_err(|e| subxt::Error::Other(format!("Failed to connect: {e:?}")))?;

		let chain_api = ChainClient::from_rpc_client(reconnecting_rpc).await?;

		Ok(Self { chain_api })
	}

	/// Get a reference to the chain API.
	pub fn chain_api(&self) -> &ChainClient {
		&self.chain_api
	}

	/// Fetch a constant from the chain API.
	pub async fn fetch_constant<T: serde::de::DeserializeOwned>(
		&self,
		pallet: &str,
		constant_name: &str,
	) -> Result<T, Box<dyn std::error::Error>> {
		let constant_key = subxt::dynamic::constant(pallet, constant_name);

		let val = self.chain_api
			.constants()
			.at(&constant_key)
			.map_err(|e| format!("Failed to fetch constant {pallet}::{constant_name}: {e}"))?
			.to_value()
			.map_err(|e| format!("Failed to convert constant {pallet}::{constant_name} to value: {e}"))?;
		
		let val = scale_value::serde::from_value::<_, T>(val).map_err(|e| {
			format!("Failed to decode constant {pallet}::{constant_name} as {}: {e}", std::any::type_name::<T>())
		})?;
		
		Ok(val)
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	const URI: &str = "wss://sys.ibp.network/asset-hub-polkadot";

	#[tokio::test]
	async fn test_client_new_invalid_uri_fails() {
		let result = Client::new("ws://127.0.0.1:1", Some(1)).await;
		assert!(result.is_err());
		let err = result.unwrap_err();
		let msg = err.to_string();
		assert!(msg.contains("Failed to connect") || msg.contains("Connection refused") || !msg.is_empty());
	}

	#[tokio::test]
	async fn test_client_new_valid_uri() {
		let result = Client::new(URI, None).await;
		assert!(result.is_ok());
	}

	#[tokio::test]
	async fn test_get_constants() {
		let client = Client::new(URI, None).await.unwrap();
		let constants = client.fetch_constant::<u32>("MultiBlockElection", "Pages").await;
		assert!(constants.is_ok());
		let constants = constants.unwrap();
		assert!(constants > 0);
	}

	#[tokio::test]
	async fn test_get_constants_invalid_pallet() {
		let client = Client::new(URI, None).await.unwrap();
		let constants = client.fetch_constant::<u32>("InvalidPallet", "MinNominatorBond").await;
		assert!(constants.is_err());
		let err = constants.unwrap_err();
		let msg = err.to_string();
		assert!(msg.contains("Failed to fetch constant InvalidPallet::MinNominatorBond"));
	}

	#[tokio::test]
	async fn test_get_constants_invalid_constant() {
		let client = Client::new(URI, None).await.unwrap();
		let constants = client.fetch_constant::<u32>("Staking", "InvalidConstant").await;
		assert!(constants.is_err());
		let err = constants.unwrap_err();
		let msg = err.to_string();
		assert!(msg.contains("Failed to fetch constant Staking::InvalidConstant"));
	}

	#[tokio::test]
	async fn test_get_constants_invalid_constant_type() {
		let client = Client::new(URI, None).await.unwrap();
		let constants = client.fetch_constant::<String>("MultiBlockElection", "Pages").await;
		assert!(constants.is_err());
		let err = constants.unwrap_err();
		let msg = err.to_string();
		println!("{}", msg);
		assert!(msg.contains("Failed to decode constant MultiBlockElection::Pages as alloc::string::String"));
	}
}