use std::fs;
use std::io::Read;
use std::os::raw::c_int;
use std::path::Path;

use anyhow::Result;
use filedescriptor::FileDescriptor;

use cli::*;
use veil::{PublicKey, SecretKey, Signature};

mod cli;

fn main() -> Result<()> {
    let opts: Opts = argh::from_env();
    match opts.cmd {
        Command::SecretKey(mut cmd) => secret_key(&mut cmd),
        Command::PublicKey(cmd) => public_key(&cmd),
        Command::DeriveKey(cmd) => derive_key(&cmd),
        Command::Encrypt(mut cmd) => encrypt(&mut cmd),
        Command::Decrypt(mut cmd) => decrypt(&mut cmd),
        Command::Sign(mut cmd) => sign(&mut cmd),
        Command::Verify(mut cmd) => verify(&mut cmd),
    }
}

fn secret_key(cmd: &mut SecretKeyArgs) -> Result<()> {
    let passphrase = prompt_passphrase(cmd.passphrase_fd)?;
    let secret_key = SecretKey::new();
    let ciphertext = secret_key.encrypt(passphrase.as_bytes(), 1 << 7, 1 << 10);
    fs::write(&mut cmd.output, ciphertext)?;
    Ok(())
}

fn public_key(cmd: &PublicKeyArgs) -> Result<()> {
    let secret_key = decrypt_secret_key(cmd.passphrase_fd, &cmd.secret_key)?;
    let public_key = secret_key.public_key(&cmd.key_id);
    println!("{}", public_key);
    Ok(())
}

fn derive_key(cmd: &DeriveKeyArgs) -> Result<()> {
    let root = cmd.public_key.parse::<PublicKey>()?;
    let public_key = root.derive(&cmd.sub_key_id);
    println!("{}", public_key);
    Ok(())
}

fn encrypt(cmd: &mut EncryptArgs) -> Result<()> {
    let secret_key = decrypt_secret_key(cmd.passphrase_fd, &cmd.secret_key)?;
    let private_key = secret_key.private_key(&cmd.key_id);
    let pks = cmd
        .recipients
        .iter()
        .map(|s| s.parse::<PublicKey>().map_err(anyhow::Error::from))
        .collect::<Result<Vec<PublicKey>>>()?;
    private_key.encrypt(&mut cmd.plaintext, &mut cmd.ciphertext, pks, cmd.fakes, cmd.padding)?;
    Ok(())
}

fn decrypt(cmd: &mut DecryptArgs) -> Result<()> {
    let secret_key = decrypt_secret_key(cmd.passphrase_fd, &cmd.secret_key)?;
    let private_key = secret_key.private_key(&cmd.key_id);
    let sender = cmd.sender.parse::<PublicKey>()?;
    private_key.decrypt(&mut cmd.ciphertext, &mut cmd.plaintext, &sender)?;
    Ok(())
}

fn sign(cmd: &mut SignArgs) -> Result<()> {
    let secret_key = decrypt_secret_key(cmd.passphrase_fd, &cmd.secret_key)?;
    let private_key = secret_key.private_key(&cmd.key_id);
    let sig = private_key.sign(&mut cmd.message)?;
    println!("{}", sig);
    Ok(())
}

fn verify(cmd: &mut VerifyArgs) -> Result<()> {
    let signer = cmd.public_key.parse::<PublicKey>()?;
    let sig: Signature = cmd.signature.parse()?;
    signer.verify(&mut cmd.message, &sig)?;
    Ok(())
}

fn decrypt_secret_key(passphrase_fd: Option<c_int>, path: &Path) -> Result<SecretKey> {
    let passphrase = prompt_passphrase(passphrase_fd)?;
    let ciphertext = fs::read(path)?;
    let sk = SecretKey::decrypt(passphrase.as_bytes(), &ciphertext)?;
    Ok(sk)
}

fn prompt_passphrase(passphrase_fd: Option<c_int>) -> Result<String> {
    match passphrase_fd {
        Some(fd) => {
            let mut buffer = String::new();
            let mut input = FileDescriptor::new(fd);
            input.read_to_string(&mut buffer)?;
            Ok(buffer)
        }
        None => Ok(rpassword::read_password_from_tty(Some("Enter passphrase: "))?),
    }
}