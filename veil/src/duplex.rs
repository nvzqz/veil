//! Implements a cryptographic duplex using Cyclist/Keccak.

use std::io;
use std::io::{Read, Write};

use crrl::jq255e::Scalar;
use cyclist::keccyak::{KeccyakMinHash, KeccyakMinKeyed};
use cyclist::Cyclist;
use rand::{CryptoRng, Rng};

use crate::blockio::ReadBlock;
use crate::keys::{PrivKey, SCALAR_LEN};

/// The length of an authentication tag in bytes.
pub const TAG_LEN: usize = KeccyakMinKeyed::tag_len();

/// An unkeyed cryptographic duplex.
#[derive(Clone)]
pub struct UnkeyedDuplex {
    state: KeccyakMinHash,
}

impl UnkeyedDuplex {
    /// Create a new [`UnkeyedDuplex`] with the given domain separation string.
    #[must_use]
    pub fn new(domain: &str) -> UnkeyedDuplex {
        // Initialize an empty hash.
        let mut state = KeccyakMinHash::default();

        // Absorb the domain separation string.
        state.absorb(domain.as_bytes());

        UnkeyedDuplex { state }
    }

    /// Extract a key from this duplex's state and use it to create a keyed duplex.
    #[must_use]
    pub fn into_keyed(mut self) -> KeyedDuplex {
        const KEY_LEN: usize = 64;

        let mut key = [0u8; KEY_LEN];
        self.state.squeeze_key_mut(&mut key);

        KeyedDuplex { state: KeccyakMinKeyed::new(&key, None, None) }
    }
}

/// A keyed cryptographic duplex.
#[derive(Clone)]
pub struct KeyedDuplex {
    state: KeccyakMinKeyed,
}

impl KeyedDuplex {
    /// Encrypt the given plaintext in place. **Provides no guarantees for authenticity.**
    pub fn encrypt_mut(&mut self, in_out: &mut [u8]) {
        self.state.encrypt_mut(in_out);
    }

    /// Decrypt the given ciphertext in place. **Provides no guarantees for authenticity.**
    pub fn decrypt_mut(&mut self, in_out: &mut [u8]) {
        self.state.decrypt_mut(in_out);
    }

    /// Encrypt and seal the given plaintext in place. Requires [`TAG_LEN`] extra bytes on the end
    /// of `in_out`.
    /// **Guarantees authenticity.**
    pub fn seal_mut(&mut self, in_out: &mut [u8]) {
        self.state.seal_mut(in_out);
    }

    /// Decrypt and unseal the given ciphertext in place. If the ciphertext is valid, returns the
    /// length of the plaintext; if invalid, returns `None`.
    /// **Guarantees authenticity.**
    #[must_use]
    pub fn unseal_mut<'a>(&mut self, in_out: &'a mut [u8]) -> Option<&'a [u8]> {
        self.state.open_mut(in_out).then_some(&in_out[..in_out.len() - TAG_LEN])
    }
}

/// Common duplex output operations.
pub trait Squeeze {
    /// Fill the given output slice with bytes squeezed from the duplex.
    fn squeeze_mut(&mut self, out: &mut [u8]);

    /// Squeeze `n` bytes from the duplex.
    #[must_use]
    fn squeeze<const N: usize>(&mut self) -> [u8; N] {
        let mut b = [0u8; N];
        self.squeeze_mut(&mut b);
        b
    }

    /// Squeeze 32 bytes from the duplex and map them to a [`Scalar`].
    #[must_use]
    fn squeeze_scalar(&mut self) -> Scalar {
        Scalar::decode_reduce(&self.squeeze::<SCALAR_LEN>())
    }

    /// Squeeze 32 bytes from the duplex and map them to a [`PrivKey`].
    #[must_use]
    fn squeeze_private_key(&mut self) -> PrivKey {
        PrivKey::decode_reduce(self.squeeze::<SCALAR_LEN>())
            .expect("unexpected invalid private key")
    }
}

impl Squeeze for UnkeyedDuplex {
    fn squeeze_mut(&mut self, out: &mut [u8]) {
        self.state.squeeze_mut(out);
    }
}

impl Squeeze for KeyedDuplex {
    fn squeeze_mut(&mut self, out: &mut [u8]) {
        self.state.squeeze_mut(out);
    }
}

// Common duplex input operations.
pub trait Absorb<const BUFFER_LEN: usize>: Clone {
    /// Absorb the given slice of data.
    fn absorb(&mut self, data: &[u8]);

    /// Extend a previous absorb operation with the given slice of data.
    fn absorb_more(&mut self, data: &[u8]);

