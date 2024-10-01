use ecdsa::{
	elliptic_curve::{
		generic_array::ArrayLength,
		ops::Invert,
		point::PointCompression,
		sec1::{FromEncodedPoint, ModulusSize, ToEncodedPoint},
		subtle::CtOption,
		AffinePoint, CurveArithmetic, FieldBytesSize, PrimeCurve, Scalar,
	},
	hazmat::{DigestPrimitive, SignPrimitive, VerifyPrimitive},
	signature::{digest::Digest, DigestVerifier},
	SignatureSize, SigningKey, VerifyingKey,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InnerSignedBlobV1Data {
	pub blob: Vec<u8>,
	pub timestamp: u64,
}

impl InnerSignedBlobV1Data {
	pub fn new(blob: Vec<u8>, timestamp: u64) -> Self {
		Self { blob, timestamp }
	}

	pub fn try_to_sign<C>(
		self,
		signing_key: &SigningKey<C>,
	) -> Result<InnerSignedBlobV1, anyhow::Error>
	where
		C: PrimeCurve + CurveArithmetic + DigestPrimitive + PointCompression,
		Scalar<C>: Invert<Output = CtOption<Scalar<C>>> + SignPrimitive<C>,
		SignatureSize<C>: ArrayLength<u8>,
		AffinePoint<C>: FromEncodedPoint<C> + ToEncodedPoint<C> + VerifyPrimitive<C>,
		FieldBytesSize<C>: ModulusSize,
	{
		let mut hasher = C::Digest::new();
		hasher.update(self.blob.as_slice());
		hasher.update(&self.timestamp.to_be_bytes());
		let prehash = hasher.finalize();
		let prehash_bytes = prehash.as_slice();

		let (signature, _recovery_id) = signing_key.sign_prehash_recoverable(prehash_bytes)?;

		Ok(InnerSignedBlobV1 {
			data: self,
			signature: signature.to_vec(),
			signer: signing_key.verifying_key().to_sec1_bytes().to_vec(),
			id: prehash_bytes.to_vec(),
		})
	}

	pub fn try_verify<C>(&self, signature: &[u8], signer: &[u8]) -> Result<(), anyhow::Error>
	where
		C: PrimeCurve + CurveArithmetic + DigestPrimitive + PointCompression,
		Scalar<C>: Invert<Output = CtOption<Scalar<C>>> + SignPrimitive<C>,
		SignatureSize<C>: ArrayLength<u8>,
		AffinePoint<C>: FromEncodedPoint<C> + ToEncodedPoint<C> + VerifyPrimitive<C>,
		FieldBytesSize<C>: ModulusSize,
	{
		let mut hasher = C::Digest::new();
		hasher.update(self.blob.as_slice());
		hasher.update(&self.timestamp.to_be_bytes());

		let verifying_key = VerifyingKey::<C>::from_sec1_bytes(signer)?;
		let signature = ecdsa::Signature::from_bytes(signature.into())?;

		match verifying_key.verify_digest(hasher, &signature) {
			Ok(_) => Ok(()),
			Err(_) => Err(anyhow::anyhow!("Failed to verify signature")),
		}
	}
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InnerSignedBlobV1 {
	pub data: InnerSignedBlobV1Data,
	pub signature: Vec<u8>,
	pub signer: Vec<u8>,
	pub id: Vec<u8>,
}

impl InnerSignedBlobV1 {
	pub fn try_verify<C>(&self) -> Result<(), anyhow::Error>
	where
		C: PrimeCurve + CurveArithmetic + DigestPrimitive + PointCompression,
		Scalar<C>: Invert<Output = CtOption<Scalar<C>>> + SignPrimitive<C>,
		SignatureSize<C>: ArrayLength<u8>,
		AffinePoint<C>: FromEncodedPoint<C> + ToEncodedPoint<C> + VerifyPrimitive<C>,
		FieldBytesSize<C>: ModulusSize,
	{
		self.data.try_verify::<C>(self.signature.as_slice(), self.signer.as_slice())
	}
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum InnerBlob {
	SignedV1(InnerSignedBlobV1),
}

impl From<InnerSignedBlobV1> for InnerBlob {
	fn from(inner: InnerSignedBlobV1) -> Self {
		InnerBlob::SignedV1(inner)
	}
}

impl InnerBlob {
	pub fn blob(&self) -> &[u8] {
		match self {
			InnerBlob::SignedV1(inner) => inner.data.blob.as_slice(),
		}
	}

	pub fn signature(&self) -> &[u8] {
		match self {
			InnerBlob::SignedV1(inner) => inner.signature.as_slice(),
		}
	}

	pub fn timestamp(&self) -> u64 {
		match self {
			InnerBlob::SignedV1(inner) => inner.data.timestamp,
		}
	}

	pub fn signer(&self) -> &[u8] {
		match self {
			InnerBlob::SignedV1(inner) => inner.signer.as_slice(),
		}
	}

	pub fn signer_hex(&self) -> String {
		hex::encode(self.signer())
	}

	pub fn id(&self) -> &[u8] {
		match self {
			InnerBlob::SignedV1(inner) => inner.id.as_slice(),
		}
	}

	pub fn verify_signature<C>(&self) -> Result<(), anyhow::Error>
	where
		C: PrimeCurve + CurveArithmetic + DigestPrimitive + PointCompression,
		Scalar<C>: Invert<Output = CtOption<Scalar<C>>> + SignPrimitive<C>,
		SignatureSize<C>: ArrayLength<u8>,
		AffinePoint<C>: FromEncodedPoint<C> + ToEncodedPoint<C> + VerifyPrimitive<C>,
		FieldBytesSize<C>: ModulusSize,
	{
		match self {
			InnerBlob::SignedV1(inner) => inner.try_verify::<C>(),
		}
	}
}

pub mod celestia {

	use celestia_types::{nmt::Namespace, Blob as CelestiaBlob};

	use super::InnerBlob;

	impl TryFrom<CelestiaBlob> for InnerBlob {
		type Error = anyhow::Error;

		// todo: it would be nice to have this be self describing over the compression and serialization format
		fn try_from(blob: CelestiaBlob) -> Result<Self, Self::Error> {
			// decompress blob.data with zstd
			let decompressed = zstd::decode_all(blob.data.as_slice())?;

			// deserialize the decompressed with bcs
			// todo: because this is a simple data structure, bcs might not be the best format
			let blob = bcs::from_bytes(decompressed.as_slice())?;

			Ok(blob)
		}
	}

	pub struct CelestiaInnerBlob(pub InnerBlob, pub Namespace);

	impl TryFrom<CelestiaInnerBlob> for CelestiaBlob {
		type Error = anyhow::Error;

		fn try_from(inner_blob: CelestiaInnerBlob) -> Result<Self, Self::Error> {
			// Extract the inner blob and namespace
			let CelestiaInnerBlob(inner_blob, namespace) = inner_blob;

			// Serialize the inner blob with bcs
			let serialized_blob = bcs::to_bytes(&inner_blob)?;

			// Compress the serialized data with zstd
			let compressed_blob = zstd::encode_all(serialized_blob.as_slice(), 0)?;

			// Construct the final CelestiaBlob by assigning the compressed data
			// and associating it with the provided namespace
			Ok(CelestiaBlob::new(namespace, compressed_blob).map_err(|e| anyhow::anyhow!(e))?)
		}
	}
}
