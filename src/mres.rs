//! A multi-recipient, hybrid cryptosystem.

use std::convert::TryInto;
use std::io::{self, Read, Write};
use std::mem;
use std::result::Result;

use rand::{CryptoRng, Rng};

use crate::duplex::{Duplex, TAG_LEN};
use crate::ristretto::{CanonicallyEncoded, G, POINT_LEN};
use crate::ristretto::{Point, Scalar};
use crate::schnorr::SIGNATURE_LEN;
use crate::{schnorr, sres, DecryptError};

/// Encrypt the contents of `reader` such that they can be decrypted and verified by all members of
/// `q_rs` and write the ciphertext to `writer` with `padding` bytes of random data added.
pub fn encrypt(
    mut rng: impl Rng + CryptoRng,
    reader: &mut impl Read,
    writer: &mut impl Write,
    d_s: &Scalar,
    q_s: &Point,
    q_rs: &[Point],
    padding: u64,
) -> io::Result<u64> {
    // Initialize a duplex and absorb the sender's public key.
    let mut mres = Duplex::new("veil.mres");
    mres.absorb(&q_s.to_canonical_encoding());

    // Derive a random ephemeral key pair and data encryption key from the duplex's current state,
    // the sender's private key, and a random nonce.
    let (d_e, k, dek) = mres.hedge(&mut rng, d_s, |clone| {
        (clone.squeeze_scalar(), clone.squeeze_scalar(), clone.squeeze(DEK_LEN))
    });
    let q_e = &d_e * &G;

    // Encode the DEK and the message offset in a header.
    let msg_offset = ((q_rs.len() as u64) * ENC_HEADER_LEN as u64) + padding;
    let mut header = Vec::with_capacity(HEADER_LEN);
    header.extend(&dek);
    header.extend(&msg_offset.to_le_bytes());

    // Absorb all encrypted headers and padding as they're written.
    let mut mres = mres.absorb_stream(writer);

    // Write a masked copy of the ephemeral public key.
    mres.write_all(&mask_point(&mut rng, q_e))?;

    // For each recipient, encrypt a copy of the header with veil.sres.
    for q_r in q_rs {
        let ciphertext = sres::encrypt(&mut rng, d_s, q_s, &d_e, &q_e, q_r, &header);
        mres.write_all(&ciphertext)?;
    }

    // Add random padding to the end of the headers.
    io::copy(&mut RngRead(&mut rng).take(padding), &mut mres)?;

    // Unwrap the headers and padding writer.
    let (mut mres, mut writer, header_len) = mres.into_inner()?;

    // Use the DEK to key the duplex.
    mres.rekey(&dek);

    // Encrypt the plaintext in blocks and write them.
    let ciphertext_len = encrypt_message(&mut mres, reader, &mut writer)?;

    // Sign the duplex's final state with the ephemeral private key.
    let (i, s) = schnorr::sign(&mut mres, &d_e, k);

    // Encrypt the proof scalar.
    let s = mres.encrypt(&s.to_canonical_encoding());

    // Write the signature components.
    writer.write_all(&i)?;
    writer.write_all(&s)?;

    Ok(header_len + ciphertext_len + i.len() as u64 + s.len() as u64)
}

fn encrypt_message(
    mres: &mut Duplex,
    reader: &mut impl Read,
    writer: &mut impl Write,
) -> io::Result<u64> {
    let mut buf = Vec::with_capacity(BLOCK_LEN);
    let mut written = 0;

    loop {
        // Read a block of data.
        let n = reader.take(BLOCK_LEN as u64).read_to_end(&mut buf)?;
        let block = &buf[..n];

        // Encrypt the block and write the ciphertext and a tag.
        writer.write_all(&mres.seal(block))?;
        written += (n + TAG_LEN) as u64;

        // Ratchet the duplex state to prevent rollback. This protects previous blocks from being
        // reversed in the event of the duplex's state being compromised.
        mres.ratchet();

        // If the block was undersized, we're at the end of the reader.
        if n < BLOCK_LEN {
            break;
        }

        // Reset the buffer.
        buf.clear();
    }

    Ok(written)
}

