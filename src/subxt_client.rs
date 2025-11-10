use crate::primitives::{ChainClient};
use std::{time::Duration};
use subxt::{backend::rpc::reconnecting_rpc_client::{ExponentialBackoff, RpcClient as ReconnectingRpcClient}, client::RuntimeVersion};
use subxt::ext::scale_value;

#[derive(Clone, Debug)]
pub struct Client {
	/// Access to chain APIs such as storage, events etc.
	chain_api: ChainClient,
}

impl Client {
	pub async fn new(uri: &str) -> Result<Self, subxt::Error> {
		// Create a reconnecting RPC client with exponential backoff
		let reconnecting_rpc =
			ReconnectingRpcClient::builder()
				.retry_policy(
					ExponentialBackoff::from_millis(500)
						.max_delay(Duration::from_secs(30))
						.take(10), // Allow up to 10 retry attempts before giving up
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