use alloy::network::{Ethereum, EthereumWallet};
use alloy::primitives::Address;
use alloy::providers::fillers::{
	ChainIdFiller, FillProvider, GasFiller, JoinFill, NonceFiller, WalletFiller,
};
use alloy::providers::RootProvider;
use alloy::rlp::{RlpDecodable, RlpEncodable};
use alloy::transports::BoxTransport;
use serde::{Deserialize, Serialize};

use crate::AtomicBridgeInitiator::AtomicBridgeInitiatorInstance;

// Codegen from the abis
alloy::sol!(
	#[allow(missing_docs)]
	#[sol(rpc)]
	AtomicBridgeInitiator,
	"abis/AtomicBridgeInitiator.json"
);

alloy::sol!(
	#[allow(missing_docs)]
	#[sol(rpc)]
	AtomicBridgeCounterparty,
	"abis/AtomicBridgeCounterparty.json"
);

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub struct EthHash(pub [u8; 32]);

impl From<AtomicBridgeInitiator::initiateBridgeTransferReturn> for EthHash {
	fn from(id: AtomicBridgeInitiator::initiateBridgeTransferReturn) -> Self {
		let mut bytes = [0u8; 32];
		bytes.copy_from_slice(id.bridgeTransferId.as_slice());
		EthHash(bytes)
	}
}

impl EthHash {
	pub fn inner(&self) -> &[u8; 32] {
		&self.0
	}
}

pub type InitiatorContract = AtomicBridgeInitiatorInstance<BoxTransport, AlloyProvider>;
pub type CounterpartyContract = AtomicBridgeInitiatorInstance<BoxTransport, AlloyProvider>;

pub type AlloyProvider = FillProvider<
	JoinFill<
		JoinFill<
			JoinFill<JoinFill<alloy::providers::Identity, GasFiller>, NonceFiller>,
			ChainIdFiller,
		>,
		WalletFiller<EthereumWallet>,
	>,
	RootProvider<BoxTransport>,
	BoxTransport,
	Ethereum,
>;

#[derive(Debug, PartialEq, Eq, Hash, Clone, RlpEncodable, RlpDecodable, Serialize, Deserialize)]
pub struct EthAddress(pub Address);

impl std::ops::Deref for EthAddress {
	type Target = Address;

	fn deref(&self) -> &Self::Target {
		&self.0
	}
}

impl From<String> for EthAddress {
	fn from(s: String) -> Self {
		EthAddress(Address::parse_checksummed(s, None).expect("Invalid Ethereum address"))
	}
}

impl From<Vec<u8>> for EthAddress {
	fn from(vec: Vec<u8>) -> Self {
		// Ensure the vector has the correct length
		assert_eq!(vec.len(), 20);

		let mut bytes = [0u8; 20];
		bytes.copy_from_slice(&vec);
		EthAddress(Address(bytes.into()))
	}
}

impl From<[u8; 32]> for EthAddress {
	fn from(bytes: [u8; 32]) -> Self {
		let mut address_bytes = [0u8; 20];
		address_bytes.copy_from_slice(&bytes[0..20]);
		EthAddress(Address(address_bytes.into()))
	}
}