/// Decrypt the contents of `reader` iff they were originally encrypted by `q_s` for `q_r` and write
/// the plaintext to `writer`.
pub fn decrypt(
    reader: &mut impl Read,
    writer: &mut impl Write,
    d_r: &Scalar,
    q_r: &Point,
    q_s: &Point,
) -> Result<u64, DecryptError> {
    // Initialize a duplex and absorb the sender's public key.
    let mut mres = Duplex::new("veil.mres");
    mres.absorb(&q_s.to_canonical_encoding());

    // Absorb all encrypted headers and padding as they're read.
    let mut mres = mres.absorb_stream(io::sink());

    // Read, unmask, and decode the ephemeral public key.
    let mut q_e = [0u8; 32];
    reader.read_exact(&mut q_e)?;
    mres.write_all(&q_e)?;
    let q_e = unmask_point(q_e).ok_or(DecryptError::InvalidCiphertext)?;

    // Find a header, decrypt it, and write the entirety of the headers and padding to the verifier.
    let dek = decrypt_header(reader, &mut mres, d_r, q_r, &q_e, q_s)?;

    // Unwrap the duplex state.
    let (mut mres, _, _) = mres.into_inner()?;

    // Use the DEK to key the duplex.
    mres.rekey(&dek);

    // Decrypt the message and verify the signature.
    decrypt_message(&mut mres, &q_e, reader, writer)
}

fn decrypt_message(
    mres: &mut Duplex,
    q_e: &Point,
    reader: &mut impl Read,
    writer: &mut impl Write,
) -> Result<u64, DecryptError> {
    let mut buf = Vec::with_capacity(ENC_BLOCK_LEN + SIGNATURE_LEN);
    let mut written = 0;

    loop {
        // Read a block and a possible signature, keeping in mind the unused bit of the buffer from
        // the last iteration.
        let n = reader
            .take((ENC_BLOCK_LEN + SIGNATURE_LEN - buf.len()) as u64)
            .read_to_end(&mut buf)?;

        // If we're at the end of the reader, we only have the signature left to process. Break out
        // of the read loop and go process the signature.
        if n == 0 {
            break;
        }

        // Pretend we don't see the possible signature at the end.
        let n = buf.len() - SIGNATURE_LEN;
        let block = &buf[..n];

        // Decrypt the block and write the plaintext. If the block cannot be decrypted, return an
        // error.
        let plaintext = mres.unseal(block).ok_or(DecryptError::InvalidCiphertext)?;
        writer.write_all(&plaintext)?;
        written += plaintext.len() as u64;

        // Ratchet the duplex state.
        mres.ratchet();

        // Clear the part of the buffer we used.
        buf.drain(0..n);
    }

    // Verify the signature.
    schnorr::verify(mres, q_e, &buf).map_err(|_| DecryptError::InvalidCiphertext)?;

    // Return the number of bytes.
    Ok(written)
}

fn decrypt_header(
    reader: &mut impl Read,
    verifier: &mut impl Write,
    d_r: &Scalar,
    q_r: &Point,
    q_e: &Point,
    q_s: &Point,
) -> Result<Vec<u8>, DecryptError> {
    let mut buf = Vec::with_capacity(ENC_HEADER_LEN);
    let mut hdr_offset = 0u64;

    // Iterate through blocks, looking for an encrypted header that can be decrypted.
    loop {
        // Read a potential encrypted header.
        let n = reader.take(ENC_HEADER_LEN as u64).read_to_end(&mut buf)?;
        let header = &buf[..n];

        // If the header is short, we're at the end of the reader.
        if header.len() < ENC_HEADER_LEN {
            return Err(DecryptError::InvalidCiphertext);
        }

        // Pass the block to the verifier.
        verifier.write_all(header)?;
        hdr_offset += ENC_HEADER_LEN as u64;

        // Try to decrypt the encrypted header.
        if let Some((dek, msg_offset)) =
            sres::decrypt(d_r, q_r, q_e, q_s, header).and_then(decode_header)
        {
            // Read the remainder of the headers and padding and write them to the verifier.
            let mut remainder = reader.take(msg_offset - hdr_offset);
            io::copy(&mut remainder, verifier)?;

            // Return the DEK and ephemeral public key.
            return Ok(dek);
        }

        buf.clear();
    }
}

#[inline]
fn decode_header(header: Vec<u8>) -> Option<(Vec<u8>, u64)> {
    // Check header for proper length.
    if header.len() != HEADER_LEN {
        return None;
    }

    // Split header into components.
    let (dek, msg_offset) = header.split_at(POINT_LEN);

    // Decode components.
    let dek = dek.to_vec();
    let msg_offset = u64::from_le_bytes(msg_offset.try_into().expect("invalid u64 len"));

    Some((dek, msg_offset))
}

