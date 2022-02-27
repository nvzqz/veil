//! A multi-recipient, hybrid cryptosystem.

use std::convert::TryInto;
use std::io::{self, Read, Result, Write};
use std::mem;

use rand::RngCore;

use crate::duplex::{Duplex, TAG_LEN};
use crate::ristretto::{CanonicallyEncoded, G, POINT_LEN};
use crate::ristretto::{Point, Scalar};
use crate::schnorr::{Signer, Verifier, SIGNATURE_LEN};
use crate::sres;

/// Encrypt the contents of `reader` such that they can be decrypted and verified by all members of
/// `q_rs` and write the ciphertext to `writer` with `padding` bytes of random data added.
pub fn encrypt(
    reader: &mut impl Read,
    writer: &mut impl Write,
    d_s: &Scalar,
    q_s: &Point,
    q_rs: &[Point],
    padding: u64,
) -> Result<u64> {
    // Initialize a duplex and absorb the sender's public key.
    let mut mres = Duplex::new("veil.mres");
    mres.absorb(&q_s.to_canonical_encoding());

    // Derive a random ephemeral key pair and data encryption key from the duplex's current state,
    // the sender's private key, and a random nonce.
    let (d_e, dek) = mres.hedge(d_s, |clone| (clone.squeeze_scalar(), clone.squeeze(DEK_LEN)));
    let q_e = &d_e * &G;

    // Encode the DEK, the ephemeral public key, and the message offset in a header.
    let msg_offset = ((q_rs.len() as u64) * ENC_HEADER_LEN as u64) + padding;
    let mut header = Vec::with_capacity(HEADER_LEN);
    header.extend(&dek);
    header.extend(q_e.to_canonical_encoding());
    header.extend(&msg_offset.to_le_bytes());

    // Count and sign all of the bytes written to `writer`.
    let signer = Signer::new(writer);

    // Absorb all encrypted headers and padding as they're read.
    let mut mres = mres.absorb_stream(signer);

    // For each recipient, encrypt a copy of the header with veil.sres.
    for q_r in q_rs {
        let ciphertext = sres::encrypt(d_s, q_s, q_r, &header);
        mres.write_all(&ciphertext)?;
    }

    // Add random padding to the end of the headers.
    let rng: &mut dyn RngCore = &mut rand::thread_rng();
    io::copy(&mut rng.take(padding), &mut mres)?;

    // Unwrap the headers and padding writer.
    let (mut mres, mut signer, header_len) = mres.into_inner()?;

    // Use the DEK to key the duplex.
    mres.rekey(&dek);

    // Encrypt the plaintext, pass it through the signer, and write it.
    let ciphertext_len = encrypt_message(&mut mres, reader, &mut signer)?;

    // Sign the encrypted headers and ciphertext with the ephemeral key pair.
    let (sig, writer) = signer.sign(&d_e, &q_e)?;

    // Encrypt the signature.
    let sig = mres.encrypt(&sig);

    // Write the encrypted signature.
    writer.write_all(&sig)?;

    Ok(header_len + ciphertext_len + sig.len() as u64)
}

fn encrypt_message(
    mres: &mut Duplex,
    reader: &mut impl Read,
    writer: &mut impl Write,
) -> Result<u64> {
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
) -> Result<(bool, u64)> {
    // Initialize a duplex and absorb the sender's public key.
    let mut mres = Duplex::new("veil.mres");
    mres.absorb(&q_s.to_canonical_encoding());

    // Initialize a verifier for the entire ciphertext.
    let verifier = Verifier::new();

    // Absorb all encrypted headers and padding as they're read.
    let mut mres = mres.absorb_stream(verifier);

    // Find a header, decrypt it, and write the entirety of the headers and padding to the verifier.
    let (dek, q_e) = match decrypt_header(reader, &mut mres, d_r, q_r, q_s)? {
        Some(header) => header,
        None => return Ok((false, 0)),
    };

    // Unwrap the received cleartext writer.
    let (mut mres, mut verifier, _) = mres.into_inner()?;

    // Use the DEK to key the duplex.
    mres.rekey(&dek);

    // Decrypt the message and get the signature.
    let (written, sig) = decrypt_message(&mut mres, reader, writer, &mut verifier)?;

    // Check to see if we decrypted the entire message.
    if let Some(sig) = sig {
        // Return the signature's validity and the number of bytes of plaintext written.
        Ok((verifier.verify(&q_e, &sig)?, written))
    } else {
        // Otherwise, return a decryption failure and the number of bytes written.
        Ok((false, written))
    }
}

