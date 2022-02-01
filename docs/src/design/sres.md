# Single-recipient Messages

`veil.sres` implements a single-recipient, deniable signcryption scheme which produces ciphertexts indistinguishable
from random noise.

## Encryption

Encryption takes a sender's key pair, $(d_S, Q_S)$, a recipient's public key, $Q_R$, a plaintext message $P$, and a
shared secret length $N_K$.

First, the protocol is initialized and the sender and recipient's public keys are sent and received, respectively:

```text
INIT('veil.sres', level=128)

AD('sender-public-key', meta=true)
AD(LE_U64(LEN(Q_S)),    meta=true, more=true)
SEND_CLR(Q_S)

AD('receiver-public-key', meta=true)
AD(LE_U64(LEN(Q_R)),      meta=true, more=true)
RECV_CLR(Q_R)
```

Second, the Diffie-Hellman shared secret point $Z=[d_S]Q_R=[d_Sd_R]G$ is used to key the protocol:

```text
AD('dh-shared-secret', meta=true)
AD(LE_U64(LEN(Z)),     meta=true, more=true)
KEY(Z)
```

Third, the plaintext $P$ is encapsulated with [`veil.akem`](akem.md) using $(d_S, Q_S, Q_R)$, yielding the shared secret
$k$, the challenge scalar $r$, and the proof scalar $s$. $r$ and $s$ are encrypted and sent as $S_0$ and $S_1$, and the
protocol is keyed with $k$:

```text
AD('challenge-scalar', meta=true)
AD(LE_U64(LEN(r)),     meta=true, more=true)
SEND_ENC(r) -> S_0

AD('proof-scalar', meta=true)
AD(LE_U64(LEN(s)), meta=true, more=true)
SEND_ENC(s) -> S_1

AD('akem-shared-secret', meta=true)
AD(LE_U64(LEN(N_K)),     meta=true, more=true)
KEY(k)
```

Finally, the plaintext $P$ is encrypted and sent as ciphertext $C$, and a MAC $M$ is generated and sent:

```text
AD('plaintext',    meta=true)
AD(LE_U64(LEN(r)), meta=true, more=true)
SEND_ENC(P) -> C

AD('mac',       meta=true)
AD(LE_U64(N_M), meta=true, more=true)
SEND_MAC(N_M) -> M
```

The final ciphertext is $S_0 || S_1 || C || M$.

## Decryption

Encryption takes a recipient's key pair, $(d_R, Q_R)$, a sender's public key, $Q_S$, two encrypted scalars $(S_0, S_1)$,
a ciphertext $C$, and a MAC $M$.

First, the protocol is initialized and the sender and recipient's public keys are received and sent, respectively:

```text
INIT('veil.sres', level=128)

AD('sender-public-key', meta=true)
AD(LE_U64(LEN(Q_S)),    meta=true, more=true)
RECV_CLR(Q_S)

AD('receiver-public-key', meta=true)
AD(LE_U64(LEN(Q_R)),      meta=true, more=true)
SEND_CLR(Q_R)
```

Second, the Diffie-Hellman shared secret point $Z=[d_R]Q_S=[d_Rd_S]G$ is used to key the protocol:

```text
AD('dh-shared-secret', meta=true)
AD(LE_U64(LEN(Z)),     meta=true, more=true)
KEY(Z)
```

Third, the challenge scalar $r$ and the proof scalar $s$ are decrypted:

```text
AD('challenge-scalar', meta=true)
AD(LE_U64(LEN(S_0)),     meta=true, more=true)
RECV_ENC(S_0) -> r

AD('proof-scalar', meta=true)
AD(LE_U64(LEN(S_1)), meta=true, more=true)
SEND_ENC(S_1) -> s
```

Fourth, the scalars are decapsulated with [`veil.akem`](akem.md) using $(d_R, Q_R, Q_S)$, returning a shared secret $k$
and a verification context $V$. The protocol is keyed with the shared secret $k$:

```text
AD('akem-shared-secret', meta=true)
AD(LE_U64(LEN(N_K)),     meta=true, more=true)
KEY(k)
```

Fifth, the ciphertext $C$ is decrypted and the MAC $M$ is verified:

```text
AD('plaintext',    meta=true)
AD(LE_U64(LEN(r)), meta=true, more=true)
RECV_ENC(C) -> P

AD('mac',       meta=true)
AD(LE_U64(N_M), meta=true, more=true)
RECV_MAC(M)
```

If the `RECV_MAC` call is unsuccessful, an error is returned.

Finally, the [`veil.akem`](akem.md) verification context $V$ is called with the challenge scalar $r$ and the decrypted
plaintext $P$. If the plaintext is verified, $P$ is returned.

## IND-CCA2 Security

This construction combines two overlapping KEM/DEM constructions: an "El Gamal-like" KEM combined with a STROBE-based
AEAD, and a hybrid signcryption KEM combined with a STROBE-based AEAD.

The STROBE-based AEAD is equivalent to Construction 5.6 of _Modern Cryptography 3e_ and is CCA-secure per Theorem 5.7,
provided STROBE's encryption is CPA-secure. STROBE's `SEND_ENC` is equivalent to Construction 3.31 and is CPA-secure per
Theorem 3.29, provided STROBE is a sufficiently strong pseudorandom function.

The first KEM/DEM construction is equivalent to Construction 12.19 of _Modern Cryptography 3e_, and is CCA-secure per
Theorem 12.22, provided the gap-CDH problem is hard relative to ristretto255 and STROBE is modeled as a random oracle.

The second KEM/DEM construction is detailed in the [`veil.akem`](akem.md) documentation and is CCA-secure.

## IK-CCA Security

`veil.sres` is IK-CCA (per [Bellare][ik-cca]), in that it is impossible for an attacker in possession of two public keys
to determine which of the two keys a given ciphertext was encrypted with in either chosen-plaintext or chosen-ciphertext
attacks. Informally, `veil.sres` ciphertexts consist exclusively of STROBE ciphertext and PRF output; an attacker being
able to distinguish between ciphertexts based on keying material would imply STROBE's AEAD construction is not IND-CCA2.

Consequently, a passive adversary scanning for encoded points would first need the parties' static Diffie-Hellman secret
in order to distinguish messages from random noise.

## Forward Sender Security

Because [`veil.akem`](akem.md) encapsulation is forward-secure for senders, so are all encrypted values after the
protocol is keyed with the shared secret $k$. A sender (or an attacker in possession of the sender's private key) will
be able to recover the two scalars, $(r, s)$, but not the plaintext.

[ik-cca]: https://iacr.org/archive/asiacrypt2001/22480568.pdf