#![warn(missing_docs)]

//! The Veil hybrid cryptosystem.
//!
//! Veil is an incredibly experimental hybrid cryptosystem for sending and receiving confidential,
//! authentic multi-recipient messages which are indistinguishable from random noise by an attacker.
//! Unlike e.g. GPG messages, Veil messages contain no metadata or format details which are not
//! encrypted. As a result, a global passive adversary would be unable to gain any information from
//! a Veil message beyond traffic analysis. Messages can be padded with random bytes to disguise
//! their true length, and fake recipients can be added to disguise their true number from other
//! recipients.
//!
//! You should not use this.
//!
//!
//! ```
//! use std::{io, str};
//! use veil::SecretKey;
//!
//! // Alice creates a secret key.
//! let alice_sk = SecretKey::new();
//!
//! // Bea creates a secret key.
//! let bea_sk = SecretKey::new();
//!
//! // Alice derives a private key for messaging with Bea and shares the corresponding public key.
//! let alice_priv = alice_sk.private_key("/friends/bea");
//! let alice_pub = alice_priv.public_key();
//!
//! // Bea derives a private key for messaging with Alice and shares the corresponding public key.
//! let bea_priv = bea_sk.private_key("/buddies/cool-ones/alice");
//! let bea_pub = bea_priv.public_key();
//!
//! // Alice encrypts a secret message for Bea.
//! let mut ciphertext = io::Cursor::new(Vec::new());
//! alice_priv.encrypt(
//!   &mut io::Cursor::new("this is a secret message"),
//!   &mut ciphertext,
//!   vec![bea_pub],
//!   20,
//!   1234,
//! ).expect("encryption failed");
//!
//! // Bea decrypts the message.
//! let mut plaintext = io::Cursor::new(Vec::new());
//! bea_priv.decrypt(
//!   &mut io::Cursor::new(ciphertext.into_inner()),
//!   &mut plaintext,
//!   &alice_pub,
//! ).expect("decryption failed");
//!
//! // Having decrypted the message, Bea can read the plaintext.
//! assert_eq!(
//!   "this is a secret message",
//!   str::from_utf8(&plaintext.into_inner()).expect("invalid UTF-8"),
//! );
//! ```

pub use self::veil::*;

pub mod akem;
pub mod mres;
pub mod pbenc;
pub mod scaldf;
pub mod schnorr;
mod util;
mod veil;

#[cfg(test)]
mod test_helpers {
    use curve25519_dalek::scalar::Scalar;

    pub fn rand_scalar() -> Scalar {
        let mut seed = [0u8; 64];
        getrandom::getrandom(&mut seed).expect("rng failure");

        Scalar::from_bytes_mod_order_wide(&seed)
    }
}