    /// Absorb the entire contents of the given reader as a single operation.
    fn absorb_reader(&mut self, reader: impl Read) -> io::Result<u64> {
        self.absorb_reader_into(reader, io::sink())
    }

    /// Copy the contents of `reader` into `writer`, absorbing the contents as a single operation.
    fn absorb_reader_into(
        &mut self,
        mut reader: impl Read,
        mut writer: impl Write,
    ) -> io::Result<u64> {
        let mut buf = [0u8; BUFFER_LEN];
        let mut first = true;
        let mut written = 0;

        loop {
            // Read a block of data.
            let n = reader.read_block(&mut buf)?;
            let block = &buf[..n];

            // Absorb the block.
            if first {
                self.absorb(block);
                first = false;
            } else {
                self.absorb_more(block);
            }

            // Write the block.
            writer.write_all(block)?;
            written += u64::try_from(n).expect("unexpected overflow");

            // If the block was undersized, we're at the end of the reader.
            if n < buf.len() {
                break;
            }
        }

        Ok(written)
    }

    /// Clone the duplex and use it to absorb the given secret and 64 random bytes. Pass the clone
    /// to the given function and return the result of that function as a secret.
    #[must_use]
    fn hedge<R>(
        &self,
        mut rng: impl Rng + CryptoRng,
        secret: &PrivKey,
        f: impl Fn(&mut Self) -> R,
    ) -> R {
        // Clone the duplex's state.
        let mut clone = self.clone();

        // Absorb the given secret.
        clone.absorb(&secret.d.encode());

        // Absorb a random value.
        clone.absorb(&rng.gen::<[u8; 64]>());

        // Call the given function with the clone.
        f(&mut clone)
    }
}

impl Absorb<{ KeccyakMinHash::absorb_rate() * 32 }> for UnkeyedDuplex {
    fn absorb(&mut self, data: &[u8]) {
        self.state.absorb(data);
    }

    fn absorb_more(&mut self, data: &[u8]) {
        self.state.absorb_more(data);
    }
}

impl Absorb<{ KeccyakMinKeyed::absorb_rate() * 32 }> for KeyedDuplex {
    fn absorb(&mut self, data: &[u8]) {
        self.state.absorb(data);
    }

    fn absorb_more(&mut self, data: &[u8]) {
        self.state.absorb_more(data);
    }
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use rand::{RngCore, SeedableRng};
    use rand_chacha::ChaChaRng;

    use super::*;

    #[test]
    fn ind_cpa_round_trip() {
        let mut rng = ChaChaRng::seed_from_u64(0xDEADBEEF);
        let plaintext = rng.gen::<[u8; 64]>();
        let key = rng.gen::<[u8; 64]>();
        let associated_data = rng.gen::<[u8; 64]>();

        let mut unkeyed = UnkeyedDuplex::new("test");
        unkeyed.absorb(&key);
        unkeyed.absorb(&associated_data);

        let mut keyed = unkeyed.into_keyed();
        let mut ciphertext = plaintext.to_vec();
        keyed.encrypt_mut(&mut ciphertext);

        let mut unkeyed = UnkeyedDuplex::new("test");
        unkeyed.absorb(&key);
        unkeyed.absorb(&associated_data);

        let mut keyed = unkeyed.into_keyed();
        keyed.decrypt_mut(&mut ciphertext);
        assert_eq!(plaintext.to_vec(), ciphertext);
    }

    #[test]
    fn ind_cca_round_trip() {
        let mut rng = ChaChaRng::seed_from_u64(0xDEADBEEF);
        let plaintext = rng.gen::<[u8; 64]>();

        let mut duplex = UnkeyedDuplex::new("test").into_keyed();
        let mut ciphertext = vec![0u8; plaintext.len() + TAG_LEN];
        ciphertext[..plaintext.len()].copy_from_slice(&plaintext);
        duplex.seal_mut(&mut ciphertext);

        let mut duplex = UnkeyedDuplex::new("test").into_keyed();
        assert_eq!(Some(plaintext.as_slice()), duplex.unseal_mut(&mut ciphertext));
    }

    #[test]
    fn absorb_blocks() {
        let mut rng = ChaChaRng::seed_from_u64(0xDEADBEEF);
        let mut message = vec![0u8; KeccyakMinHash::absorb_rate() * 3 + 8];
        rng.fill_bytes(&mut message);

        let mut one = UnkeyedDuplex::new("ok");
        one.absorb_reader(Cursor::new(&message)).expect("error absorbing");

        let mut two = UnkeyedDuplex::new("ok");
        two.absorb_reader(Cursor::new(&message)).expect("error absorbing");

        assert_eq!(one.squeeze::<4>(), two.squeeze::<4>());
    }
}