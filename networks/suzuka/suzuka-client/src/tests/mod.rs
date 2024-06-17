use crate::{
	coin_client::CoinClient,
	rest_client::{Client, FaucetClient},
	types::LocalAccount,
};
use anyhow::{Context, Result};
use once_cell::sync::Lazy;
use std::str::FromStr;
use tokio::time::{sleep, Duration};
use url::Url;

static SUZUKA_CONFIG: Lazy<suzuka_config::Config> = Lazy::new(|| {
	let dot_movement = dot_movement::DotMovement::try_from_env().unwrap();
	let config = dot_movement.try_get_config_from_json::<suzuka_config::Config>.unwrap();
});

// :!:>section_1c
static NODE_URL: Lazy<Url> = Lazy::new(|| {

	let node_connection_address = SUZUKA_CONFIG.execution_config.maptos_config.client.maptos_faucet_rest_connection_hostname;
	let node_connection_port = SUZUKA_CONFIG.execution_config.maptos_config.client.maptos_faucet_rest_connection_port;

	let node_connection_url = format!("http://{}:{}", node_connection_address, node_connection_port);

	Url::from_str(node_connection_url.as_str())
	.unwrap()
});

static FAUCET_URL: Lazy<Url> = Lazy::new(|| {

	let faucet_listen_address = SUZUKA_CONFIG.execution_config.maptos_config.faucet.maptos_faucet_rest_listen_hostname;
	let faucet_listen_port = SUZUKA_CONFIG.execution_config.maptos_config.faucet.maptos_faucet_rest_listen_port;

	let faucet_listen_url = format!("http://{}:{}", faucet_listen_address, faucet_listen_port);

	Url::from_str(faucet_listen_url.as_str())
	.unwrap()
});
// <:!:section_1c

#[tokio::test]
async fn test_example_interaction() -> Result<()> {
	// :!:>section_1a
	let rest_client = Client::new(NODE_URL.clone());
	let faucet_client = FaucetClient::new(FAUCET_URL.clone(), NODE_URL.clone()); // <:!:section_1a

	// :!:>section_1b
	let coin_client = CoinClient::new(&rest_client); // <:!:section_1b

	// Create two accounts locally, Alice and Bob.
	// :!:>section_2
	let mut alice = LocalAccount::generate(&mut rand::rngs::OsRng);
	let bob = LocalAccount::generate(&mut rand::rngs::OsRng); // <:!:section_2

	// Print account addresses.
	println!("\n=== Addresses ===");
	println!("Alice: {}", alice.address().to_hex_literal());
	println!("Bob: {}", bob.address().to_hex_literal());

	// Create the accounts on chain, but only fund Alice.
	// :!:>section_3
	faucet_client
		.fund(alice.address(), 100_000_000)
		.await
		.context("Failed to fund Alice's account")?;
	faucet_client
		.create_account(bob.address())
		.await
		.context("Failed to fund Bob's account")?; // <:!:section_3

	// Print initial balances.
	println!("\n=== Initial Balances ===");
	println!(
		"Alice: {:?}",
		coin_client
			.get_account_balance(&alice.address())
			.await
			.context("Failed to get Alice's account balance")?
	);
	println!(
		"Bob: {:?}",
		coin_client
			.get_account_balance(&bob.address())
			.await
			.context("Failed to get Bob's account balance")?
	);

	// Have Alice send Bob some coins.
	let txn_hash = coin_client
		.transfer(&mut alice, bob.address(), 1_000, None)
		.await
		.context("Failed to submit transaction to transfer coins")?;
	rest_client
		.wait_for_transaction(&txn_hash)
		.await
		.context("Failed when waiting for the transfer transaction")?;

	// Print intermediate balances.
	println!("\n=== Intermediate Balances ===");
	// :!:>section_4
	println!(
		"Alice: {:?}",
		coin_client
			.get_account_balance(&alice.address())
			.await
			.context("Failed to get Alice's account balance the second time")?
	);
	println!(
		"Bob: {:?}",
		coin_client
			.get_account_balance(&bob.address())
			.await
			.context("Failed to get Bob's account balance the second time")?
	); // <:!:section_4

	// Have Alice send Bob some more coins.
	// :!:>section_5
	let txn_hash = coin_client
		.transfer(&mut alice, bob.address(), 1_000, None)
		.await
		.context("Failed to submit transaction to transfer coins")?; // <:!:section_5
															 // :!:>section_6
	rest_client
		.wait_for_transaction(&txn_hash)
		.await
		.context("Failed when waiting for the transfer transaction")?; // <:!:section_6

	// Print final balances.
	println!("\n=== Final Balances ===");
	println!(
		"Alice: {:?}",
		coin_client
			.get_account_balance(&alice.address())
			.await
			.context("Failed to get Alice's account balance the second time")?
	);
	println!(
		"Bob: {:?}",
		coin_client
			.get_account_balance(&bob.address())
			.await
			.context("Failed to get Bob's account balance the second time")?
	);

	sleep(Duration::from_secs(10)).await;

	let anvil_rpc_port = "8545";
	let anvil_rpc_url = format!("http://localhost:{anvil_rpc_port}");
	let anvil_ws_url = format!("ws://localhost:{anvil_rpc_port}");

	let cur_blockheight = rest_client.get_ledger_information().await?.state().block_height;
	let base_url = "http://localhost:30731";
	let state_root_hash_query = format!("/movement/v1/state-root-hash/{}", cur_blockheight);
	let state_root_hash_url = format!("{}{}", base_url, state_root_hash_query);
	println!("State root hash url: {}", state_root_hash_url);

	let client = reqwest::Client::new();

	let health_url = format!("{}/movement/v1/health", base_url);
	let response = client.get(&health_url).send().await?;
	assert!(response.status().is_success());

	println!("Health check passed");

	let response = client.get(&state_root_hash_url).send().await?;
	let state_key = response.text().await?;
	println!("State key: {}", state_key);

	Ok(())
}
