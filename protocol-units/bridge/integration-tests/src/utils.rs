#![allow(dead_code)]
use crate::HarnessMvtClient;
use alloy::hex;
use anyhow::Result;
use aptos_sdk::{
	coin_client::CoinClient, rest_client::Transaction, types::account_address::AccountAddress,
};
use bridge_service::chains::bridge_contracts::{BridgeContract, BridgeContractError};
use bridge_service::chains::movement::client::MovementClient;
use bridge_service::chains::movement::utils::{
	self as movement_utils, MovementAddress, MovementHash,
};
use bridge_service::chains::bridge_contracts::BridgeContractResult;
use bridge_service::types::{Amount, AssetType, BridgeAddress, BridgeTransferId, BridgeTransferDetails, HashLock, TimeLock};
use tracing::debug;

pub fn assert_bridge_transfer_details(
	details: &BridgeTransferDetails<MovementAddress>, // MovementAddress for initiator
	expected_bridge_transfer_id: [u8; 32],
	expected_hash_lock: [u8; 32],
	expected_sender_address: AccountAddress,
	expected_recipient_address: Vec<u8>,
	expected_amount: u64,
	expected_state: u8,
) {
	assert_eq!(details.bridge_transfer_id.0, expected_bridge_transfer_id);
	assert_eq!(details.hash_lock.0, expected_hash_lock);
	assert_eq!(details.initiator_address.0 .0, expected_sender_address);
	assert_eq!(details.recipient_address.0, expected_recipient_address);
	assert_eq!(details.amount.0, AssetType::Moveth(expected_amount));
	assert_eq!(details.state, expected_state, "Bridge transfer state mismatch.");
}

pub async fn extract_bridge_transfer_details(
	movement_client: &mut MovementClient,
) -> BridgeContractResult<Option<BridgeTransferDetails<MovementAddress>>> {
	let sender_address = movement_client.signer().address();
	let sequence_number = 0; // Modify as needed
	let rest_client = movement_client.rest_client();

	let transactions = rest_client
		.get_account_transactions(sender_address, Some(sequence_number), Some(20))
		.await
		.map_err(|e| BridgeContractError::CallError)?;

	// Loop through the transactions to find the one with the event we need
	if let Some(transaction) = transactions.into_inner().last() {
		if let Transaction::UserTransaction(user_txn) = transaction {
			for event in &user_txn.events {
				if let aptos_sdk::rest_client::aptos_api_types::MoveType::Struct(struct_tag) =
					&event.typ
				{
					match struct_tag.name.as_str() {
						"BridgeTransferInitiatedEvent" | "BridgeTransferLockedEvent" => {
							// Extract the bridge_transfer_id from the event data
							let bridge_transfer_id = event
								.data
								.get("bridge_transfer_id")
								.and_then(|v| v.as_str())
								.ok_or(BridgeContractError::EventNotFound)?;

							let recipient = event
								.data
								.get("recipient")
								.and_then(|v| v.as_str())
								.ok_or(BridgeContractError::EventNotFound)?;

							let amount = event
								.data
								.get("amount")
								.and_then(|v| v.as_u64())
								.ok_or(BridgeContractError::EventNotFound)?;

							let hash_lock = event
								.data
								.get("hash_lock")
								.and_then(|v| v.as_str())
								.ok_or(BridgeContractError::EventNotFound)?;

							let time_lock = event
								.data
								.get("time_lock")
								.and_then(|v| v.as_u64())
								.ok_or(BridgeContractError::EventNotFound)?;

							// Decode and convert the event values into their expected types
							let decoded_bridge_transfer_id: [u8; 32] = hex::decode(bridge_transfer_id.trim_start_matches("0x"))
								.map_err(|_| BridgeContractError::SerializationError)?
								.try_into()
								.map_err(|_| BridgeContractError::SerializationError)?;

							let decoded_recipient = hex::decode(recipient.trim_start_matches("0x"))
								.map_err(|_| BridgeContractError::SerializationError)?;

							let decoded_hash_lock: [u8; 32] = hex::decode(hash_lock.trim_start_matches("0x"))
								.map_err(|_| BridgeContractError::SerializationError)?
								.try_into()
								.map_err(|_| BridgeContractError::SerializationError)?;

							// Convert the sender (initiator) address to `AccountAddress`
							let originator_address = AccountAddress::from_hex_literal(&sender_address.to_string())
								.map_err(|_| BridgeContractError::SerializationError)?;

							// Construct the `BridgeTransferDetails` struct
							let details = BridgeTransferDetails {
								bridge_transfer_id: BridgeTransferId(decoded_bridge_transfer_id),
								initiator_address: BridgeAddress(MovementAddress(originator_address)),
								recipient_address: BridgeAddress(decoded_recipient),
								amount: Amount(AssetType::Moveth(amount)),
								hash_lock: HashLock(decoded_hash_lock),
								time_lock: TimeLock(time_lock),
								state: 1, // Default state, can be adjusted
							};

							return Ok(Some(details));
						}
						_ => {}
					}
				}
			}
		}
	}

	Err(BridgeContractError::EventNotFound)
}

