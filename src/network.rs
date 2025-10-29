// use subxt::{config::PolkadotConfig, utils::H256, OnlineClient};
// use crate::polkadot;

// #[derive(Debug, Decode)]
// struct IndividualExposure<AccountId, Balance> {
//     who: AccountId,
//     value: Balance,
// }

// #[derive(Debug, Decode)]
// struct Exposure<AccountId, Balance> {
//     #[codec(compact)]
//     total: Balance,
//     #[codec(compact)]
//     own: Balance,
//     others: Vec<IndividualExposure<AccountId, Balance>>,
// }
// pub struct CurrencyToVoteHandler<'a> {
//     client: &'a NetworkClient,
// }

// impl<'a> CurrencyToVoteHandler<'a> {
//     pub fn new(client: &'a NetworkClient) -> Self {
//         Self { client }
//     }

//     pub async fn factor(&self, at: H256) -> Result<u128, Box<dyn std::error::Error>> {
//         let total_issuance = self.client.get_total_issuance(at).await?;
//         Ok((total_issuance / u64::MAX as u128).max(1))
//     }

//     pub async fn convert_to_vote(&self, x: u128, at: H256) -> Result<u64, Box<dyn std::error::Error>> {
//         let factor = self.factor(at).await?;
//         Ok((x / factor).min(u64::MAX as u128) as u64)
//     }

//     pub async fn convert_to_currency(&self, x: u128, at: H256) -> Result<u128, Box<dyn std::error::Error>> {
//         let factor = self.factor(at).await?;
//         Ok(x * factor)
//     }
// }

// pub struct NetworkClient {
//     api: OnlineClient<PolkadotConfig>,
// }

// impl NetworkClient {
//     pub async fn new(node_url: &str) -> Result<Self, Box<dyn std::error::Error>> {
//         let api = OnlineClient::<PolkadotConfig>::from_url(node_url).await?;
//         Ok(NetworkClient { api })
//     }

//     pub async fn get_total_issuance(&self, at: H256) -> Result<u128, Box<dyn std::error::Error>> {
//         let total_issuance_query = polkadot::storage().balances().total_issuance();
//         let result = self.api.storage().at(at).fetch(&total_issuance_query).await;
//         if result.is_err() {
//             return Err(Box::new(result.err().unwrap()));
//         }
//         let total_issuance = result.unwrap();
//         if total_issuance.is_none() {
//             return Ok(0); // Return 0 if not found
//         }
//         let total_issuance = total_issuance.unwrap();
//         Ok(total_issuance)
//     }

//     pub async fn get_validators_and_expo_at(&self, at: H256) -> Result<(u32, Vec<(String, u128)>), Box<dyn std::error::Error>> {
//         // Get current validators
//         let validators_query = polkadot::storage().session().validators();
//         let validators_result = self.api.storage().at(at).fetch(&validators_query).await?;
//         let validators = validators_result.unwrap_or_default();

//         // Get active era
//         let active_era_query = polkadot::storage().staking().active_era();
//         let era_result = self.api.storage().at(at).fetch(&active_era_query).await?;
//         if era_result.is_none() {
//             return Err(Box::new(std::io::Error::new(std::io::ErrorKind::Other, "Era not found")));
//         }
//         let era_info = era_result.unwrap();
//         let era: u32 = era_info.index;

//         let mut validators_and_expo = vec![];

//         for validator in validators {
//             // Get exposure for this validator in current era using dynamic storage
//             let exposure_query = polkadot::storage().staking().eras_stakers_overview(era, validator.clone());
//             let exposure_result = self.api.storage().at(at).fetch(&exposure_query).await?;
        
//             if exposure_result.is_none() {
//                 continue;
//             }
//             let exposure = exposure_result.unwrap();
//             println!("{:#?}", exposure);
//             let total_stake = exposure.total;
//             validators_and_expo.push((validator.to_string(), total_stake));
//         }
//         Ok((era, validators_and_expo))
//     }
// }
