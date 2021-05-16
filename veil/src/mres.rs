use std::convert::TryInto;
use std::io::{self, Read, Result, Write};

use curve25519_dalek::constants::RISTRETTO_BASEPOINT_POINT;
use curve25519_dalek::ristretto::RistrettoPoint;
use curve25519_dalek::scalar::Scalar;
use strobe_rs::{SecParam, Strobe};

use crate::akem;
use crate::schnorr::{Signer, Verifier, SIGNATURE_LEN};
use crate::util::{StrobeExt, MAC_LEN, U64_LEN};

/// Encrypt the contents of `reader` such that they can be decrypted and verified by all members of
/// `q_rs` and write the ciphertext to `writer` with `padding` bytes of random data added.
pub fn encrypt<R, W>(
    reader: &mut R,
    writer: &mut W,
    d_s: &Scalar,
    q_s: &RistrettoPoint,
    q_rs: Vec<RistrettoPoint>,
    padding: u64,
) -> Result<u64>
where
    R: Read,
    W: Write,
{
    // Initialize a protocol and add the MAC length and sender's public key as associated data.
    let mut mres = Strobe::new(b"veil.mres", SecParam::B256);
    mres.meta_ad_u32(MAC_LEN as u32);
    mres.ad_point(q_s);

    // Derive a random ephemeral key pair and DEK from the protocol's current state, the sender's
    // private key, and a random nonce.
    let (d_e, q_e, dek) = mres.hedge(d_s.as_bytes(), |clone| {
        // Generate an ephemeral key pair.
        let d_e = clone.prf_scalar();
        let q_e = RISTRETTO_BASEPOINT_POINT * d_e;

        // Return the key pair and a DEK.
        (d_e, q_e, clone.prf_array())
    });

    // Encode the DEK and message offset in a header.
    let header = encode_header(&dek, q_rs.len(), padding);

    // Count and sign all of the bytes written to `writer`.
    let mut written = 0u64;
    let mut signer = Signer::new(writer);

    // For each recipient, encrypt a copy of the header.
    for q_r in q_rs {
        let ciphertext = akem::encapsulate(d_s, q_s, &d_e, &q_e, &q_r, &header);
        signer.write_all(&ciphertext)?;
        written += ciphertext.len() as u64;
    }

    // Add random padding to the end of the headers.
    written += io::copy(&mut RngReader.take(padding), &mut signer)?;

    // Key the protocol with the DEK.
    mres.key(&dek, false);

    // Prep for streaming encryption.
    mres.send_enc(&mut [], false);

    // Read through src in 32KiB chunks.
    let mut buf = [0u8; 32 * 1024];
    loop {
        let n = reader.read(&mut buf)?;
        if n == 0 {
            break;
        }

        // Encrypt the block.
        mres.send_enc(&mut buf[0..n], true);

        // Write the ciphertext and sign it.
        signer.write_all(&buf[0..n])?;
        written += n as u64;
    }

    // Sign the encrypted headers and ciphertext with the ephemeral key pair.
    let mut sig = signer.sign(&d_e, &q_e);

    // Encrypt the signature.
    mres.send_enc(&mut sig, false);

    // Write the encrypted signature.
    signer.into_inner().write_all(&sig)?;
    written += sig.len() as u64;

    Ok(written)
}

/// Decrypt the contents of `reader` iff they were originally encrypted by `q_s` for `q_r` and write
/// the plaintext to `writer`.
pub fn decrypt<R, W>(
    reader: &mut R,
    writer: &mut W,
    d_r: &Scalar,
    q_r: &RistrettoPoint,
    q_s: &RistrettoPoint,
) -> Result<(bool, u64)>
where
    R: Read,
    W: Write,
{
    // Initialize a protocol and add the MAC length and sender's public key as associated data.
    let mut mres = Strobe::new(b"veil.mres", SecParam::B256);
    mres.meta_ad_u32(MAC_LEN as u32);
    mres.ad_point(q_s);

    // Initialize a verifier for the entire ciphertext.
    let mut verifier = Verifier::new();

    // Find a header, decrypt it, and write the entirety of the headers and padding to the verifier.
    let (dek, q_e) = match decrypt_header(reader, &mut verifier, d_r, q_r, q_s)? {
        Some((dek, q_e)) => (dek, q_e),
        None => return Ok((false, 0)),
    };

    // Key the protocol with the recovered DEK.
    mres.key(&dek, false);

    // Decrypt the message and get the signature.
    let (written, sig) = decrypt_message(reader, writer, &mut verifier, &mut mres)?;

    // Return the signature's validity and the number of bytes of plaintext written.
    Ok((verifier.verify(&q_e, &sig), written))
}

