use crate::{Error, Verified, VerifierOperations};
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
	SignatureSize,
};
use m1_da_light_node_util::inner_blob::InnerBlob;
use std::collections::HashSet;

#[derive(Clone)]
pub struct Verifier<C>
where
	C: PrimeCurve + CurveArithmetic + DigestPrimitive + PointCompression,
	Scalar<C>: Invert<Output = CtOption<Scalar<C>>> + SignPrimitive<C>,
	SignatureSize<C>: ArrayLength<u8>,
	AffinePoint<C>: FromEncodedPoint<C> + ToEncodedPoint<C> + VerifyPrimitive<C>,
	FieldBytesSize<C>: ModulusSize,
{
	pub _curve_marker: std::marker::PhantomData<C>,
}

impl<C> Verifier<C>
where
	C: PrimeCurve + CurveArithmetic + DigestPrimitive + PointCompression,
	Scalar<C>: Invert<Output = CtOption<Scalar<C>>> + SignPrimitive<C>,
	SignatureSize<C>: ArrayLength<u8>,
	AffinePoint<C>: FromEncodedPoint<C> + ToEncodedPoint<C> + VerifyPrimitive<C>,
	FieldBytesSize<C>: ModulusSize,
{
	pub fn new() -> Self {
		Self { _curve_marker: std::marker::PhantomData }
	}
}

#[tonic::async_trait]
impl<C> VerifierOperations<InnerBlob, InnerBlob> for Verifier<C>
where
	C: PrimeCurve + CurveArithmetic + DigestPrimitive + PointCompression,
	Scalar<C>: Invert<Output = CtOption<Scalar<C>>> + SignPrimitive<C>,
	SignatureSize<C>: ArrayLength<u8>,
	AffinePoint<C>: FromEncodedPoint<C> + ToEncodedPoint<C> + VerifyPrimitive<C>,
	FieldBytesSize<C>: ModulusSize,
{
	async fn verify(&self, blob: InnerBlob, _height: u64) -> Result<Verified<InnerBlob>, Error> {
		blob.verify_signature::<C>().map_err(|e| Error::Validation(e.to_string()))?;

		Ok(Verified::new(blob))
	}
}

/// Verifies that the signer of the inner blob is in the known signers set.
/// This is built around an inner signer because we should always check the signature first. That is, this composition prevents unsafe usage.
#[derive(Clone)]
pub struct InKnownSignersVerifier<C>
where
	C: PrimeCurve + CurveArithmetic + DigestPrimitive + PointCompression,
	Scalar<C>: Invert<Output = CtOption<Scalar<C>>> + SignPrimitive<C>,
	SignatureSize<C>: ArrayLength<u8>,
	AffinePoint<C>: FromEncodedPoint<C> + ToEncodedPoint<C> + VerifyPrimitive<C>,
	FieldBytesSize<C>: ModulusSize,
{
	pub inner_verifier: Verifier<C>,
	/// The set of known signers in sec1 bytes hex format.
	pub known_signers_sec1_bytes_hex: HashSet<String>,
}

impl<C> InKnownSignersVerifier<C>
where
	C: PrimeCurve + CurveArithmetic + DigestPrimitive + PointCompression,
	Scalar<C>: Invert<Output = CtOption<Scalar<C>>> + SignPrimitive<C>,
	SignatureSize<C>: ArrayLength<u8>,
	AffinePoint<C>: FromEncodedPoint<C> + ToEncodedPoint<C> + VerifyPrimitive<C>,
	FieldBytesSize<C>: ModulusSize,
{
	pub fn new(known_signers_sec1_bytes_hex: HashSet<String>) -> Self {
		Self { inner_verifier: Verifier::new(), known_signers_sec1_bytes_hex }
	}
}

#[tonic::async_trait]
impl<C> VerifierOperations<InnerBlob, InnerBlob> for InKnownSignersVerifier<C>
where
	C: PrimeCurve + CurveArithmetic + DigestPrimitive + PointCompression,
	Scalar<C>: Invert<Output = CtOption<Scalar<C>>> + SignPrimitive<C>,
	SignatureSize<C>: ArrayLength<u8>,
	AffinePoint<C>: FromEncodedPoint<C> + ToEncodedPoint<C> + VerifyPrimitive<C>,
	FieldBytesSize<C>: ModulusSize,
{
	async fn verify(&self, blob: InnerBlob, height: u64) -> Result<Verified<InnerBlob>, Error> {
		let inner_blob = self.inner_verifier.verify(blob, height).await?;

		let signer = inner_blob.inner().signer_hex();
		if !self.known_signers_sec1_bytes_hex.contains(&signer) {
			return Err(Error::Validation("signer not in known signers".to_string()));
		}

		Ok(inner_blob)
	}
}
