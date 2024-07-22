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
use mcr_settlement_config::Config;
use std::str::FromStr;
use tracing::info;

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

//#[cfg(feature = "integration-tests")]
#[tokio::test]
async fn test_node_settlement_state() -> anyhow::Result<()> {
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

	let node_url = config.execution_config.maptos_config.client.get_rest_url()?;
	let faucet_url = config.execution_config.maptos_config.client.get_faucet_url()?;

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
	let mcr_address: Address = config.settle.mcr_contract_address.trim().parse()?;

	// Define Signers. Ceremony defines 2 signers (index 0 and 1). The first has 95% of the stakes.
	let signer_private_key = config.settle.signer_private_key.clone();
	let signer = signer_private_key.parse::<PrivateKeySigner>()?;
	let signer_address = signer.address();
	let provider_client = ProviderBuilder::new()
		.with_recommended_fillers()
		.wallet(EthereumWallet::from(alice.clone()))
		.on_builtin(&rpc_url)
		.await?;
	let contract = MCR::new(mcr_address, &provider_client);

	// Get the height for this commitment using on-chain commitment.
	let mut commitment_height = 0;
	for index in (cur_blockheight.saturating_sub(5)..=cur_blockheight).rev() {
		let MCR::getValidatorCommitmentAtBlockHeightReturn { _0: onchain_commitment_at_height } =
			contract
				.getValidatorCommitmentAtBlockHeight(U256::from(index), signer_address)
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
		suzuka_config.execution_config.maptos_config.fin.fin_rest_listen_hostname,
		suzuka_config.execution_config.maptos_config.fin.fin_rest_listen_port,
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
		} = contract
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
	// Have Alice send Bob some coins.
	let txn_hash = coin_client.transfer(&mut alice, bob.address(), 1_000, None).await?;
	rest_client.wait_for_transaction(&txn_hash).await?;

	let _ = tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
	// Have Bod send Alice some more coins.
	let txn_hash = coin_client.transfer(&mut bob, alice.address(), 1_000, None).await?;
	rest_client.wait_for_transaction(&txn_hash).await?;

	let _ = tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

	Ok(())
}
