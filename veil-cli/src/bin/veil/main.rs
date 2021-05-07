use std::io::{Read, Write};
use std::os::raw::c_int;
use std::path::Path;
use std::{fs, io, mem};

use anyhow::Result;
use filedescriptor::FileDescriptor;
use structopt::StructOpt;

use veil::{PublicKey, SecretKey, Signature};

use crate::cli::{Command, Opts};

mod cli;

fn main() -> Result<()> {
    let cli = Opts::from_args();
    match cli.cmd {
        Command::SecretKey { output } => secret_key(&output),
        Command::PublicKey { secret_key, key_id } => {
            let secret_key = open_secret_key(&secret_key, cli.passphrase_fd)?;
            public_key(secret_key, &key_id)
        }
        Command::DeriveKey {
            public_key,
            sub_key_id,
        } => derive_key(&public_key, &sub_key_id),
        Command::Encrypt {
            secret_key,
            key_id,
            plaintext,
            ciphertext,
            recipients,
            fakes,
            padding,
        } => {
            let secret_key = open_secret_key(&secret_key, cli.passphrase_fd)?;
            encrypt(
                secret_key,
                &key_id,
                &plaintext,
                &ciphertext,
                recipients,
                fakes,
                padding,
            )
        }
        Command::Decrypt {
            secret_key,
            key_id,
            ciphertext,
            plaintext,
            sender,
        } => {
            let secret_key = open_secret_key(&secret_key, cli.passphrase_fd)?;
            decrypt(secret_key, &key_id, &ciphertext, &plaintext, &sender)
        }
        Command::Sign {
            secret_key,
            key_id,
            message,
        } => {
            let secret_key = open_secret_key(&secret_key, cli.passphrase_fd)?;
            sign(secret_key, &key_id, &message)
        }
        Command::Verify {
            public_key,
            message,
            signature,
        } => verify(&public_key, &message, &signature),
    }
}

fn secret_key(output_path: &Path) -> Result<()> {
    let secret_key = SecretKey::new();
    let mut f = open_output(output_path)?;
    let passphrase = rpassword::read_password_from_tty(Some("Enter passphrase: "))?;
    let ciphertext = secret_key.encrypt(passphrase.as_bytes(), 1 << 7, 1 << 10);
    f.write_all(&ciphertext)?;
    Ok(())
}

fn public_key(secret_key: SecretKey, key_id: &str) -> Result<()> {
    let public_key = secret_key.public_key(key_id);
    println!("{}", public_key);
    Ok(())
}

fn derive_key(public_key: &str, key_id: &str) -> Result<()> {
    let root = public_key.parse::<PublicKey>()?;
    let public_key = root.derive(key_id);
    println!("{}", public_key);
    Ok(())
}

fn encrypt(
    secret_key: SecretKey,
    key_id: &str,
    plaintext: &Path,
    ciphertext: &Path,
    recipients: Vec<String>,
    fakes: usize,
    padding: u64,
) -> Result<()> {
    let private_key = secret_key.private_key(key_id);
    let mut plaintext = open_input(plaintext)?;
    let mut ciphertext = open_output(ciphertext)?;
    let pks = recipients
        .into_iter()
        .map(|s| s.parse::<PublicKey>().map_err(anyhow::Error::from))
        .collect::<Result<Vec<PublicKey>>>()?;

    private_key.encrypt(&mut plaintext, &mut ciphertext, pks, fakes, padding)?;

    Ok(())
}

fn decrypt(
    secret_key: SecretKey,
    key_id: &str,
    ciphertext: &Path,
    plaintext_path: &Path,
    sender_ascii: &str,
) -> Result<()> {
    let private_key = secret_key.private_key(key_id);
    let sender = sender_ascii.parse::<PublicKey>()?;
    let mut ciphertext = open_input(ciphertext)?;
    let mut plaintext = open_output(plaintext_path)?;

    if let Err(e) = private_key.decrypt(&mut ciphertext, &mut plaintext, &sender) {
        if plaintext_path != Path::new("-") {
            mem::drop(plaintext);
            fs::remove_file(plaintext_path)?;
        }
        return Err(anyhow::Error::from(e));
    }

    Ok(())
}

fn sign(secret_key: SecretKey, key_id: &str, message: &Path) -> Result<()> {
    let private_key = secret_key.private_key(key_id);
    let mut message = open_input(message)?;

    let sig = private_key.sign(&mut message)?;
    println!("{}", sig);

    Ok(())
}

fn verify(signer: &str, message: &Path, signature: &str) -> Result<()> {
    let signer = signer.parse::<PublicKey>()?;
    let sig: Signature = signature.parse()?;
    let mut message = open_input(message)?;
    signer.verify(&mut message, &sig)?;
    Ok(())
}

fn open_input(path: &Path) -> Result<Box<dyn io::Read>> {
    Ok(if path == Path::new("-") {
        Box::new(io::stdin())
    } else {
        Box::new(fs::File::open(path)?)
    })
}

fn open_output(path: &Path) -> Result<Box<dyn io::Write>> {
    Ok(if path == Path::new("-") {
        Box::new(io::stdout())
    } else {
        Box::new(fs::File::create(path)?)
    })
}

fn open_secret_key(path: &Path, passphrase_fd: Option<c_int>) -> Result<SecretKey> {
    let passphrase = match passphrase_fd {
        Some(fd) => {
            let mut buffer = String::new();
            let mut input = FileDescriptor::new(fd);
            input.read_to_string(&mut buffer)?;
            buffer
        }
        None => rpassword::read_password_from_tty(Some("Enter passphrase: "))?,
    };
    let ciphertext = fs::read(path)?;
    let sk = SecretKey::decrypt(passphrase.as_bytes(), &ciphertext)?;
    Ok(sk)
}