// Runtime module - compiled once and cached by Rust's incremental compilation
// Changes to other modules won't invalidate this module's cache
#[subxt::subxt(
	runtime_metadata_path = "metadata/multi_block.scale",
	derive_for_all_types = "Clone, Debug, Eq, PartialEq",
	substitute_type(
		path = "sp_npos_elections::ElectionScore",
		with = "::subxt::utils::Static<sp_npos_elections::ElectionScore>"
	),
	substitute_type(
		path = "sp_staking::PagedExposureMetadata<Balance>",
		with = "::subxt::utils::Static<u32>"
	),
	substitute_type(
		path = "sp_staking::ExposurePage<AccountId, Balance>",
		with = "::subxt::utils::Static<u32>"
	)
)]
pub mod multi_block {}