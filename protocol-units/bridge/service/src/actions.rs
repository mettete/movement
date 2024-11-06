use crate::chains::bridge_contracts::{BridgeContract, BridgeContractError};
use crate::chains::movement::utils as movement_utils;
use crate::types::{Amount, BridgeAddress, BridgeTransferId, HashLock, HashLockPreImage};
use crate::ChainId;
use std::fmt;
use std::future::Future;
use std::pin::Pin;
use thiserror::Error;

#[derive(Error, Debug, Clone)]
pub struct ActionExecError(TransferAction, BridgeContractError);

impl ActionExecError {
	pub fn inner(self) -> (TransferAction, BridgeContractError) {
		(self.0, self.1)
	}
}

impl fmt::Display for ActionExecError {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		write!(f, "Action: {}/ Error: {}", self.0, self.1,)
	}
}

#[derive(Debug, Clone)]
pub struct TransferAction {
	pub chain: ChainId,
	pub transfer_id: BridgeTransferId,
	pub kind: TransferActionType,
}
impl fmt::Display for TransferAction {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		write!(f, "Action: {}/{}/{}", self.chain, self.transfer_id, self.kind)
	}
}

#[derive(Debug, Clone)]
pub enum TransferActionType {
	LockBridgeTransfer {
		bridge_transfer_id: BridgeTransferId,
		hash_lock: HashLock,
		initiator: BridgeAddress<Vec<u8>>,
		recipient: BridgeAddress<Vec<u8>>,
		amount: Amount,
	},
	WaitAndCompleteInitiator(u64, HashLockPreImage),
	RefundInitiator,
	TransferDone,
	NoAction,
}

impl fmt::Display for TransferActionType {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		let act = match self {
			TransferActionType::LockBridgeTransfer { .. } => "LockBridgeTransfer",
			TransferActionType::WaitAndCompleteInitiator(..) => "WaitAndCompleteInitiator",
			TransferActionType::RefundInitiator => "RefundInitiator",
			TransferActionType::TransferDone => "TransferDone",
			TransferActionType::NoAction => "NoAction",
		};
		write!(f, "{}", act)
	}
}

pub fn process_action<A>(
	action: TransferAction,
	mut client: impl BridgeContract<A> + 'static,
) -> Option<Pin<Box<dyn Future<Output = Result<(), ActionExecError>> + Send>>>
where
	A: Clone + Send + TryFrom<Vec<u8>>,
{
	tracing::info!("Action: creating execution for action:{action}");
	match action.kind.clone() {
		TransferActionType::LockBridgeTransfer {
			bridge_transfer_id,
			hash_lock,
			initiator,
			recipient,
			amount,
		} => {
			let future = async move {
				if recipient.0.len() == 32 {
					if let Err(e) = movement_utils::fund_recipient(&recipient).await {
						return Err(ActionExecError(action.clone(), e));
					}
				}

				client
					.lock_bridge_transfer(
						bridge_transfer_id,
						hash_lock,
						initiator,
						BridgeAddress(recipient.0.try_into().map_err(|_| {
							ActionExecError(
								action.clone(),
								BridgeContractError::BadAddressEncoding("lock bridge traénsfer fail to convert recipient address to vec<u8>".to_string()),
							)
						})?),
						amount,
					)
					.await
					.map_err(|err| ActionExecError(action, err))
			};
			Some(Box::pin(future))
		}
		TransferActionType::WaitAndCompleteInitiator(wait_time_sec, secret) => {
			let future = async move {
				if wait_time_sec != 0 {
					let _ = tokio::time::sleep(tokio::time::Duration::from_secs(wait_time_sec));
				}
				client
					.initiator_complete_bridge_transfer(action.transfer_id, secret)
					.await
					.map_err(|err| ActionExecError(action, err))
			};
			Some(Box::pin(future))
		}
		TransferActionType::RefundInitiator => None,
		TransferActionType::TransferDone => None,
		TransferActionType::NoAction => None,
	}
}
