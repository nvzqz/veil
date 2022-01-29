use std::convert::TryInto;

use crypto_bigint::{Encoding, NonZero, U512};
use rand::RngCore;
use unicode_normalization::UnicodeNormalization;

use crate::constants::{MAC_LEN, U32_LEN, U64_LEN, USIZE_LEN};
use crate::strobe::Protocol;

/// The number of bytes encryption adds to a plaintext.
pub const OVERHEAD: usize = U32_LEN + U32_LEN + SALT_LEN + MAC_LEN;

/// Encrypt the given plaintext using the given passphrase.
#[must_use]
pub fn encrypt(passphrase: &str, time: u32, space: u32, plaintext: &[u8]) -> Vec<u8> {
    // Generate a random salt.
    let mut salt = [0u8; SALT_LEN];
    rand::thread_rng().fill_bytes(&mut salt);

    // Perform the balloon hashing.
    let mut pbenc = init(passphrase, &salt, time, space);

    // Allocate an output buffer.
    let mut out = Vec::with_capacity(plaintext.len() + OVERHEAD);

    // Encode the time, space, and block size parameters.
    out.extend(time.to_le_bytes());
    out.extend(space.to_le_bytes());

    // Copy the salt.
    out.extend(salt);

    // Encrypt the ciphertext.
    out.extend(pbenc.encrypt("ciphertext", plaintext));

    // Generate a MAC.
    out.extend(pbenc.mac::<MAC_LEN>("mac"));

    out
}

/// Decrypt the given ciphertext using the given passphrase.
#[must_use]
pub fn decrypt(passphrase: &str, ciphertext: &[u8]) -> Option<Vec<u8>> {
    if ciphertext.len() < OVERHEAD {
        return None;
    }

    // Decode the parameters.
    let (time, ciphertext) = ciphertext.split_at(U32_LEN);
    let time = u32::from_le_bytes(time.try_into().expect("invalid u32 len"));
    let (space, ciphertext) = ciphertext.split_at(U32_LEN);
    let space = u32::from_le_bytes(space.try_into().expect("invalid u32 len"));

    // Perform the balloon hashing.
    let (salt, ciphertext) = ciphertext.split_at(SALT_LEN);
    let mut pbenc = init(passphrase, salt, time, space);

    // Decrypt the ciphertext.
    let (ciphertext, mac) = ciphertext.split_at(ciphertext.len() - MAC_LEN);
    let plaintext = pbenc.decrypt("ciphertext", ciphertext);

    // Verify the MAC.
    pbenc.verify_mac("mac", mac)?;

    Some(plaintext)
}

macro_rules! hash_counter {
    ($pbenc:ident, $ctr:ident, $left:expr, $right:expr, $out:expr) => {
        $pbenc.ad("counter", &$ctr.to_le_bytes());
        $ctr += 1;

        $pbenc.ad("left", &$left);
        $pbenc.ad("right", &$right);

        $pbenc.prf_fill("out", &mut $out);
    };
}

