use alloy::providers::ProviderBuilder;
use alloy::signers::local::PrivateKeySigner;
use alloy_network::EthereumWallet;
use alloy_primitives::Address;
use alloy_primitives::U256;
use anyhow::Context;
use godfig::{backend::config_file::ConfigFile, Godfig};
use mcr_settlement_client::eth_client::{MOVEToken, MovementStaking, MCR};
use mcr_settlement_config::Config;
use std::str::FromStr;
use tracing::info;

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
	use tracing_subscriber::EnvFilter;

	tracing_subscriber::fmt()
		.with_env_filter(
			EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
		)
		.init();

	let dot_movement = dot_movement::DotMovement::try_from_env()?;
	let config_file = dot_movement.try_get_or_create_config_file().await?;

	// get a matching godfig object
	let godfig: Godfig<Config, ConfigFile> =
		Godfig::new(ConfigFile::new(config_file), vec!["mcr_settlement".to_string()]);
	let config: Config = godfig.try_wait_for_ready().await?;
	let rpc_url = config.eth_rpc_connection_url();

	let testing_config = config.testing.as_ref().context("Testing config not defined.")?;
	run_genesis_ceremony(
		&config,
		PrivateKeySigner::from_str(&testing_config.mcr_testing_admin_account_private_key)?,
		&rpc_url,
		Address::from_str(&testing_config.move_token_contract_address)?,
		Address::from_str(&testing_config.movement_staking_contract_address)?,
		Address::from_str(&config.settle.mcr_contract_address)?,
	)
	.await?;
	Ok(())
}

async fn run_genesis_ceremony(
	config: &Config,
	governor: PrivateKeySigner,
	rpc_url: &str,
	move_token_address: Address,
	staking_address: Address,
	mcr_address: Address,
) -> Result<(), anyhow::Error> {
	// Build alice client for MOVEToken, MCR, and staking
	let alice: PrivateKeySigner = config
		.testing
		.as_ref()
		.context("Testing config not defined.")?
		.well_known_account_private_keys
		.get(1)
		.context("No well known account")?
		.parse()?;
	let alice_address = alice.address();
	let alice_rpc_provider = ProviderBuilder::new()
		.with_recommended_fillers()
		.wallet(EthereumWallet::from(alice.clone()))
		.on_builtin(&rpc_url)
		.await?;
	let alice_staking = MovementStaking::new(staking_address, &alice_rpc_provider);
	let alice_move_token = MOVEToken::new(move_token_address, &alice_rpc_provider);

	// Build bob client for MOVEToken, MCR, and staking
	let bob: PrivateKeySigner = config
		.testing
		.as_ref()
		.context("Testing config not defined.")?
		.well_known_account_private_keys
		.get(2)
		.context("No well known account")?
		.parse()?;
	let bob_address = bob.address();
	let bob_rpc_provider = ProviderBuilder::new()
		.with_recommended_fillers()
		.wallet(EthereumWallet::from(bob.clone()))
		.on_builtin(&rpc_url)
		.await?;
	let bob_staking = MovementStaking::new(staking_address, &bob_rpc_provider);
	let bob_move_token = MOVEToken::new(move_token_address, &bob_rpc_provider);

	// Build MCR admin client to declare Alice and Bob
	let governor_rpc_provider = ProviderBuilder::new()
		.with_recommended_fillers()
		.wallet(EthereumWallet::from(governor.clone()))
		.on_builtin(&rpc_url)
		.await?;
	let governor_token = MOVEToken::new(move_token_address, &governor_rpc_provider);
	let governor_mcr = MCR::new(mcr_address, &governor_rpc_provider);
	let governor_staking = MovementStaking::new(staking_address, &governor_rpc_provider);

	// Allow Alice and Bod to stake by adding to white list.
	governor_staking
		.whitelistAddress(alice_address)
		.send()
		.await?
		.watch()
		.await
		.context("Governor failed to whilelist alice")?;
	governor_staking
		.whitelistAddress(bob_address)
		.send()
		.await?
		.watch()
		.await
		.context("Governor failed to whilelist Bod")?;

	// alice stakes for mcr
	governor_token
		.mint(alice_address, U256::from(100))
		.send()
		.await?
		.watch()
		.await
		.context("Governor failed to mint for alice")?;
	alice_move_token
		.approve(staking_address, U256::from(95))
		.send()
		.await?
		.watch()
		.await
		.context("Alice failed to approve MCR")?;
	alice_staking
		.stake(mcr_address, move_token_address, U256::from(95))
		.send()
		.await?
		.watch()
		.await
		.context("Alice failed to stake for MCR")?;

	// bob stakes for mcr
	governor_token
		.mint(bob.address(), U256::from(100))
		.send()
		.await?
		.watch()
		.await
		.context("Governor failed to mint for bob")?;
	bob_move_token
		.approve(staking_address, U256::from(5))
		.send()
		.await?
		.watch()
		.await
		.context("Bob failed to approve MCR")?;
	bob_staking
		.stake(mcr_address, move_token_address, U256::from(5))
		.send()
		.await?
		.watch()
		.await
		.context("Bob failed to stake for MCR")?;

	// mcr accepts the genesis
	info!("MCR accepts the genesis");
	governor_mcr
		.acceptGenesisCeremony()
		.send()
		.await?
		.watch()
		.await
		.context("Governor failed to accept genesis ceremony")?;
	info!("mcr accepted");

	Ok(())
}
