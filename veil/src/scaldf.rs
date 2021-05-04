//! scaldf implements Veil's scalar derivation functions, which derive ristretto255 scalars from
//! other pieces of data.
//!
//! Scalars are generated as follows, given a protocol name `P` and datum `D`:
//!
//! ```text
//! INIT(P, level=256)
//! KEY(D)
//! PRF(64)
//! ```
//!
//! The two recognized protocol identifiers are: `veil.scaldf.label`, used to derive delta scalars
//! from labels; `veil.scaldf.root`, used to derive root scalars from secret keys.
//!

use curve25519_dalek::constants::RISTRETTO_BASEPOINT_POINT;
use curve25519_dalek::ristretto::RistrettoPoint;
use curve25519_dalek::scalar::Scalar;
use strobe_rs::{SecParam, Strobe};

pub(crate) fn derive_root(seed: &[u8; 64]) -> Scalar {
    let mut out = [0u8; 64];

    let mut root_df = Strobe::new(b"veil.scaldf.root", SecParam::B256);
    root_df.key(&seed.to_vec(), false);
    root_df.prf(&mut out, false);

    Scalar::from_bytes_mod_order_wide(&out)
}

pub(crate) fn derive_scalar(d: &Scalar, key_id: &str) -> Scalar {
    let mut seed = [0u8; 64];
    let mut d_p = *d;

    for label in key_id_parts(key_id) {
        let mut root_df = Strobe::new(b"veil.scaldf.label", SecParam::B256);
        root_df.key(label.as_bytes(), false);
        root_df.prf(&mut seed, false);

        let r = Scalar::from_bytes_mod_order_wide(&seed);

        d_p += &r;
    }

    d_p
}

pub(crate) fn derive_point(q: &RistrettoPoint, key_id: &str) -> RistrettoPoint {
    let r = RISTRETTO_BASEPOINT_POINT * derive_scalar(&Scalar::zero(), key_id);

    q + r
}

fn key_id_parts(key_id: &str) -> Vec<&str> {
    key_id
        .trim_matches(|s| s == '/')
        .split(|s| s == '/')
        .collect()
}

#[cfg(test)]
mod tests {
    use crate::scaldf::key_id_parts;

    #[test]
    pub fn key_id_splitting() {
        assert_eq!(vec!["one", "two", "three"], key_id_parts("/one/two/three"));
        assert_eq!(vec!["two", "three"], key_id_parts("two/three"));
    }
}
