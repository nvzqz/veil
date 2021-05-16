# Key Encapsulation

## Encapsulation

Encapsulation is as follows, given the sender's key pair, $d_S$ and $Q_S$, an ephemeral key pair, $d_E$ and $Q_E$, the
receiver's public key, $Q_R$, a plaintext message $P$, and MAC size $N_M$:

```text
INIT('veil.akem', level=256)
AD(LE_U32(N_M),   meta=true)
AD(Q_R)
AD(Q_S)
```

The static shared secret point is calculated ${ZZ_S}={Q_R}^{d_S}=G^{{d_R}{d_S}}$ and used as a key to encrypt the
ephemeral public key $Q_E$:

```text
KEY(ZZ_S)
SEND_ENC(Q_E) -> E
```

The ephemeral shared secret point is calculated ${ZZ_E}={Q_R}^{d_E}=G^{{d_R}{d_E}}$ and used as a key:

```text
KEY(ZZ_E)
```

This is effectively an authenticated ECDH KEM, but instead of returning KDF output for use in a DEM, we use the keyed
protocol to directly encrypt the ciphertext and create a MAC:

```text
SEND_ENC(P)     -> C
SEND_MAC(N_M)   -> M
```

The resulting ciphertext is the concatenation of $E$, $C$, and $M$.

## Decapsulation

Decapsulation is then the inverse of encryption, given the recipient's key pair, $d_R$ and $Q_R$, and the sender's
public key $Q_S$:

```text
INIT('veil.akem', level=256)
AD(LE_U32(N_M),   meta=true)
AD(Q_R)
AD(Q_S)
```

The static shared secret point is calculated ${ZZ_S}={Q_S}^{d_R}=G^{{d_R}{d_S}}$ and used as a key to decrypt the
ephemeral public key $Q_E$:

```text
KEY(ZZ_S)
RECV_ENC(E) -> Q_E
```

The ephemeral shared secret point is calculated ${ZZ_E}={Q_E}^{d_R}=G^{{d_R}{d_E}}$ and used as a key to decrypt the
plaintext and verify the MAC:

```text
KEY(ZZ_E)
RECV_ENC(C) -> P
RECV_MAC(M)
```

If the `RECV_MAC` call is successful, the ephemeral public key $Q_E$ and the plaintext message $P$ are returned.

## IND-CCA2 Security

This construction combines two overlapping KEM/DEM constructions: a "El Gamal-like" KEM combined with a STROBE-based
AEAD, and an ephemeral ECIES-style KEM combined with a STROBE-based AEAD.

The STROBE-based AEAD is equivalent to Construction 5.6 of Modern Cryptography 3e and is CCA-secure per Theorem 5.7,
provided STROBE's encryption is CPA-secure. STROBE's SEND_ENC is equivalent to Construction 3.31 and is CPA-secure per
Theorem 3.29, provided STROBE is a sufficiently strong pseudorandom function.

The first KEM/DEM construction is equivalent to Construction 12.19 of Modern Cryptography 3e, and is CCA-secure per
Theorem 12.22, provided the gap-CDH problem is hard relative to ristretto255 and STROBE is modeled as a random oracle.

The second KEM/DEM construction is equivalent to Construction 12.23 of Modern Cryptography 3e, and is CCA-secure per
Corollary 12.24, again provided that the gap-CDH problem is hard relative to ristretto255 and STROBE is modeled as a
random oracle.

## IK-CCA Security

`veil.akem` is IK-CCA (per [Bellare][ik-cca]), in that it is impossible for an attacker in possession of two public keys
to determine which of the two keys a given ciphertext was encrypted with in either chosen-plaintext or chosen-ciphertext
attacks. Informally, veil.akem ciphertexts consist exclusively of STROBE ciphertext and PRF output; an attacker being
able to distinguish between ciphertexts based on keying material would imply STROBE's AEAD construction is not IND-CCA2.

Consequently, a passive adversary scanning for encoded points would first need the parties' static Diffie-Hellman secret
in order to distinguish messages from random noise.

## Forward Sender Security

Because the ephemeral private key is discarded after encryption, a compromise of the sender's private key will not
compromise previously-created ciphertexts. If the sender's private key is compromised, the most an attacker can discover
about previously sent messages is the ephemeral public key, not the message itself.

## Insider Authenticity

This construction is not secure against insider attacks on authenticity, nor is it intended to be. A recipient can forge
ciphertexts which appear to be from a sender by re-using the ephemeral public key and encrypting an alternate plaintext,
but the forgeries will only be decryptable by the forger. Because this type of forgery is possible, `veil.akem`
ciphertexts are therefore repudiable.

## Randomness Re-Use

The ephemeral key pair, $d_E$ and $Q_E$, are generated outside of this construction and can be used multiple times for
multiple recipients. This improves the efficiency of the scheme without reducing its security, per Bellare et al.'s
treatment of [Randomness Reusing Multi-Recipient Encryption Schemes][rr-mres].


[ik-cca]: https://iacr.org/archive/asiacrypt2001/22480568.pdf

[rr-mres]: http://cseweb.ucsd.edu/~Mihir/papers/bbs.pdf