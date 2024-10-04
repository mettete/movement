use alloy::network::EthereumWallet;
use alloy::providers::ProviderBuilder;
use alloy::signers::local::PrivateKeySigner;
use alloy_primitives::Address;
use alloy_primitives::U256;
use bridge_config::common::eth::EthConfig;
use bridge_config::Config as BridgeConfig;
use bridge_service::chains::ethereum::types::AtomicBridgeCounterparty;
use bridge_service::chains::ethereum::types::AtomicBridgeInitiator;
use bridge_service::chains::ethereum::types::EthAddress;
use bridge_service::chains::ethereum::types::WETH9;
use bridge_service::chains::ethereum::utils::{send_transaction, send_transaction_rules};
use bridge_service::types::TimeLock;

pub async fn setup(mut config: BridgeConfig) -> Result<BridgeConfig, anyhow::Error> {
	//Setup Eth config
	setup_local_ethereum(&mut config.eth).await?;

	//let (movement_client, child) = MovementClient::new_for_test(MovementConfig::build_for_test()).await?;

	Ok(config)
}

pub async fn setup_local_ethereum(config: &mut EthConfig) -> Result<(), anyhow::Error> {
	let signer_private_key = config.signer_private_key.parse::<PrivateKeySigner>()?;
	let rpc_url = config.eth_rpc_connection_url();

	tracing::info!("Bridge deploy setup_local_ethereum");
	config.eth_initiator_contract =
		deploy_eth_initiator_contract(signer_private_key.clone(), &rpc_url)
			.await
			.to_string();
	tracing::info!("Bridge deploy after intiator");
	config.eth_counterparty_contract =
		deploy_counterpart_contract(signer_private_key.clone(), &rpc_url)
			.await
			.to_string();
	let eth_weth_contract = deploy_weth_contract(signer_private_key.clone(), &rpc_url).await;
	config.eth_weth_contract = eth_weth_contract.to_string();

	initialize_initiator_contract(
		signer_private_key.clone(),
		&rpc_url,
		&config.eth_initiator_contract,
		EthAddress(eth_weth_contract),
		EthAddress(signer_private_key.address()),
		*TimeLock(1),
		config.gas_limit,
		config.transaction_send_retries,
	)
	.await?;
	Ok(())
}

async fn deploy_eth_initiator_contract(
	signer_private_key: PrivateKeySigner,
	rpc_url: &str,
) -> Address {
	let rpc_provider = ProviderBuilder::new()
		.with_recommended_fillers()
		.wallet(EthereumWallet::from(signer_private_key.clone()))
		.on_builtin(rpc_url)
		.await
		.expect("Error during provider creation");

	let contract = AtomicBridgeInitiator::deploy(rpc_provider.clone())
		.await
		.expect("Failed to deploy AtomicBridgeInitiator");
	tracing::info!("initiator_contract address: {}", contract.address().to_string());
	contract.address().to_owned()
}

async fn deploy_counterpart_contract(
	signer_private_key: PrivateKeySigner,
	rpc_url: &str,
) -> Address {
	let rpc_provider = ProviderBuilder::new()
		.with_recommended_fillers()
		.wallet(EthereumWallet::from(signer_private_key))
		.on_builtin(rpc_url)
		.await
		.expect("Error during provider creation");
	let contract = AtomicBridgeCounterparty::deploy(rpc_provider.clone())
		.await
		.expect("Failed to deploy AtomicBridgeInitiator");
	tracing::info!("counterparty_contract address: {}", contract.address().to_string());
	contract.address().to_owned()
}

async fn deploy_weth_contract(signer_private_key: PrivateKeySigner, rpc_url: &str) -> Address {
	let rpc_provider = ProviderBuilder::new()
		.with_recommended_fillers()
		.wallet(EthereumWallet::from(signer_private_key.clone()))
		.on_builtin(rpc_url)
		.await
		.expect("Error during provider creation");
	let weth = WETH9::deploy(rpc_provider).await.expect("Failed to deploy WETH9");
	tracing::info!("weth_contract address: {}", weth.address().to_string());
	weth.address().to_owned()
}

async fn initialize_initiator_contract(
	signer_private_key: PrivateKeySigner,
	rpc_url: &str,
	initiator_contract_address: &str,
	weth: EthAddress,
	owner: EthAddress,
	timelock: u64,
	gas_limit: u64,
	transaction_send_retries: u32,
) -> Result<(), anyhow::Error> {
	let rpc_provider = ProviderBuilder::new()
		.with_recommended_fillers()
		.wallet(EthereumWallet::from(signer_private_key))
		.on_builtin(rpc_url)
		.await
		.expect("Error during provider creation");
	let initiator_contract =
		AtomicBridgeInitiator::new(initiator_contract_address.parse()?, rpc_provider);

	let call = initiator_contract.initialize(weth.0, owner.0, U256::from(timelock));
	send_transaction(call, &send_transaction_rules(), transaction_send_retries, gas_limit.into())
		.await
		.expect("Failed to send transaction");
	Ok(())
}
