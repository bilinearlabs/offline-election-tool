use crate::primitives::{ChainClient};
use std::{time::Duration};
use subxt::backend::{
	rpc::reconnecting_rpc_client::{ExponentialBackoff, RpcClient as ReconnectingRpcClient},
};

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
}