fn decrypt_message(
    mres: &mut Duplex,
    reader: &mut impl Read,
    writer: &mut impl Write,
    verifier: &mut Verifier,
) -> Result<(u64, Option<[u8; SIGNATURE_LEN]>)> {
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

        // Add the block to the verifier.
        verifier.write_all(block)?;

        // Decrypt the block and write the plaintext.
        if let Some(plaintext) = mres.unseal(block) {
            writer.write_all(&plaintext)?;
            written += plaintext.len() as u64;
        } else {
            // If the tag is invalid, return the number of bytes written and no signature.
            return Ok((written, None));
        }

        // Ratchet the duplex state.
        mres.ratchet();

        // Clear the part of the buffer we used.
        buf.drain(0..n);
    }

    // Decrypt the signature.
    let sig = mres.decrypt(&buf);

    Ok((written, Some(sig.try_into().expect("invalid sig len"))))
}

fn decrypt_header(
    reader: &mut impl Read,
    verifier: &mut impl Write,
    d_r: &Scalar,
    q_r: &Point,
    q_s: &Point,
) -> Result<Option<(Vec<u8>, Point)>> {
    let mut buf = Vec::with_capacity(ENC_HEADER_LEN);
    let mut hdr_offset = 0u64;

    // Iterate through blocks, looking for an encrypted header that can be decrypted.
    loop {
        // Read a potential encrypted header.
        let n = reader.take(ENC_HEADER_LEN as u64).read_to_end(&mut buf)?;
        let header = &buf[..n];

        // If the header is short, we're at the end of the reader.
        if header.len() < ENC_HEADER_LEN {
            return Ok(None);
        }

        // Pass the block to the verifier.
        verifier.write_all(header)?;
        hdr_offset += ENC_HEADER_LEN as u64;

        // Try to decrypt the encrypted header.
        if let Some((dek, q_e, msg_offset)) =
            sres::decrypt(d_r, q_r, q_s, header).and_then(|h| decode_header(&h))
        {
            // Read the remainder of the headers and padding and write them to the verifier.
            let mut remainder = reader.take(msg_offset - hdr_offset);
            io::copy(&mut remainder, verifier)?;

            // Return the DEK and ephemeral public key.
            return Ok(Some((dek, q_e)));
        }

        buf.clear();
    }
}

#[inline]
fn decode_header(header: &[u8]) -> Option<(Vec<u8>, Point, u64)> {
    // Check header for proper length.
    if header.len() != HEADER_LEN {
        return None;
    }

    // Split header into components.
    let (dek, q_e) = header.split_at(DEK_LEN);
    let (q_e, msg_offset) = q_e.split_at(POINT_LEN);

    // Decode components.
    let dek = dek.to_vec();
    let q_e = Point::from_canonical_encoding(q_e)?;
    let msg_offset = u64::from_le_bytes(msg_offset.try_into().expect("invalid u64 len"));

    Some((dek, q_e, msg_offset))
}