const DEK_LEN: usize = 32;
const HEADER_LEN: usize = DEK_LEN + U64_LEN;
const ENC_HEADER_LEN: usize = HEADER_LEN + akem::OVERHEAD;

fn decrypt_message<R, W>(
    reader: &mut R,
    writer: &mut W,
    verifier: &mut Verifier,
    mres: &mut Strobe,
) -> Result<(u64, [u8; SIGNATURE_LEN])>
where
    R: Read,
    W: Write,
{
    let mut written = 0u64;
    let mut input = [0u8; 32 * 1024];
    let mut buf = Vec::with_capacity(input.len() + SIGNATURE_LEN);

    // Prep for streaming decryption.
    mres.recv_enc(&mut [], false);

    // Read through src in 32KiB chunks, keeping the last 64 bytes as the signature.
    loop {
        // Read a block of ciphertext and copy it to the buffer.
        let n = reader.read(&mut input)?;
        buf.extend_from_slice(&input[..n]);

        // Process the data if we have at least a signature's worth.
        if buf.len() > SIGNATURE_LEN {
            // Pop the first N-64 bytes off the buffer.
            let mut block: Vec<u8> = buf.drain(..buf.len() - SIGNATURE_LEN).collect();

            // Verify the ciphertext.
            verifier.write_all(&block)?;

            // Decrypt the ciphertext.
            mres.recv_enc(&mut block, true);

            // Write the plaintext.
            writer.write_all(&block)?;
            written += block.len() as u64;
        }

        // If our last read returned zero bytes, we're at the end of the ciphertext.
        if n == 0 {
            break;
        }
    }

    // Keep the last 64 bytes as the encrypted signature.
    let mut sig: [u8; SIGNATURE_LEN] = buf.try_into().unwrap();
    mres.recv_enc(&mut sig, false);

    // Return the bytes written and the decrypted signature.
    Ok((written, sig))
}

fn decrypt_header<R>(
    reader: &mut R,
    verifier: &mut Verifier,
    d_r: &Scalar,
    q_r: &RistrettoPoint,
    q_s: &RistrettoPoint,
) -> Result<Option<([u8; DEK_LEN], RistrettoPoint)>>
where
    R: Read,
{
    let mut buf = [0u8; ENC_HEADER_LEN];
    let mut hdr_offset = 0u64;

    // Iterate through blocks, looking for an encrypted header that can be decrypted.
    while let Ok(()) = reader.read_exact(&mut buf) {
        verifier.write_all(&buf)?;
        hdr_offset += buf.len() as u64;

        if let Some((p, header)) = akem::decapsulate(d_r, q_r, q_s, &buf) {
            // Recover the ephemeral public key, the DEK, and the message offset.
            let dek: [u8; DEK_LEN] = header[..DEK_LEN].try_into().unwrap();
            let msg_offset = u64::from_le_bytes(header[DEK_LEN..].try_into().unwrap());

            // Read the remainder of the headers and padding and write them to the verifier.
            let mut remainder = reader.take(msg_offset - hdr_offset);
            io::copy(&mut remainder, verifier)?;

            // Return the DEK and ephemeral public key.
            return Ok(Some((dek, p)));
        }
    }

    // If no header was found, return none.
    Ok(None)
}

fn encode_header(dek: &[u8; DEK_LEN], r_len: usize, padding: u64) -> Vec<u8> {
    let msg_offset = ((r_len as u64) * ENC_HEADER_LEN as u64) + padding;
    vec![dek.to_vec(), (&msg_offset.to_le_bytes()).to_vec()].concat()
}

struct RngReader;

