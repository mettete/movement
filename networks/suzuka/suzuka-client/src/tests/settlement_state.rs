use alloy_primitives::Address;
use alloy_primitives::U256;
use aptos_sdk::{
	coin_client::CoinClient,
	rest_client::{Client as AptosClient, FaucetClient},
	types::{block_info::BlockInfo, LocalAccount},
};
use url::Url;

use alloy::providers::ProviderBuilder;
use alloy::signers::local::PrivateKeySigner;
use alloy_network::EthereumWallet;
use anyhow::Context;
use godfig::{backend::config_file::ConfigFile, Godfig};
use mcr_settlement_client::eth_client::{MOVEToken, MovementStaking, MCR};
use mcr_settlement_config::Config as McrConfig;
use std::str::FromStr;
use suzuka_config::Config as SuzukaConfig;
use tracing::info;

async fn run_genesis_ceremony(
	config: &McrConfig,
	governor: PrivateKeySigner,
	rpc_url: &str,
	move_token_address: Address,
	staking_address: Address,
	mcr_address: Address,
) -> Result<(), anyhow::Error> {
	// Build validator client for MOVEToken, MCR, and staking
	// Validator is the e2e started node that we test.
	let validator: PrivateKeySigner = config.settle.signer_private_key.clone().parse()?;
	let validator_address = validator.address();
	let validator_rpc_provider = ProviderBuilder::new()
		.with_recommended_fillers()
		.wallet(EthereumWallet::from(validator.clone()))
		.on_builtin(&rpc_url)
		.await?;
	let validator_staking = MovementStaking::new(staking_address, &validator_rpc_provider);
	let validator_move_token = MOVEToken::new(move_token_address, &validator_rpc_provider);

	// Build bob client for MOVEToken, MCR, and staking
	// Bod act as another validator that we don't test.
	// It's to have at least 2 staking validator.
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

	// Build MCR admin client to declare Validator and Bob
	let governor_rpc_provider = ProviderBuilder::new()
		.with_recommended_fillers()
		.wallet(EthereumWallet::from(governor.clone()))
		.on_builtin(&rpc_url)
		.await?;
	let governor_token = MOVEToken::new(move_token_address, &governor_rpc_provider);
	let governor_mcr = MCR::new(mcr_address, &governor_rpc_provider);
	let governor_staking = MovementStaking::new(staking_address, &governor_rpc_provider);

	// Allow Validator and Bod to stake by adding to white list.
	governor_staking
		.whitelistAddress(validator_address)
		.send()
		.await?
		.watch()
		.await
		.context("Governor failed to whilelist validator")?;
	governor_staking
		.whitelistAddress(bob_address)
		.send()
		.await?
		.watch()
		.await
		.context("Governor failed to whilelist Bod")?;

	// alice stakes for mcr
	info!("Validator stakes for MCR");
	let token_name = governor_token.name().call().await.context("Failed to get token name")?;
	info!("Token name: {}", token_name._0);

	// debug: this is showing up correctly
	let has_minter_role = governor_token
		.hasMinterRole(governor.address())
		.call()
		.await
		.context("Failed to check if governor has minter role")?;
	info!("Governor Has minter role for governor: {}", has_minter_role._0);

	let has_minter_role_from_alice = validator_move_token
		.hasMinterRole(governor.address())
		.call()
		.await
		.context("Failed to check if governor has minter role")?;
	info!("Governoe Has minter role for Validator: {}", has_minter_role_from_alice._0);

	//info!("config chain_id: {}",config.eth_chain_id.clone().to_string());
	//info!("governor chain_id: {}", governor_rpc_provider.get_chain_id().await.context("Failed to get chain id")?.to_string());

	// debug: this is showing up correctly
	let alice_hash_minter_role = governor_token
		.hasMinterRole(validator_address)
		.call()
		.await
		.context("Failed to check if alice has minter role")?;
	info!("Validator has minter role for governor: {}", alice_hash_minter_role._0);

	// validator stakes for mcr
	governor_token
		.mint(validator_address, U256::from(100))
		//		.gas(100000)
		.send()
		.await?
		.watch()
		.await
		.context("Governor failed to mint for validator")?;
	validator_move_token
		.approve(staking_address, U256::from(95))
		.gas(5000000)
		.send()
		.await?
		.watch()
		.await
		.context("Validator failed to approve MCR")?;
	validator_staking
		.stake(mcr_address, move_token_address, U256::from(95))
		.gas(100000)
		.send()
		.await?
		.watch()
		.await
		.context("Validator failed to stake for MCR")?;

	// bob stakes for mcr
	governor_token
		.mint(bob.address(), U256::from(100))
		.gas(100000)
		.send()
		.await?
		.watch()
		.await
		.context("Governor failed to mint for bob")?;
	bob_move_token
		.approve(staking_address, U256::from(5))
		.gas(100000)
		.send()
		.await?
		.watch()
		.await
		.context("Bob failed to approve MCR")?;
	bob_staking
		.stake(mcr_address, move_token_address, U256::from(5))
		.gas(100000)
		.send()
		.await?
		.watch()
		.await
		.context("Bob failed to stake for MCR")?;

	// mcr accepts the genesis
	info!("MCR accepts the genesis");
	governor_mcr
		.acceptGenesisCeremony()
		.gas(100000)
		.send()
		.await?
		.watch()
		.await
		.context("Governor failed to accept genesis ceremony")?;
	info!("mcr accepted");

	Ok(())
}