const DEK_LEN: usize = 32;
const HEADER_LEN: usize = DEK_LEN + POINT_LEN + mem::size_of::<u64>();
const ENC_HEADER_LEN: usize = HEADER_LEN + sres::OVERHEAD;
const BLOCK_LEN: usize = 32 * 1024;
const ENC_BLOCK_LEN: usize = BLOCK_LEN + TAG_LEN;

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::*;

    #[test]
    fn round_trip() -> Result<()> {
        let d_s = Scalar::random(&mut rand::thread_rng());
        let q_s = &d_s * &G;

        let d_r = Scalar::random(&mut rand::thread_rng());
        let q_r = &d_r * &G;

        let message = b"this is a thingy";
        let mut src = Cursor::new(message);
        let mut dst = Cursor::new(Vec::new());

        let ctx_len = encrypt(&mut src, &mut dst, &d_s, &q_s, &[q_s, q_r], 123)?;
        assert_eq!(dst.position(), ctx_len, "returned/observed ciphertext length mismatch");

        let mut src = Cursor::new(dst.into_inner());
        let mut dst = Cursor::new(Vec::new());

        let (verified, ptx_len) = decrypt(&mut src, &mut dst, &d_r, &q_r, &q_s)?;
        assert_eq!(dst.position(), ptx_len, "returned/observed plaintext length mismatch");
        assert_eq!(message.to_vec(), dst.into_inner(), "incorrect plaintext");
        assert!(verified, "valid message not verified");

        Ok(())
    }

    #[test]
    fn bad_sender_public_key() -> Result<()> {
        let d_s = Scalar::random(&mut rand::thread_rng());
        let q_s = &d_s * &G;

        let d_r = Scalar::random(&mut rand::thread_rng());
        let q_r = &d_r * &G;

        let message = b"this is a thingy";
        let mut src = Cursor::new(message);
        let mut dst = Cursor::new(Vec::new());

        let ctx_len = encrypt(&mut src, &mut dst, &d_s, &q_s, &[q_s, q_r], 123)?;
        assert_eq!(dst.position(), ctx_len, "returned/observed ciphertext length mismatch");

        let q_s = Point::random(&mut rand::thread_rng());

        let mut src = Cursor::new(dst.into_inner());
        let mut dst = Cursor::new(Vec::new());

        let (verified, ptx_len) = decrypt(&mut src, &mut dst, &d_r, &q_r, &q_s)?;
        assert_eq!(dst.position(), ptx_len, "returned/observed plaintext length mismatch");
        assert!(!verified, "invalid message not rejected");

        Ok(())
    }

    #[test]
    fn bad_recipient_public_key() -> Result<()> {
        let d_s = Scalar::random(&mut rand::thread_rng());
        let q_s = &d_s * &G;

        let d_r = Scalar::random(&mut rand::thread_rng());
        let q_r = &d_r * &G;

        let message = b"this is a thingy";
        let mut src = Cursor::new(message);
        let mut dst = Cursor::new(Vec::new());

        let ctx_len = encrypt(&mut src, &mut dst, &d_s, &q_s, &[q_s, q_r], 123)?;
        assert_eq!(dst.position(), ctx_len, "returned/observed ciphertext length mismatch");

        let q_r = Point::random(&mut rand::thread_rng());

        let mut src = Cursor::new(dst.into_inner());
        let mut dst = Cursor::new(Vec::new());

        let (verified, ptx_len) = decrypt(&mut src, &mut dst, &d_r, &q_r, &q_s)?;
        assert_eq!(dst.position(), ptx_len, "returned/observed plaintext length mismatch");
        assert!(!verified, "invalid message not rejected");

        Ok(())
    }

    #[test]
    fn bad_recipient_private_key() -> Result<()> {
        let d_s = Scalar::random(&mut rand::thread_rng());
        let q_s = &d_s * &G;

        let d_r = Scalar::random(&mut rand::thread_rng());
        let q_r = &d_r * &G;

        let message = b"this is a thingy";
        let mut src = Cursor::new(message);
        let mut dst = Cursor::new(Vec::new());

        let ctx_len = encrypt(&mut src, &mut dst, &d_s, &q_s, &[q_s, q_r], 123)?;
        assert_eq!(dst.position(), ctx_len, "returned/observed ciphertext length mismatch");

        let d_r = Scalar::random(&mut rand::thread_rng());

        let mut src = Cursor::new(dst.into_inner());
        let mut dst = Cursor::new(Vec::new());

        let (verified, ptx_len) = decrypt(&mut src, &mut dst, &d_r, &q_r, &q_s)?;
        assert_eq!(dst.position(), ptx_len, "returned/observed plaintext length mismatch");
        assert!(!verified, "invalid message not rejected");

        Ok(())
    }

    #[test]
    fn multi_block_message() -> Result<()> {
        let d_s = Scalar::random(&mut rand::thread_rng());
        let q_s = &d_s * &G;

        let d_r = Scalar::random(&mut rand::thread_rng());
        let q_r = &d_r * &G;

        let message = [69u8; 65 * 1024];
        let mut src = Cursor::new(message);
        let mut dst = Cursor::new(Vec::new());

        let ctx_len = encrypt(&mut src, &mut dst, &d_s, &q_s, &[q_s, q_r], 123)?;
        assert_eq!(dst.position(), ctx_len, "returned/observed ciphertext length mismatch");

        let mut src = Cursor::new(dst.into_inner());
        let mut dst = Cursor::new(Vec::new());

        let (verified, ptx_len) = decrypt(&mut src, &mut dst, &d_r, &q_r, &q_s)?;
        assert_eq!(dst.position(), ptx_len, "returned/observed plaintext length mismatch");
        assert_eq!(message.to_vec(), dst.into_inner(), "incorrect plaintext");
        assert!(verified, "valid message not verified");

        Ok(())
    }

    #[test]
    fn split_sig() -> Result<()> {
        let d_s = Scalar::random(&mut rand::thread_rng());
        let q_s = &d_s * &G;

        let d_r = Scalar::random(&mut rand::thread_rng());
        let q_r = &d_r * &G;

        let message = [69u8; 32 * 1024 - 37];
        let mut src = Cursor::new(message);
        let mut dst = Cursor::new(Vec::new());

        let ctx_len = encrypt(&mut src, &mut dst, &d_s, &q_s, &[q_s, q_r], 0)?;
        assert_eq!(dst.position(), ctx_len, "returned/observed ciphertext length mismatch");

        let mut src = Cursor::new(dst.into_inner());
        let mut dst = Cursor::new(Vec::new());

        let (verified, ptx_len) = decrypt(&mut src, &mut dst, &d_r, &q_r, &q_s)?;
        assert_eq!(dst.position(), ptx_len, "returned/observed plaintext length mismatch");
        assert_eq!(message.to_vec(), dst.into_inner(), "incorrect plaintext");
        assert!(verified, "valid message not verified");

        Ok(())
    }

    #[test]
    fn flip_every_bit() -> Result<()> {
        let d_s = Scalar::random(&mut rand::thread_rng());
        let q_s = &d_s * &G;

        let d_r = Scalar::random(&mut rand::thread_rng());
        let q_r = &d_r * &G;

        let message = b"this is a thingy";
        let mut src = Cursor::new(message);
        let mut dst = Cursor::new(Vec::new());

        encrypt(&mut src, &mut dst, &d_s, &q_s, &[q_s, q_r], 123)?;

        let ciphertext = dst.into_inner();

        for i in 0..ciphertext.len() {
            for j in 0u8..8 {
                let mut ciphertext = ciphertext.clone();
                ciphertext[i] ^= 1 << j;
                let mut src = Cursor::new(ciphertext);

                let (verified, _) = decrypt(&mut src, &mut io::sink(), &d_r, &q_r, &q_s)?;
                assert!(!verified, "bit flip at byte {}, bit {} produced a valid message", i, j);
            }
        }

        Ok(())
    }
}