impl Read for RngReader {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        getrandom::getrandom(buf).expect("rng failure");
        Ok(buf.len())
    }
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use curve25519_dalek::constants::RISTRETTO_BASEPOINT_POINT;

    use crate::util;

    use super::*;

    #[test]
    pub fn round_trip() -> Result<()> {
        let d_s = util::rand_scalar();
        let q_s = RISTRETTO_BASEPOINT_POINT * d_s;

        let d_r = util::rand_scalar();
        let q_r = RISTRETTO_BASEPOINT_POINT * d_r;

        let message = b"this is a thingy";
        let mut src = Cursor::new(message);
        let mut dst = Cursor::new(Vec::new());

        let ctx_len = encrypt(&mut src, &mut dst, &d_s, &q_s, vec![q_s, q_r], 123)?;
        assert_eq!(dst.position(), ctx_len);

        let mut src = Cursor::new(dst.into_inner());
        let mut dst = Cursor::new(Vec::new());

        let (verified, ptx_len) = decrypt(&mut src, &mut dst, &d_r, &q_r, &q_s)?;
        assert_eq!(true, verified);
        assert_eq!(dst.position(), ptx_len);
        assert_eq!(message.to_vec(), dst.into_inner());

        Ok(())
    }

    #[test]
    pub fn multi_block_message() -> Result<()> {
        let d_s = util::rand_scalar();
        let q_s = RISTRETTO_BASEPOINT_POINT * d_s;

        let d_r = util::rand_scalar();
        let q_r = RISTRETTO_BASEPOINT_POINT * d_r;

        let message = [69u8; 65 * 1024];
        let mut src = Cursor::new(message);
        let mut dst = Cursor::new(Vec::new());

        let ctx_len = encrypt(&mut src, &mut dst, &d_s, &q_s, vec![q_s, q_r], 123)?;
        assert_eq!(dst.position(), ctx_len);

        let mut src = Cursor::new(dst.into_inner());
        let mut dst = Cursor::new(Vec::new());

        let (verified, ptx_len) = decrypt(&mut src, &mut dst, &d_r, &q_r, &q_s)?;
        assert_eq!(true, verified);
        assert_eq!(dst.position(), ptx_len);
        assert_eq!(message.to_vec(), dst.into_inner());

        Ok(())
    }

    #[test]
    pub fn split_sig() -> Result<()> {
        let d_s = util::rand_scalar();
        let q_s = RISTRETTO_BASEPOINT_POINT * d_s;

        let d_r = util::rand_scalar();
        let q_r = RISTRETTO_BASEPOINT_POINT * d_r;

        let message = [69u8; 32 * 1024 - 37];
        let mut src = Cursor::new(message);
        let mut dst = Cursor::new(Vec::new());

        let ctx_len = encrypt(&mut src, &mut dst, &d_s, &q_s, vec![q_s, q_r], 0)?;
        assert_eq!(dst.position(), ctx_len);

        let mut src = Cursor::new(dst.into_inner());
        let mut dst = Cursor::new(Vec::new());

        let (verified, ptx_len) = decrypt(&mut src, &mut dst, &d_r, &q_r, &q_s)?;
        assert_eq!(true, verified);
        assert_eq!(dst.position(), ptx_len);
        assert_eq!(message.to_vec(), dst.into_inner());

        Ok(())
    }

    #[test]
    pub fn bad_message() -> Result<()> {
        let d_s = util::rand_scalar();
        let q_s = RISTRETTO_BASEPOINT_POINT * d_s;

        let d_r = util::rand_scalar();
        let q_r = RISTRETTO_BASEPOINT_POINT * d_r;

        let message = [69u8; 32 * 1024 - 37];
        let mut src = Cursor::new(message);
        let mut dst = Cursor::new(Vec::new());

        let ctx_len = encrypt(&mut src, &mut dst, &d_s, &q_s, vec![q_s, q_r], 0)?;
        assert_eq!(dst.position(), ctx_len);

        let mut ciphertext = dst.into_inner();
        ciphertext[22] ^= 1;

        let mut src = Cursor::new(ciphertext);
        let mut dst = Cursor::new(Vec::new());

        let (verified, ptx_len) = decrypt(&mut src, &mut dst, &d_r, &q_r, &q_s)?;
        assert_eq!(false, verified);
        assert_eq!(dst.position(), ptx_len);

        Ok(())
    }
}