//#[cfg(feature = "integration-tests")]
#[tokio::test]
async fn test_node_settlement_state() -> anyhow::Result<()> {
	use tracing_subscriber::EnvFilter;
	tracing_subscriber::fmt()
		.with_env_filter(
			EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
		)
		.init();

	info!("Begin test_client_settlement");

	let dot_movement = dot_movement::DotMovement::try_from_env()?;
	let config_file = dot_movement.try_get_or_create_config_file().await?;

	// get a matching godfig object
	let godfig: Godfig<SuzukaConfig, ConfigFile> =
		Godfig::new(ConfigFile::new(config_file), vec![]);
	let config: SuzukaConfig = godfig.try_wait_for_ready().await?;

	let rpc_url = config.mcr.eth_rpc_connection_url();

	let testing_config = config.mcr.testing.as_ref().context("Testing config not defined.")?;
	run_genesis_ceremony(
		&config.mcr,
		PrivateKeySigner::from_str(&testing_config.mcr_testing_admin_account_private_key)?,
		&rpc_url,
		Address::from_str(&testing_config.move_token_contract_address)?,
		Address::from_str(&testing_config.movement_staking_contract_address)?,
		Address::from_str(&config.mcr.settle.mcr_contract_address)?,
	)
	.await?;

	let connection_host =
		config.execution_config.maptos_config.client.maptos_rest_connection_hostname;
	let connection_port = config.execution_config.maptos_config.client.maptos_rest_connection_port;
	let node_url: Url = format!("http://{}:{}", connection_host, connection_port).parse()?;

	let connection_host =
		config.execution_config.maptos_config.faucet.maptos_faucet_rest_listen_hostname;
	let connection_port =
		config.execution_config.maptos_config.faucet.maptos_faucet_rest_listen_port;
	let faucet_url: Url = format!("http://{}:{}", connection_host, connection_port).parse()?;

	//1) Start Alice an Bod transfer transactions.
	// Loop on Alice and Bod transfer to produce Tx and block
	tokio::spawn({
		let node_url = node_url.clone();
		let faucet_url = faucet_url.clone();
		async move {
			loop {
				tracing::info!("Run run_alice_bob_tx");
				if let Err(err) = run_alice_bob_tx(&node_url, &faucet_url).await {
					panic!("Alice and Bob transfer Tx fail:{err}");
				}
				let _ = tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
			}
		}
	});

	// Wait for some block to be executed.
	let _ = tokio::time::sleep(tokio::time::Duration::from_millis(1000)).await;

	let client = reqwest::Client::new();

	// Get current node commitment
	let node_commitment_uri = "movement/v1/current_commitment";
	let node_commitment_url = format!("{}{}", node_url, node_commitment_uri);
	let response = client.get(&node_commitment_url).send().await?;
	let node_commitment = response.text().await?;

	let rest_client = AptosClient::new(node_url.clone());
	let cur_blockheight = rest_client.get_ledger_information().await?.state().block_height;

	// Init smart contract connection
	let mcr_address: Address = config.mcr.settle.mcr_contract_address.trim().parse()?;

	// Define Signers. Ceremony defines 2 signers (index 1 and 2). The first has 95% of the stakes.
	//
	let validator_private_key = config.mcr.settle.signer_private_key.clone();
	let validator_private_key = validator_private_key.parse::<PrivateKeySigner>()?;
	let validator_address = validator_private_key.address();
	let provider_client = ProviderBuilder::new()
		.with_recommended_fillers()
		.wallet(EthereumWallet::from(validator_private_key.clone()))
		.on_builtin(&rpc_url)
		.await?;
	let validator_contract = MCR::new(mcr_address, &provider_client);

	// Get the height for this commitment using on-chain commitment.
	let mut commitment_height = 0;
	for index in (cur_blockheight.saturating_sub(5)..=cur_blockheight).rev() {
		let MCR::getValidatorCommitmentAtBlockHeightReturn { _0: onchain_commitment_at_height } =
			validator_contract
				.getValidatorCommitmentAtBlockHeight(U256::from(index), validator_address)
				.call()
				.await?;
		let onchain_commitment_str = hex::encode(&onchain_commitment_at_height.commitment);

		if onchain_commitment_str == node_commitment {
			commitment_height = index;
			break;
		}
	}
	assert!(commitment_height != 0, "Commitment not found on the smart contract.");

	// Get current fin state.
	let finview_node_url = format!(
		"{}:{}",
		config.execution_config.maptos_config.fin.fin_rest_listen_hostname,
		config.execution_config.maptos_config.fin.fin_rest_listen_port,
	);
	let fin_state_root_hash_query = "/movement/v1/get-finalized-block-info";
	let fin_state_root_hash_url =
		format!("http://{}{}", finview_node_url, fin_state_root_hash_query);
	println!("block fin_state_root_hash_url:{fin_state_root_hash_url:?}");
	let response = client.get(&fin_state_root_hash_url).send().await?;
	println!("block response:{response:?}");
	let fin_block_info: BlockInfo = response.json().await?;

	// Get block for this height
	let rest_client = AptosClient::new(node_url.clone());
	let block = rest_client.get_block_by_height(commitment_height, false).await?;

	// Compare the block hash with fin_block_info id.
	assert_eq!(
		block.inner().block_hash,
		aptos_sdk::rest_client::aptos_api_types::HashValue(fin_block_info.id()),
		"Fin state doesn't correspond to current block"
	);

	// Wait to get the commitment accepted.
	let mut accepted_block_commitment = None;
	let mut nb_try = 0;
	while accepted_block_commitment.is_none() && nb_try < 20 {
		// Try to get an accepted commitment
		let MCR::getAcceptedCommitmentAtBlockHeightReturn {
			_0: get_accepted_commitment_at_block_height,
		} = validator_contract
			.getAcceptedCommitmentAtBlockHeight(U256::from(commitment_height))
			.call()
			.await?;
		//0 height means None.
		if get_accepted_commitment_at_block_height.height != U256::from(0) {
			accepted_block_commitment = Some(get_accepted_commitment_at_block_height);
			break;
		}
		nb_try += 1;
		let _ = tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
	}
	assert!(accepted_block_commitment.is_some(), "Commitment not accepted.");

	Ok(())
}

async fn run_alice_bob_tx(node_url: &Url, faucet_url: &Url) -> anyhow::Result<()> {
	let rest_client = AptosClient::new(node_url.clone());
	let faucet_client = FaucetClient::new(faucet_url.clone(), node_url.clone()); // <:!:section_1a

	let coin_client = CoinClient::new(&rest_client); // <:!:section_1b

	// Create two accounts locally, Alice and Bob.
	let mut alice = LocalAccount::generate(&mut rand::rngs::OsRng);
	let mut bob = LocalAccount::generate(&mut rand::rngs::OsRng); // <:!:section_2

	faucet_client.fund(alice.address(), 100_000_000).await?;
	faucet_client.fund(bob.address(), 100_000_000).await?;
	let _ = tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
	loop {
		// Have Alice send Bob some coins.
		let txn_hash = coin_client.transfer(&mut alice, bob.address(), 1_000, None).await?;
		rest_client.wait_for_transaction(&txn_hash).await?;

		let _ = tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
		// Have Bod send Alice some more coins.
		let txn_hash = coin_client.transfer(&mut bob, alice.address(), 1_000, None).await?;
		rest_client.wait_for_transaction(&txn_hash).await?;

		let _ = tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
	}
}
