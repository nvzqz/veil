use elliptic_curve::sec1::{EncodedPoint, FromEncodedPoint};
use elliptic_curve::Group;
use p256::NistP256;

/// The length of an encoded point in bytes.
pub(crate) const POINT_LEN: usize = 33;

/// The length of an encoded scalar in bytes.
pub(crate) const SCALAR_LEN: usize = 32;

/// A scalar value for the elliptic curve. Never zero.
pub(crate) type Scalar = elliptic_curve::NonZeroScalar<NistP256>;

/// A point on the elliptic curve. Never the additive identity.
pub(crate) type Point = elliptic_curve::ProjectivePoint<NistP256>;

/// Decodes the given byte slice as a point. Returns `None` if the point is invalid or infinity.
pub(crate) fn decode_point(b: impl AsRef<[u8]>) -> Option<Point> {
    let p = EncodedPoint::<NistP256>::from_bytes(b).ok()?;
    Option::<Point>::from(Point::from_encoded_point(&p)).filter(|p| (!p.is_identity()).into())
}

/// Decodes the given byte slice as a scalar. Returns `None` if the scalar is invalid or zero.
pub(crate) fn decode_scalar(b: impl AsRef<[u8]>) -> Option<Scalar> {
    let b: [u8; SCALAR_LEN] = b.as_ref().try_into().ok()?;
    Scalar::from_repr(b.into()).into()
}