pub async fn extract_bridge_transfer_id(
	movement_client: &mut MovementClient,
) -> Result<[u8; 32], anyhow::Error> {
	let sender_address = movement_client.signer().address();
	let sequence_number = 0; // Modify as needed
	let rest_client = movement_client.rest_client();

	let transactions = rest_client
		.get_account_transactions(sender_address, Some(sequence_number), Some(20))
		.await
		.map_err(|e| anyhow::Error::msg(format!("Failed to get transactions: {:?}", e)))?;

	if let Some(transaction) = transactions.into_inner().last() {
		if let Transaction::UserTransaction(user_txn) = transaction {
			for event in &user_txn.events {
				if let aptos_sdk::rest_client::aptos_api_types::MoveType::Struct(struct_tag) =
					&event.typ
				{
					match struct_tag.name.as_str() {
						"BridgeTransferInitiatedEvent" | "BridgeTransferLockedEvent" => {
							if let Some(bridge_transfer_id) =
								event.data.get("bridge_transfer_id").and_then(|v| v.as_str())
							{
								let hex_str = bridge_transfer_id.trim_start_matches("0x");
								let decoded_vec = hex::decode(hex_str).map_err(|_| {
									anyhow::Error::msg("Failed to decode hex string into Vec<u8>")
								})?;
								return decoded_vec.try_into().map_err(|_| {
									anyhow::Error::msg(
										"Failed to convert decoded Vec<u8> to [u8; 32]",
									)
								});
							}
						}
						_ => {}
					}
				}
			}
		}
	}
	Err(anyhow::Error::msg("No matching transaction found"))
}

pub async fn fund_and_check_balance(
	movement_harness: &mut HarnessMvtClient,
	expected_balance: u64,
) -> Result<()> {
	let movement_client_signer = movement_harness.movement_client.signer();
	let rest_client = movement_harness.rest_client.clone();
	let coin_client = CoinClient::new(&rest_client);
	let faucet_client = movement_harness.faucet_client.write().unwrap();
	faucet_client.fund(movement_client_signer.address(), expected_balance).await?;

	let balance = coin_client.get_account_balance(&movement_client_signer.address()).await?;
	assert!(
		balance >= expected_balance,
		"Expected Movement Client to have at least {}, but found {}",
		expected_balance,
		balance
	);

	Ok(())
}

pub async fn publish_for_test(movement_client: &mut MovementClient) {
	let _ = movement_client.publish_for_test();
}

pub async fn initiate_bridge_transfer_helper(
	movement_client: &mut MovementClient,
	initiator_address: AccountAddress,
	recipient_address: Vec<u8>,
	hash_lock: [u8; 32],
	amount: u64,
	timelock_modify: bool,
	framework: bool
) -> Result<(), BridgeContractError> {
	// Publish for test
	//let _ = movement_client.publish_for_test();

	if timelock_modify {
		// Set the timelock to 1 second for testing
		movement_client.initiator_set_timelock(1, false).await.expect("Failed to set timelock");
	}

	// Mint MovETH to the initiator's address
	let mint_amount = 200 * 100_000_000; // Assuming 8 decimals for MovETH

	let mint_args = vec![
		movement_utils::serialize_address_initiator(&movement_client.signer().address())?, // Mint to initiator's address
		movement_utils::serialize_u64_initiator(&mint_amount)?, // Amount to mint (200 MovETH)
	];

	let mint_payload = movement_utils::make_aptos_payload(
		movement_client.native_address, // Address where moveth module is published
		"moveth",
		"mint",
		Vec::new(),
		mint_args,
	);

	// Send transaction to mint MovETH
	movement_utils::send_and_confirm_aptos_transaction(
		&movement_client.rest_client(),
		movement_client.signer(),
		mint_payload,
	)
	.await
	.map_err(|_| BridgeContractError::MintError)?;

	debug!("Successfully minted 200 MovETH to the initiator");

	// Initiate the bridge transfer
	movement_client
		.initiate_bridge_transfer(
			BridgeAddress(MovementAddress(initiator_address)),
			BridgeAddress(recipient_address),
			HashLock(MovementHash(hash_lock).0),
			Amount(AssetType::Moveth(amount)),
			framework
		)
		.await
		.expect("Failed to initiate bridge transfer");

	Ok(())
}