fn init(passphrase: &str, salt: &[u8], time: u32, space: u32) -> Protocol {
    // Normalize the passphrase into NFKC form.
    let mut passphrase = passphrase.nfkc().to_string().bytes().collect::<Vec<u8>>();

    // Initialize the protocol.
    let mut pbenc = Protocol::new("veil.pbenc");

    // Key with the passphrase.
    pbenc.key("passphrase", &passphrase);

    // Include the salt, time, space, block size, and delta parameters as associated data.
    pbenc.ad("salt", salt);
    pbenc.ad("time", &time.to_le_bytes());
    pbenc.ad("space", &space.to_le_bytes());
    pbenc.ad("block-size", &(N as u32).to_le_bytes());
    pbenc.ad("delta", &(DELTA as u32).to_le_bytes());

    // Convert params.
    let time = time as usize;
    let big_space = NonZero::new(U512::from(space)).unwrap();
    let space = space as usize;

    // Allocate buffers.
    let mut ctr = 0u64;
    let mut buf = vec![[0u8; N]; space];

    // Step 1: Expand input into buffer.
    hash_counter!(pbenc, ctr, passphrase, salt, buf[0]);
    for m in 1..space {
        hash_counter!(pbenc, ctr, buf[m - 1], [], buf[m]);
    }

    // Step 2: Mix buffer contents.
    for t in 0..time {
        for m in 0..space {
            // Step 2a: Hash last and current blocks.
            let prev = (m as isize - 1).rem_euclid(space as isize) as usize; // wrap 0 to last block
            hash_counter!(pbenc, ctr, buf[prev], buf[m], buf[m]);

            // Step 2b: Hash in pseudo-randomly chosen blocks.
            for i in 0..DELTA {
                // Map indexes to a block and hash it and the salt.
                let mut idx = [0u8; N];
                idx[..U64_LEN].copy_from_slice(&(t as u64).to_le_bytes());
                idx[U64_LEN..U64_LEN * 2].copy_from_slice(&(m as u64).to_le_bytes());
                idx[U64_LEN * 2..U64_LEN * 3].copy_from_slice(&(i as u64).to_le_bytes());
                hash_counter!(pbenc, ctr, salt, idx, idx);

                // Map the hashed block to a block index.
                let idx = U512::from_le_bytes(idx) % big_space;
                let idx = usize::from_le_bytes(
                    idx.to_le_bytes()[..USIZE_LEN].try_into().expect("invalid usize len"),
                );

                // Hash the pseudo-randomly selected block.
                hash_counter!(pbenc, ctr, buf[idx], [], buf[m]);
            }
        }
    }

    // Step 3: Extract output from buffer.
    pbenc.key("extract", &buf[space - 1]);

    pbenc
}

const SALT_LEN: usize = 16;
const DELTA: usize = 3;
const N: usize = 64;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    pub fn round_trip() {
        let passphrase = "this is a secret";
        let message = b"this is too";
        let ciphertext = encrypt(passphrase, 5, 3, message);
        let plaintext = decrypt(passphrase, &ciphertext);

        assert_eq!(Some(message.to_vec()), plaintext);
    }

    #[test]
    pub fn bad_passphrase() {
        let passphrase = "this is a secret";
        let message = b"this is too";
        let ciphertext = encrypt(passphrase, 5, 3, message);

        assert_eq!(None, decrypt("whoops", &ciphertext));
    }

    #[test]
    pub fn bad_time() {
        let passphrase = "this is a secret";
        let message = b"this is too";
        let mut ciphertext = encrypt(passphrase, 5, 3, message);
        ciphertext[0] ^= 1;

        assert_eq!(None, decrypt(passphrase, &ciphertext));
    }

    #[test]
    pub fn bad_space() {
        let passphrase = "this is a secret";
        let message = b"this is too";
        let mut ciphertext = encrypt(passphrase, 5, 3, message);
        ciphertext[8] ^= 1;

        assert_eq!(None, decrypt(passphrase, &ciphertext));
    }

    #[test]
    pub fn bad_salt() {
        let passphrase = "this is a secret";
        let message = b"this is too";
        let mut ciphertext = encrypt(passphrase, 5, 3, message);
        ciphertext[9] ^= 1;

        assert_eq!(None, decrypt(passphrase, &ciphertext));
    }

    #[test]
    pub fn bad_ciphertext() {
        let passphrase = "this is a secret";
        let message = b"this is too";
        let mut ciphertext = encrypt(passphrase, 5, 3, message);
        ciphertext[OVERHEAD - MAC_LEN + 1] ^= 1;

        assert_eq!(None, decrypt(passphrase, &ciphertext));
    }

    #[test]
    pub fn bad_mac() {
        let passphrase = "this is a secret";
        let message = b"this is too";
        let mut ciphertext = encrypt(passphrase, 5, 3, message);
        ciphertext[message.len() + OVERHEAD - 1] ^= 1;

        assert_eq!(None, decrypt(passphrase, &ciphertext));
    }
}