// Encode the given point and randomly mask the two bits which are always zero. This does not
// produce a bitstring which is uniformly distributed, but rather one which is biased to 25% of the
// space. In the absence of a bijective Elligator2-style mapping for Ristretto, this is the best
// we can do.
#[inline]
fn mask_point(mut rng: impl Rng + CryptoRng, q: Point) -> [u8; 32] {
    let mask: u8 = rng.gen();
    let mut b = q.to_canonical_encoding();
    b[0] |= mask & !0xfe;
    b[31] |= mask & !0x7f;
    b
}

#[inline]
fn unmask_point(mut b: [u8; 32]) -> Option<Point> {
    b[0] &= 0xfe;
    b[31] &= 0x7f;
    Point::from_canonical_encoding(&b)
}

struct RngRead<R>(R)
where
    R: Rng + CryptoRng;

impl<R> Read for RngRead<R>
where
    R: Rng + CryptoRng,
{
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.0.try_fill_bytes(buf)?;
        Ok(buf.len())
    }
}

const DEK_LEN: usize = 32;
const HEADER_LEN: usize = DEK_LEN + mem::size_of::<u64>();
const ENC_HEADER_LEN: usize = HEADER_LEN + sres::OVERHEAD;
const BLOCK_LEN: usize = 32 * 1024;
const ENC_BLOCK_LEN: usize = BLOCK_LEN + TAG_LEN;

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use rand::SeedableRng;
    use rand_chacha::ChaChaRng;

    use super::*;

    macro_rules! assert_failed {
        ($action: expr) => {
            match $action {
                Ok(_) => panic!("decrypted but shouldn't have"),
                Err(DecryptError::InvalidCiphertext) => Ok(()),
                Err(e) => Err(e),
            }
        };
    }

    #[test]
    fn round_trip() -> Result<(), DecryptError> {
        let (mut rng, d_s, q_s, d_r, q_r) = setup();

        let message = b"this is a thingy";
        let mut src = Cursor::new(message);
        let mut dst = Cursor::new(Vec::new());

        let ctx_len = encrypt(&mut rng, &mut src, &mut dst, &d_s, &q_s, &[q_s, q_r], 123)?;
        assert_eq!(dst.position(), ctx_len, "returned/observed ciphertext length mismatch");

        let mut src = Cursor::new(dst.into_inner());
        let mut dst = Cursor::new(Vec::new());

        let ptx_len = decrypt(&mut src, &mut dst, &d_r, &q_r, &q_s)?;
        assert_eq!(dst.position(), ptx_len, "returned/observed plaintext length mismatch");
        assert_eq!(message.to_vec(), dst.into_inner(), "incorrect plaintext");

        Ok(())
    }

    #[test]
    fn bad_sender_public_key() -> Result<(), DecryptError> {
        let (mut rng, d_s, q_s, d_r, q_r) = setup();

        let message = b"this is a thingy";
        let mut src = Cursor::new(message);
        let mut dst = Cursor::new(Vec::new());

        let ctx_len = encrypt(&mut rng, &mut src, &mut dst, &d_s, &q_s, &[q_s, q_r], 123)?;
        assert_eq!(dst.position(), ctx_len, "returned/observed ciphertext length mismatch");

        let q_s = Point::random(&mut rng);

        let mut src = Cursor::new(dst.into_inner());
        let mut dst = Cursor::new(Vec::new());

        assert_failed!(decrypt(&mut src, &mut dst, &d_r, &q_r, &q_s))
    }

    #[test]
    fn bad_recipient_public_key() -> Result<(), DecryptError> {
        let (mut rng, d_s, q_s, d_r, q_r) = setup();

        let message = b"this is a thingy";
        let mut src = Cursor::new(message);
        let mut dst = Cursor::new(Vec::new());

        let ctx_len = encrypt(&mut rng, &mut src, &mut dst, &d_s, &q_s, &[q_s, q_r], 123)?;
        assert_eq!(dst.position(), ctx_len, "returned/observed ciphertext length mismatch");

        let q_r = Point::random(&mut rng);

        let mut src = Cursor::new(dst.into_inner());
        let mut dst = Cursor::new(Vec::new());

        assert_failed!(decrypt(&mut src, &mut dst, &d_r, &q_r, &q_s))
    }

    #[test]
    fn bad_recipient_private_key() -> Result<(), DecryptError> {
        let (mut rng, d_s, q_s, _, q_r) = setup();

        let message = b"this is a thingy";
        let mut src = Cursor::new(message);
        let mut dst = Cursor::new(Vec::new());

        let ctx_len = encrypt(&mut rng, &mut src, &mut dst, &d_s, &q_s, &[q_s, q_r], 123)?;
        assert_eq!(dst.position(), ctx_len, "returned/observed ciphertext length mismatch");

        let d_r = Scalar::random(&mut rng);

        let mut src = Cursor::new(dst.into_inner());
        let mut dst = Cursor::new(Vec::new());

        assert_failed!(decrypt(&mut src, &mut dst, &d_r, &q_r, &q_s))
    }

    #[test]
    fn multi_block_message() -> Result<(), DecryptError> {
        let (mut rng, d_s, q_s, d_r, q_r) = setup();

        let message = [69u8; 65 * 1024];
        let mut src = Cursor::new(message);
        let mut dst = Cursor::new(Vec::new());

        let ctx_len = encrypt(&mut rng, &mut src, &mut dst, &d_s, &q_s, &[q_s, q_r], 123)?;
        assert_eq!(dst.position(), ctx_len, "returned/observed ciphertext length mismatch");

        let mut src = Cursor::new(dst.into_inner());
        let mut dst = Cursor::new(Vec::new());

        let ptx_len = decrypt(&mut src, &mut dst, &d_r, &q_r, &q_s)?;
        assert_eq!(dst.position(), ptx_len, "returned/observed plaintext length mismatch");
        assert_eq!(message.to_vec(), dst.into_inner(), "incorrect plaintext");

        Ok(())
    }

    #[test]
    fn split_sig() -> Result<(), DecryptError> {
        let (mut rng, d_s, q_s, d_r, q_r) = setup();

        let message = [69u8; 32 * 1024 - 37];
        let mut src = Cursor::new(message);
        let mut dst = Cursor::new(Vec::new());

        let ctx_len = encrypt(&mut rng, &mut src, &mut dst, &d_s, &q_s, &[q_s, q_r], 0)?;
        assert_eq!(dst.position(), ctx_len, "returned/observed ciphertext length mismatch");

        let mut src = Cursor::new(dst.into_inner());
        let mut dst = Cursor::new(Vec::new());

        let ptx_len = decrypt(&mut src, &mut dst, &d_r, &q_r, &q_s)?;
        assert_eq!(dst.position(), ptx_len, "returned/observed plaintext length mismatch");
        assert_eq!(message.to_vec(), dst.into_inner(), "incorrect plaintext");

        Ok(())
    }

    #[test]
    fn flip_every_bit() -> Result<(), DecryptError> {
        let (mut rng, d_s, q_s, d_r, q_r) = setup();

        let message = b"this is a thingy";
        let mut src = Cursor::new(message);
        let mut dst = Cursor::new(Vec::new());

        encrypt(&mut rng, &mut src, &mut dst, &d_s, &q_s, &[q_s, q_r], 123)?;

        let ciphertext = dst.into_inner();

        for i in 0..ciphertext.len() {
            for j in 0u8..8 {
                let mut ciphertext = ciphertext.clone();
                ciphertext[i] ^= 1 << j;
                let mut src = Cursor::new(ciphertext);

                match decrypt(&mut src, &mut io::sink(), &d_r, &q_r, &q_s) {
                    Err(DecryptError::InvalidCiphertext) => {}
                    Ok(_) => panic!("bit flip at byte {i}, bit {j} produced a valid message"),
                    Err(e) => panic!("unknown error: {}", e),
                };
            }
        }

        Ok(())
    }

    fn setup() -> (ChaChaRng, Scalar, Point, Scalar, Point) {
        let mut rng = ChaChaRng::seed_from_u64(0xDEADBEEF);

        let d_s = Scalar::random(&mut rng);
        let q_s = &d_s * &G;

        let d_r = Scalar::random(&mut rng);
        let q_r = &d_r * &G;

        (rng, d_s, q_s, d_r, q_r)
    }
}
