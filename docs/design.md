# The Veil Cryptosystem

Veil is a public-key cryptosystem that provides confidentiality, authenticity, and integrity
services for messages of arbitrary sizes and multiple receivers. This document describes its
cryptographic constructions, their security properties, and how they are combined to implement
Veil's feature set.

## Motivation

Veil is a clean-slate effort to build a secure, asynchronous, PGP-like messaging cryptosystem using
modern tools and techniques to resist new attacks from modern adversaries. PGP provides confidential
and authentic multi-receiver messaging, but with many deficiencies.

### Cryptographic Agility

PGP was initially released in 1991 using a symmetric algorithm called BassOmatic invented by Phil
Zimmerman himself. Since then, it's supported IDEA, DES, Triple-DES, CAST5, Blowfish, SAFER-SK128,
AES-128, AES-192, AES-256, Twofish, and Camellia, with proposed support for ChaCha20. For hash
algorithms, it's supported MD5, SHA-1, RIPE-MD160, MD2, "double-width SHA", TIGER/192, HAVAL,
SHA2-256, SHA2-384, SHA2-512, SHA2-224. For public key encryption, it's supported RSA, ElGamal,
Diffie-Hellman, and ECDH, all with different parameters. For digital signatures, it's supported RSA,
DSA, ElGamal, ECDSA, and EdDSA, again, all with different parameters.

As Adam Langley  said regarding TLS [[Lan16]](#lan16):

> Cryptographic agility is a huge cost. Implementing and supporting multiple algorithms means more
code. More code begets more bugs. More things in general means less academic focus on any one thing,
and less testing and code-review per thing. Any increase in the number of options also means more
combinations and a higher chance for a bad interaction to arise.

At best, each of these algorithms represents a geometrically increasing burden on implementors,
analysts, and users. At worst, they represent a catastrophic risk to the security of the
system [[Ngu04]](#ngu04) [[BSW21]](#bsw21).

A modern system would use a limited number of cryptographic primitives and use a single instance of
each.

### Informal Constructions

PGP messages use a Sign-Then-Encrypt (StE) construction, which is insecure given an encryption
oracle ([[AR10]](#ar10), p. 41):

> In the StE scheme, the adversary `A` can easily break the sUF-CMA security in the outsider model.
It can ask the encryption oracle to signcrypt a message `m` for `R'` and get `C=(Encrypt(pk_R',
m||σ),ID_S,ID_R')` where `σ=Sign(pk_S, m)`.  Then, it can recover `m||σ` using `sk_R'` and forge the
signcryption ciphertext `C=(Encrypt(pk_R, m||σ),ID_S,ID_R)`.

This may seem like an academic distinction, but this attack is trivial to mount. If you send your
boss an angry resignation letter signed and encrypted with PGP, your boss can re-transmit that to
your future boss, encrypted with her public key.

A modern system would use established, analyzed constructions with proofs in established models to
achieve established notions with reasonable reductions to weak assumptions.

### Non-Repudiation

A standard property of digital signatures is that of _non-repudiation_, or the inability of the
signer to deny they signed a message. Any possessor of the signer's public key, a message, and a
signature can verify the signature for themselves. For explicitly signed, public messages, this is a
very desirable property. For encrypted, confidential messages, this is not.

Similar to the vindictive boss scenario above, an encrypted-then-signed PGP message can be decrypted
by an intended receiver (or someone in possession of their private key) and presented to a third
party as an unencrypted, signed message without having to reveal anything about themselves. The
inability of PGP to preserve the privacy context of confidential messages should rightfully have a
chilling effect on its users [[BGB04]](#bgb04).

A modern system would be designed to provide some level of deniability to confidential messages.

### Global Passive Adversaries

A new type of adversary which became immediately relevant to the post-Snowden era is the Global
Passive Adversary, which monitors all traffic on all links of a network. For an adversary with an
advantaged network position (e.g. a totalitarian state), looking for cryptographically-protected
messages is trivial given the metadata they often expose. Even privacy features like GnuPG's
`--hidden-recipients` still produce encrypted messages which are trivially identifiable as encrypted
messages, because PGP messages consist of packets with explicitly identifiable metadata.In
addition to being secure, privacy-enhancing technologies must be undetectable.

Bernstein summarized this dilemma [[BHKL13]](#bhkl13):

> Cryptography hides patterns in user data but does not evade censorship if the censor can recognize
patterns in the cryptography itself.

A modern system would produce messages without recognizable metadata or patterns.

## Security Model And Notions

### Multi-User Confidentiality

To evaluate the confidentiality of a scheme, we consider an adversary `A` attempting to attack a
sender and receiver ([[BS10]](#bs10), p. 44). `A` creates two equal-length messages `(m_0, m_1)`,
the sender selects one at random and encrypts it, and `A` guesses which of the two has been
encrypted without tricking the receiver into decrypting it for them. To model real-world
possibilities, we assume `A` has three capabilities:

1. `A` can create their own key pairs. Veil does not have a centralized certificate authority and
   creating new key pairs is intentionally trivial.
2. `A` can trick the sender into encrypting arbitrary plaintexts with arbitrary public keys. This
    allows us to model real-world flaws such as servers which return encrypted error messages with
    client-provided data [[YHR04]](#yhr04).
3. `A` can trick the receiver into decrypting arbitrary ciphertexts from arbitrary senders. This
    allows us to model real-world flaws such as padding oracles [[RD10]](#rd10).

Given these capabilities, `A` can mount an attack in two different settings: the outsider setting
and the insider setting.

#### Outsider Confidentiality

In the multi-user outsider model, we assume `A` knows the public keys of all users but none of their
private keys ([[BS10]](#bs10), p. 44).

The multi-user outsider model is useful in evaluating the strength of a scheme against adversaries
who have access to some aspect of the sender and receiver's interaction with messages (e.g. a
padding oracle) but who have not compromised the private keys of either.

#### Insider Confidentiality

In the multi-user insider model, we assume `A` knows the sender's private key in addition to the
public keys of both users ([[BS10]](#bs10), p.45-46).

The multi-user insider model is useful in evaluating the strength of a scheme against adversaries
who have compromised a user.

##### Forward Sender Security

A scheme which provides confidentiality in the multi-user insider setting is called _forward sender
secure_ because an adversary who compromises a sender cannot read messages that sender has
previously encrypted [[CHK03]](#chk03).

### Multi-User Authenticity

To evaluate the authenticity of a scheme, we consider an adversary `A` attempting to attack a sender
and receiver ([[BS10]](#bs10) p. 47). `A` attempts to forge a ciphertext which the receiver will
decrypt but which the sender never encrypted. To model real-world possibilities, we again assume `A`
has three capabilities:

1. `A` can create their own key pairs.
2. `A` can trick the sender into encrypting arbitrary plaintexts with arbitrary public keys.
3. `A` can trick the receiver into decrypting arbitrary ciphertexts from arbitrary senders.

As with multi-user confidentiality, this can happen in the outsider setting and the insider setting.

#### Outsider Authenticity

In the multi-user outsider model, we again assume `A` knows the public keys of all users but none of
their private keys ([[BS10]](#bs10), p. 47).

Again, this is useful to evaluate the strength of a scheme in which `A` has some insight into
senders and receivers but has not compromised either.

#### Insider Authenticity

In the multi-user insider model, we assume `A` knows the receiver's private key in addition to the
public keys of both users ([[BS10]](#bs10), p. 47).

##### Key Compromise Impersonation

A scheme which provides authenticity in the multi-user insider setting effectively resists _key
compromise impersonation_, in which `A`, given knowledge of a receiver's private key, can forge
messages to that receiver from arbitrary senders [Str06](#str06). The classic example is
authenticated Diffie-Hellman (e.g. [[RFC9180]](#rfc9180) [[ABHKLR21]](#abhklr21)), in which the
static Diffie-Hellman shared secret point `K=[d_S]Q_R` is used to encrypt a message and its
equivalent `K'=[d_R]Q_S` is used to decrypt it. An attacker in possession of the receiver's private
key `d_R` and the sender's public key `Q_S` can simply encrypt the message using `K'=[d_R]Q_S`
without ever having knowledge of `d_S`. Digital signatures are a critical element of schemes which
provide insider authenticity, as they give receivers a way to verify the authenticity of a message
using authenticators they (or an adversary with their private key) could never construct themselves.

### Insider vs. Outsider Security

The multi-receiver setting motivates a focus on insider security over the traditional emphasis on
outsider security (contra [[AR10]](#ar10) p. 26, [[BS10]](#bs10) p. 46; see [[BBM18]](#bbm18)).
Given a probability of an individual key compromise `P`, a multi-user system of `N` users has an
overall `1-{(1-P)}^N` probability of at least one key being compromised. A system with an
exponentially increasing likelihood of losing all confidentiality and authenticity properties is not
acceptable.

### Indistinguishable From Random Noise

Indistinguishability from random noise is a critical property for censorship-resistant
communication [[BHKL13]](#bhkl13):

> Censorship-circumvention tools are in an arms race against censors. The censors study all traffic
passing into and out of their controlled sphere, and try to disable censorship-circumvention tools
without completely shutting down the Internet. Tools aim to shape their traffic patterns to match
unblocked programs, so that simple traffic profiling cannot identify the tools within a reasonable
number of traces; the censors respond by deploying firewalls with increasingly sophisticated
deep-packet inspection.
>
> Cryptography hides patterns in user data but does not evade censorship
if the censor can recognize patterns in the cryptography itself.

### Limited Deniability

The inability of a receiver (or an adversary in possession of a receiver's private key) to prove the
authenticity of a message to a third party is critical for privacy. Other privacy-sensitive
protocols achieve this by forfeiting insider authenticity or authenticity altogether
[[BGB04]](#bgb04). Veil achieves a limited version of deniability: a receiver can only prove the
authenticity of a message to a third party by revealing their own private key. This deters a
dishonest receiver from selectively leaking messages and requires all-or-nothing disclosure from an
adversary who compromises an honest receiver.

## Cryptographic Primitives

In the interests of cryptographic minimalism, Veil uses just two distinct cryptographic primitives:

1. Keccyak-Min for confidentiality, authentication, and integrity.
2. jq255e for key agreement and authenticity [[Por22]](#por22).

### Keccyak-Min

Keccyak-Min is the adaptation of the Cyclist duplex construction from Xoodyak [[DHPVV20]](#dhpvv20)
to the Keccak-_p_\[1600,10\] permutation instead of Xoodoo (thus "Keccyak").

Cyclist is a permutation-based cryptographic duplex, a cryptographic primitive that provides
symmetric-key confidentiality, integrity, and authentication via a single object
[[DHPVV20]](#dhpvv20). Duplexes offer a way to replace complex, ad-hoc constructions combining
encryption algorithms, cipher modes, AEADs, MACs, and hash algorithms using a single primitive
[[DHPVV20]](#dhpvv20) [[BDPV11]](#bdpv11). Duplexes have security properties which reduce to the
properties of the cryptographic sponge, which themselves reduce to the strength of the underlying
permutation [[BDPV08]](#bdpv08).

The Keccak-_p_\[1600,10\] permutation (a.k.a. KitTen) is the fastest Keccak-_p_ variant which still
maintains a reasonable margin of security [[Aum19]](#aum19). It has a 1600-bit width, like
Keccak-_f_\[1600\] (the basis of SHA-3), but with a reduced number of rounds for speed.This allows
for much higher throughput in software than the Xoodoo permutation at the expense of requiring a
larger state. Xoodyak's Cyclist parameters of `R_hash=b-256`, `R_kin=b-32`, `R_kout=b-192`, and
`L_ratchet=16` are adapted for the larger permutation width of `b=1600`. The resulting construction
targets a 128-bit security level and is ~4x faster than SHA-256, ~5x faster than ChaCha20Poly1305,
and ~7.5x faster than AES-128-GCM in software.

Veil's security assumes that Cyclist's `Encrypt` operation is IND-CPA secure, its `Squeeze`
operation is sUF-CMA secure, and its `Encrypt`/`Squeeze`-based authenticated encryption construction
is IND-CCA2 secure.

### jq255e

jq255e is a double-odd elliptic curve selected for efficiency and simplicity [[Por20]](#por20)
[[Por22]](#por22). It provides a prime-order group, has non-malleable encodings, and has no
co-factor concerns. This allows for the use of a wide variety of cryptographic constructions built
on group operations. It targets a 128-bit security level, lends itself to constant-time
implementations, and can run in constrained environments.

Veil's security assumes that the Gap Discrete Logarithm and Gap Diffie-Hellman problems are hard
relative to jq255e.

## Construction Techniques

Veil uses a few common construction techniques in its design which bear specific mention.

### Unkeyed And Keyed Duplexes

Veil uses Cyclist, which offers both unkeyed ("hash") and keyed modes. All Veil constructions begin
in the unkeyed mode by absorbing a constant domain separation string (e.g. `veil.mres`). To convert
from an unkeyed duplex to a keyed duplex, a 512-bit key is derived from the unkeyed duplex's state
and used to initialize a keyed duplex:

```text
function EncryptExample(x, p):
  Absorb("example.encrypt") // Initialize an unkeyed duplex.
  Absorb(x)                 // Absorb some key material.
  k ← SqueezeKey(64)        // Squeeze a 512-bit key.
  Cyclist(k, ϵ, ϵ)          // Initialize a keyed duplex with the derived key.
  c ← Encrypt(p)            // Encrypt the plaintext.
  return c
```

The unkeyed duplex is used as a kind of key derivation function, with the lower absorb rate of
Cyclist's unkeyed mode providing better avalanching properties.

### Integrated Constructions

Cyclist is a cryptographic duplex, thus each operation is cryptographically dependent on the
previous operations. Veil makes use of this by integrating different types of constructions to
produce a single, unified construction. Instead of having to pass forward specific values (e.g.
hashes of values or derived keys) to ensure cryptographic dependency, Cyclist allows for
constructions which simply absorb all values, thus ensuring transcript integrity of complex
protocols.

For example, a traditional hybrid encryption scheme like HPKE [[RFC9180]](#rfc9180) will describe a
key encapsulation mechanism (KEM) like X25519 and a data encapsulation mechanism (DEM) like AES-GCM
and link the two together via a key derivation function (KDF) like HKDF by deriving a key and nonce
for the DEM from the KEM output.

In contrast, the same construction using Cyclist would be the following three operations, in order:

```text
function HPKE(d_E, Q_R, p):
  Cyclist([d_E]Q_R, ϵ, ϵ) // Initialize a keyed duplex with the shared secret.
  c ← Encrypt(p)          // Encrypt the plaintext.
  t ← Squeeze(16)         // Squeeze an authentication tag.
  return c || t           // Return ciphertext and tag.
```

The duplex is keyed with the shared secret point, used to encrypt the plaintext, and finally used to
squeeze an authentication tag. Each operation modifies the duplex's state, making the final
`Squeeze` operation's output dependent on both the previous `Encrypt` operation (and its argument,
`p`) but also the `Cyclist` operation before it.

This is both a dramatically clearer way of expressing the overall hybrid public-key encryption
construction and more efficient: because the ephemeral shared secret point is unique, no nonce need
be derived (or no all-zero nonce need be justified in an audit).

#### Process History As Hidden State

A subtle but critical benefit of integrating constructions via a cryptographic duplex is that
authenticators produced via `Squeeze` operations are dependent on the entire process history of the
duplex, not just on the emitted ciphertext. The DEM components of our HPKE analog (i.e.
`Encrypt`/`Squeeze`) are superficially similar to an Encrypt-then-MAC (EtM) construction, but where
an adversary in possession of the MAC key can forge authenticators given an EtM ciphertext, the
duplex-based approach makes that infeasible. The output of the `Squeeze` operation is dependent not
just on the keying material (i.e. the `Cyclist` operation) but also on the plaintext `p`. An
adversary attempting to forge an authenticator given only key material and ciphertext will be unable
to reconstruct the duplex's state and thus unable to compute their forgery.

### Hedged Ephemeral Values

When generating ephemeral values, Veil uses Aranha et al.'s "hedged signature" technique to mitigate
against both catastrophic randomness failures and differential fault attacks against purely
deterministic schemes [[AOTZ20]](#aotz20).

Specifically, the duplex's state is cloned, and the clone absorbs a context-specific secret value
(e.g. the signer's private key in a digital signature scheme) and a 64-byte random value. The clone
duplex is used to produce the ephemeral value or values for the scheme.

For example, the following operations would be performed on the cloned duplex:

```text
function HedgedScalar(d):
  with clone do           // Clone the duplex's current state.
    Absorb(d)             // Absorb the private value.
    v ← Rand(64)          // Generate a 64-byte random value.
    Absorb(v)             // Absorb the random value.
    x ← Squeeze(32) mod q // Squeeze a scalar from the cloned duplex.
    return x              // Return the scalar to the outer context.
  end clone               // Destroy the cloned duplex's state.
```

The ephemeral scalar `x` is returned to the context of the original construction and the cloned
duplex is discarded. This ensures that even in the event of a catastrophic failure of the random
number generator, `x` is still unique relative to `d`. Depending on the uniqueness needs of the
construction, an ephemeral value can be hedged with a plaintext in addition to a private key.

For brevity, a hedged ephemeral value `x` derived from a private input value `y` is denoted as
`x ← Hedge(y, Squeeze(32) mod q)`.

## Digital Signatures

`veil.schnorr` implements a Schnorr digital signature scheme.

### Signing A Message

Signing a message requires a signer's private key `d` and a message `m` of arbitrary length.

```text
function Sign(d, m):
  Absorb("veil.schnorr")          // Initialize an unkeyed duplex.
  Absorb([d]G)                    // Absorb the signer's public key.
  Absorb(m)                       // Absorb the message.
  Cyclist(SqueezeKey(64), ϵ, ϵ)   // Convert to a keyed duplex.
  k ← Hedge(d, Squeeze(32) mod q) // Squeeze a hedged commitment scalar.
  I ← [k]G                        // Calculate the commitment point.
  S_0 ← Encrypt(I)                // Encrypt the commitment point.
  r ← Squeeze(16)                 // Squeeze a short challenge scalar.
  s ← d×️r + k                     // Calculate the proof scalar.
  S_1 ← Encrypt(s)                // Encrypt the proof scalar.
  return S_0 || S_1               // Return the commitment point and proof scalar.
```

### Verifying A Signature

Verifying a signature requires a signer's public key `Q`, a message `m`, and a signature
`S_0 || S_1`.

```text
function Verify(Q, m, S_0 || S_1):
  Absorb("veil.schnorr")          // Initialize an unkeyed duplex.
  Absorb(Q)                       // Absorb the signer's public key.
  Absorb(m)                       // Absorb the message.
  Cyclist(SqueezeKey(64), ϵ, ϵ)   // Convert to a keyed duplex.
  I ← Decrypt(S_0)                // Decrypt the commitment point.
  r' ← Squeeze(16)                // Squeeze a counterfactual challenge scalar.
  s ← Decrypt(S_1)                // Decrypt the proof scalar.
  I' ← [s]G - [r']Q               // Calculate the counterfactual commitment scalar.
  return I = I'                   // The signature is valid if both points are equal.
```

### Constructive Analysis Of `veil.schnorr`

The Schnorr signature scheme is the application of the Fiat-Shamir transform to the Schnorr
identification scheme.

Unlike Construction 13.12 of [[KL20]](#kl20) (p. 482), `veil.schnorr` transmits the commitment point
`I` as part of the signature and the verifier calculates `I'` vs transmitting the challenge scalar
`r` and calculating `r'`. In this way, `veil.schnorr` is closer to EdDSA [[BCJZ21]](#bcjz21)  or the
Schnorr variant proposed by Hamburg [[Ham17]](#ham17). Short challenge scalars are used which allow
for faster verification with no loss in security [[Por22]](#por22). In addition, this construction
allows for the use of variable-time optimizations during signature verification
[[Por20+1]](#por201).

### UF-CMA Security

Per Theorem 13.10 of [[KL20]](#kl20) (p. 478), this construction is UF-CMA secure if the Schnorr
identification scheme is secure and the hash function is secure:

> Let `Π` be an identification scheme, and let `Π'` be the signature scheme that results by applying
the Fiat-Shamir transform to it. If `Π` is secure and `H` is modeled as a random oracle, then `Π'`
is secure.

Per Theorem 13.11 of [[KL20]](#kl20) (p. 481), the security of the Schnorr identification scheme is
conditioned on the hardness of the discrete logarithm problem:

> If the discrete-logarithm problem is hard relative to `G`, then the Schnorr identification scheme
is secure.

Per Sec 5.10 of [[BDPV11+1]](#bdpv111), Cyclist is a suitable random oracle if the underlying
permutation is indistinguishable from a random permutation. Thus, `veil.schnorr` is UF-CMA if the
discrete-logarithm problem is hard relative to jq255e and Keccak-_p_ is indistinguishable from a
random permutation.

### sUF-CMA Security

Some Schnorr/EdDSA implementations (e.g. Ed25519) suffer from malleability issues, allowing for
multiple valid signatures for a given signer and message [[BCJZ21]](#bcjz21). [[CGN20]](#cgn20)
describe a strict verification function for Ed25519 which achieves sUF-CMA security in addition to
strong binding:

1. Reject the signature if `S ∉ {0,…,L-1}`.
2. Reject the signature if the public key `A` is one of 8 small order points.
3. Reject the signature if `A` or `R` are non-canonical.
4. Compute the hash `SHA2_512(R||A||M)` and reduce it mod `L` to get a scalar `h`.
5. Accept if `8(S·B)-8R-8(h·A)=0`.

Rejecting `S≥L` makes the scheme sUF-CMA secure, and rejecting small order `A` values makes the
scheme strongly binding. `veil.schnorr`'s use of canonical point and scalar encoding routines
obviate the need for these checks. Likewise, jq255e is a prime order group, which obviates the need
for cofactoring in verification.

When implemented with a prime order group and canonical encoding routines, the Schnorr signature
scheme is strongly unforgeable under chosen message attack (sUF-CMA) in the random oracle model and
even with practical cryptographic hash functions [[PS00]](#ps00) [[NSW09]](#nsw09).

### Key Privacy

The EdDSA variant (i.e. `S=(I,s)` ) is used over the traditional Schnorr construction (i.e.
`S=(r,s)`) to enable the variable-time computation of `I'=[s]G - [r]Q`, which provides a ~30%
performance improvement. That construction, however, allows for the recovery of the signing public
key given a signature and a message: given the commitment point `I`, one can calculate
`Q=-[r^-1](I - [s]G)`.

For Veil, this behavior is not desirable. A global passive adversary should not be able to discover
the identity of a signer from a signed message.

To eliminate this possibility, `veil.schnorr` encrypts both components of the signature with a
duplex keyed with the signer's public key in addition to the message. An attack which recovers the
plaintext of either signature component in the absence of the public key would imply that Cyclist is
not IND-CPA.

### Indistinguishability From Random Noise

Given that both signature components are encrypted with Cyclist, an attack which distinguishes
between a `veil.schnorr` and random noise would also imply that Cyclist is not IND-CPA.

## Encrypted Headers

`veil.sres` implements a single-receiver, deniable signcryption scheme which Veil uses to encrypt
message headers. It integrates an ephemeral ECDH KEM, a Cyclist DEM, and a designated-verifier
Schnorr signature scheme to provide multi-user insider security with limited deniability.

### Encrypting A Header

Encrypting a header requires a sender's private key `d_S`,  an ephemeral private key `d_E`, the
receiver's public key `Q_R`, a nonce `N`, and a plaintext `P`.

```text
function EncryptHeader(d_S, d_E, Q_R, N, P):
  Absorb("veil.sres")             // Initialize an unkeyed duplex.
  Absorb([d_S]G)                  // Absorb the sender's public key.
  Absorb(Q_R)                     // Absorb the receiver's public key.
  Absorb(N)                       // Absorb the nonce.
  Absorb([d_S]Q_R)                // Absorb the static ECDH shared secret.
  Cyclist(SqueezeKey(64), ϵ, ϵ)   // Initialize a keyed duplex with the derived key.
  C_0 ← Encrypt([d_E]G)           // Encrypt the ephemeral public key.
  Absorb([d_E]Q_R)                // Absorb the ephemeral ECDH shared secret.
  C_1 ← Encrypt(P)                // Encrypt the plaintext.
  k ← Hedge(d, Squeeze(32) mod q) // Squeeze a hedged commitment scalar.
  I ← [k]G                        // Calculate the commitment point.
  S_0 ← Encrypt(I)                // Encrypt the commitment point.
  r ← Squeeze(32) mod q           // Squeeze a challenge scalar.
  s ← d_S✕r + k                   // Calculate the proof scalar.
  X ← [s]Q_R                      // Calculate the proof point.
  S_1 ← Encrypt(X)                // Encrypt the proof point.
  return C_0 || C_1 || S_0 || S_1
```

### Decrypting A Header

Decrypting a header requires a receiver's private key `d_R`, the sender's public key `Q_R`, a nonce
`N`, and a ciphertext `C_0 || C_1 || S_0 || S_1`.

```text
function DecryptHeader(d_R, Q_S, N, C_0 || C_1 || S_0 || S_1):
  Absorb("veil.sres")             // Initialize an unkeyed duplex.
  Absorb(Q_S)                     // Absorb the sender's public key.
  Absorb([d_R]G)                  // Absorb the receiver's public key.
  Absorb(N)                       // Absorb the nonce.
  Absorb([d_R]Q_S)                // Absorb the static ECDH shared secret.
  Cyclist(SqueezeKey(64), ϵ, ϵ)   // Initialize a keyed duplex with the derived key.
  Q_E ← Decrypt(C_0)              // Decrypt the ephemeral public key.
  Absorb([d_R]Q_E)                // Absorb the ephemeral ECDH shared secret.
  P ← Decrypt(C_1)                // Decrypt the ciphertext.
  I ← Encrypt(S_0)                // Decrypt the commitment point.
  r' ← Squeeze(32) mod q          // Squeeze a counterfactual challenge scalar.
  X ← Encrypt(S_1)                // Decrypt the proof point.
  X' ← [d_R](I + [r']Q_S)         // Calculate a counterfactual proof point.
  if X ≠ X':                      // Return an error if the points are not equal.
    return ⊥
  return (Q_E, P)                 // Otherwise, return the ephemeral public key and plaintext.
```

### Constructive Analysis Of `veil.sres`

`veil.sres` is an integration of two well-known constructions: an ECIES-style hybrid public key
encryption scheme and a designated-verifier Schnorr signature scheme.

The initial portion of `veil.sres` is equivalent to ECIES (see Construction 12.23 of
[[KL20]](#kl20), p.  435), (with the commitment point `I` as an addition to the ciphertext, and
the challenge scalar `r` serving as the authentication tag for the data encapsulation mechanism) and
is IND-CCA2 secure (see Corollary 12.14 of [[KL20]](#kl20), p. 436).

The latter portion of `veil.sres` is a designated-verifier Schnorr signature scheme which adapts an
EdDSA-style Schnorr signature scheme by multiplying the proof scalar `s` by the receiver's public
key `Q_R` to produce a designated-verifier point `X` [[SWP04]](#swp04). The EdDSA-style Schnorr
signature is sUF-CMA secure when implemented in a prime order group and a cryptographic hash
function [[BCJZ21]](#bcjz21) [[CGN20]](#cgn20) [[PS00]](#ps00) [[NSW09]](#nsw09) (see also
[`veil.schnorr`](#digital-signatures).

### Multi-User Confidentiality Of Headers

One of the two main goals of the `veil.sres` is confidentiality in the multi-user setting (see
[Multi-User Confidentiality](#multi-user-confidentiality)), or the inability of an adversary `A` to
learn information about plaintexts.

#### Outsider Confidentiality Of Headers

First, we evaluate the confidentiality of `veil.sres` in the multi-user outsider setting (see
[Outsider Confidentiality](#outsider-confidentiality)), in which the adversary `A` knows the public
keys of all users but none of their private keys ([[BS10]](#bs10), p. 44).

The classic multi-user attack on the generic Encrypt-Then-Sign (EtS) construction sees `A` strip the
signature `σ` from the challenge ciphertext `C=(c,σ,Q_S,Q_R)` and replace it with `σ ← Sign(d_A,c)`
to produce an attacker ciphertext `C'=(c,σ',Q_{\Adversary},Q_R)` at which point `A` can trick the
receiver into decrypting the result and giving `A` the randomly-chosen plaintext `m_0 ∨ m_1`
[[AR10]](#ar10). This attack is not possible with `veil.sres`, as the sender's public key is
strongly bound during encryption and decryption.

`A` is unable to forge valid signatures for existing ciphertexts, limiting them to passive
attacks. A passive attack on any of the three components of `veil.sres` ciphertexts--`C`,
`S_0`, `S_1`--would only be possible if Cyclist is not IND-CPA secure.

Therefore, `veil.sres` provides confidentiality in the multi-user outsider setting.

#### Insider Confidentiality Of Headers

Next, we evaluate the confidentiality of `veil.sres` in the multi-user insider setting (see
[Insider Confidentiality](#insider-confidentiality), in which the adversary `A` knows the sender's
private key in addition to the public keys of both users ([[BS10]](#bs10), p. 45-46).

`A` cannot decrypt the message by themselves, as they do not know either `d_E` or `d_R` and
cannot calculate the ECDH shared secret `[d_E]Q_R=[d_R]Q_E=[d_E{d_R}G]`.

`A` also cannot trick the receiver into decrypting an equivalent message by replacing the signature,
despite `A`'s ability to use `d_S` to create new signatures. In order to generate a valid signature
on a ciphertext `c'` (e.g.\ `c'=c||1`), `A` would have to squeeze a valid challenge scalar `r'` from
the duplex state. Unlike the signature hash function in the generic EtS composition, however, the
duplex state is cryptographically dependent on values `A` does not know, specifically the ECDH
shared secret `[d_E]Q_S` (via the `Absorb` operation) and the plaintext `P` (via the `Encrypt`
operation).

Therefore, `veil.sres` provides confidentiality in the multi-user insider setting.

### Multi-User Authenticity Of Headers

The second of the two main goals of the `veil.sres` is authenticity in the multi-user setting (see
[Multi-User Authenticity](#multi-user-authenticity)), or the inability of an adversary `A` to forge
valid ciphertexts.

#### Outsider Authenticity Of Headers

First, we evaluate the authenticity of `veil.sres` in the multi-user outsider setting (see [Outsider
Authenticity](#outsider-authenticity)), in which the adversary `A` knows the public keys of all
users but none of their private keys ([[BS10]](#bs10), p. 47).

Because the Schnorr signature scheme is sUF-CMA secure, it is infeasible for `A` to forge a
signature for a new message or modify an existing signature for an existing message. Therefore,
`veil.sres` provides authenticity in the multi-user outsider setting.

#### Insider Authenticity Of Headers

Next, we evaluate the authenticity of `veil.sres` in the multi-user insider setting (see
[Insider Authenticity](#insider-authenticity)), in which the adversary `A` knows the receiver's
private key in addition to the public keys of both users  ([[BS10]](#bs10), p. 48).

Again, the Schnorr signature scheme is sUF-CMA secure and the signature is created using the
signer's private key. The receiver (or `A` in possession of the receiver's private key) cannot forge
signatures for new messages. Therefore, `veil.sres` provides authenticity in the multi-user insider
setting.

### Limited Deniability Of Headers

`veil.sres`'s use of a designated-verifier Schnorr scheme provides limited deniability for senders
(see [Limited Deniability](#limited-deniability)). Without revealing `d_R`, the receiver cannot
prove the authenticity of a message (including the identity of its sender) to a third party.

### Indistinguishability Of Headers From Random Noise Of Encrypted Headers

All of the components of a `veil.sres` ciphertext--`C`, `S_0`, and `S_1`--are Cyclist ciphertexts.
An adversary in the outsider setting (i.e. knowing only public keys) is unable to calculate any of
the key material used to produce the ciphertexts; a distinguishing attack is infeasible if Cyclist
is IND-CPA secure.

### Re-use Of Ephemeral Keys

The re-use of an ephemeral key pair `(d_E, Q_E)` across multiple ciphertexts does not impair the
confidentiality of the scheme provided `(N, Q_R)` pairs are not re-used [[BBS03]](#bbs03). An
adversary who compromises a retained ephemeral private key would be able to decrypt all messages the
sender encrypted using that ephemeral key, thus the forward sender security is bounded by the
sender's retention of the ephemeral private key.

## Encrypted Messages

`veil.mres` implements a multi-receiver signcryption scheme.

### Encrypting A Message

Encrypting a message requires a sender's private key `d_S`, receiver public keys `Q_R0…Q_Rn`,
padding length `N_P`, and plaintext `P`.

```text
function EncryptMessage(d_S, Q_R0…Q_Rn, N_P, P):
  Absorb("veil.mres")                 // Initialize an unkeyed duplex.
  Absorb([d_S]G)                      // Absorb the sender's public key.
  k ← Hedge(d_S, Squeeze(32) mod q)   // Hedge a commitment scalar.
  d_E ← Hedge(d_S, Squeeze(32) mod q) // Hedge an ephemeral private key.
  K ← Hedge(d_S, Squeeze(32))         // Hedge a data encryption key.
  N ← Hedge(d_S, Squeeze(16))         // Hedge a nonce.
  C ← N                               // Write the nonce.
  Absorb(N)                           // Absorb the nonce.
  H ← K || N_Q || N_P                 // Encode the DEK and params in a header.

  for Q_R_i in Q_R_0…Q_R_n:           // Encrypt the header for each receiver.
    N_i ← Squeeze(16)
    E_i ← EncryptHeader(d_S, d_E, Q_R_i, H, N_i)
    Absorb(E_i)
    C ← C || E_i

  y ← Rand(N_P)                       // Generate random padding.
  Absorb(y)                           // Absorb padding.
  C ← C || y                          // Append padding to ciphertext.

  Absorb(K)                           // Absorb the DEK.
  Cyclist(SqueezeKey(64), ϵ, ϵ)       // Convert to a keyed duplex.

  for 32KiB blocks p in P:            // Encrypt and tag each block.
    C ← C || Encrypt(p)
    C ← C || Squeeze(16)

  I ← [k]G                            // Calculate the commitment point.
  C ← C || Encrypt(I)                 // Encrypt the commitment point.
  r ← Squeeze(16)                     // Squeeze a short challenge scalar.
  s ← d_E×️r + k                       // Calculate the proof scalar.
  C ← C || Encrypt(s)                 // Encrypt the proof scalar.

  return C
```

### Decrypting A Message

Decrypting a message requires a receiver's private key `d_R`, sender's public key `Q_S`, and
ciphertext `C`.

```text
function DecryptMessage(d_R, Q_S, C):
  Absorb("veil.mres")               // Initialize an unkeyed duplex.
  Absorb(Q_S)                       // Absorb the sender's public key.
  Absorb(C[0..16])                  // Absorb the nonce.
  C ← C[16..]

  (i, N_Q) ← (0, ∞)                 // Go through ciphertext looking for a decryptable header.
  while i < N_Q:
  for each possible encrypted header E_i in C[16..]:
    N_i ← Squeeze(16)
    (E_i, C) ← C[..HEADER_LEN] || C[HEADER_LEN..]
    Absorb(E_i)
    x ← DecryptHeader(d_R, Q_S, N_i, E_i)
    if x ≠ ⊥:
      (Q_E, K || N_Q || N_P) ← x    // Once we decrypt a header, process the remaining headers.

  Absorb(C[..N_P])                  // Absorb the padding.
  C ← C[N_P..]                      // Skip to the message beginning.

  Absorb(K)                         // Absorb the DEK.
  Cyclist(SqueezeKey(64), ϵ, ϵ)     // Convert to a keyed duplex.

  P ← ϵ
  for 32KiB blocks c_i || t_i in C: // Decrypt each block, checking tags.
    p_i ← Decrypt(c_i)
    t_i' ← Squeeze(16)
    if t_i ≠ t_i':
      return ⊥
    P ← P || p_i

  S_0 || S_1 ← C                    // Split the last 64 bytes of the message.
  I ← Decrypt(S_0)                  // Decrypt the commitment point.
  r' ← Squeeze(16)                  // Squeeze a counterfactual challenge scalar.
  s ← Decrypt(S_1)                  // Decrypt the proof scalar.
  I' ← [s]G - [r']Q                 // Calculate the counterfactual commitment scalar.
  if I ≠ I':                        // Verify the signature.
    return ⊥
  return P
```

### Constructive Analysis Of `veil.mres`

`veil.mres` is an integration of two well-known constructions: a multi-receiver hybrid encryption
scheme and an EdDSA-style Schnorr signature scheme.

The initial portion of `veil.mres` is a multi-receiver hybrid encryption scheme, with per-receiver
copies of a symmetric data encryption key (DEK) encrypted in headers with the receivers' public keys
[[Kur02]](#kur02) [[BBS03]](#bbs03) [[BBKS07]](#bbks07) [[RFC4880]](#rfc4880). The headers are
encrypted with the `veil.sres` construction (see [`veil.sres`](#encrypted-headers)), which provides
full insider security (i.e. IND-CCA2 and sUF-CMA in the multi-user insider setting), using a
per-header `Squeeze` value as a nonce. The message itself is divided into a sequence of 32KiB
blocks, each encrypted with a sequence of Cyclist `Encrypt`/`Squeeze` operations, which is IND-CCA2
secure.

The latter portion of `veil.mres` is an EdDSA-style Schnorr signature scheme. The EdDSA-style
Schnorr signature is sUF-CMA secure when implemented in a prime order group and a cryptographic hash
function [[BCJZ21]](#bcjz21) [[CGN20]](#cgn20) [[PS00]](#ps00) [[NSW09]](#nsw09) (see also
[`veil.schnor`](#digital-signatures)).  Short challenge scalars are used which allow for faster
verification with no loss in security [[Por22]](#por22). In addition, this construction allows for
the use of variable-time optimizations during signature verification [[Por20+1]](#por201).

### Multi-User Confidentiality Of Messages

One of the two main goals of the `veil.mres` is confidentiality in the multi-user setting (see
[Multi-User Confidentiality](#multi-user-confidentiality)), or the inability of an adversary `A` to
learn information about plaintexts. As `veil.mres` is a multi-receiver scheme, we adopt Bellare et
al.'s adaptation of the multi-user setting, in which `A` may compromise a subset of receivers
[[BBKS]](#bbks07).

#### Outsider Confidentiality Of Messages

First, we evaluate the confidentiality of `veil.mres` in the multi-user outsider setting (see
[Outsider Confidentiality](#outsider-confidentiality)), in which the adversary `A` knows the public
keys of all users but none of their private keys ([[BS10]](#bs10), p. 44).

As with [`veil.sres`](#encrypted-headers), `veil.mres` superficially resembles an Encrypt-Then-Sign
(EtS) scheme, which are vulnerable to an attack where by `A` strips the signature from the challenge
ciphertext and either signs it themselves or tricks the sender into signing it, thereby creating a
new ciphertext they can then trick the receiver into decrypting for them. Again, as with
`veil.sres`, the identity of the sender is strongly bound during encryption encryption and
decryption making this infeasible.

`A` is unable to forge valid signatures for existing ciphertexts, limiting them to passive attacks.
`veil.mres` ciphertexts consist of ephemeral keys, encrypted headers, random padding, encrypted
message blocks, and encrypted signature points. Each component of the ciphertext is dependent on the
previous inputs (including the headers, which use `Squeeze`-derived nonce to link the `veil.sres`
ciphertexts to the `veil.mres` state). A passive attack on any of those would only be possible if
Cyclist is not IND-CPA secure.

#### Insider Confidentiality Of Messages

Next, we evaluate the confidentiality of `veil.mres` in the multi-user insider setting (see [Insider
Confidentiality](#insider-confidentiality)), in which the adversary `A` knows the sender's private
key in addition to the public keys of all users ([[BS10]](#bs10), p. 45-46). `A` cannot decrypt the
message by themselves, as they do not know either `d_E` or any `d_R` and cannot decrypt any of the
`veil.sres`-encrypted headers. As with [`veil.sres`](#multi-user-confidentiality-of-headers) `A`
cannot trick the receiver into decrypting an equivalent message by replacing the signature, despite
`A`~'s ability to use `d_S` to create new headers. In order to generate a valid signature on a
ciphertext `c'` (e.g. `c'=c||1`), `A` would have to squeeze a valid challenge scalar `r'` from the
duplex state.  Unlike the signature hash function in the generic EtS composition, however, the
duplex state is cryptographically dependent on a value `A` does not know, specifically the data
encryption key `K` (via the `Absorb` operation) and the plaintext blocks `p_{0..n}` (via the
`Encrypt` operation).

Therefore, `veil.mres` provides confidentiality in the multi-user insider setting.

### Multi-User Authenticity Of Messages

The second of the two main goals of the `veil.mres` is authenticity in the multi-user setting (see
[Multi-User Authenticity](#multi-user-authenticity)), or the inability of an adversary `A` to forge
valid ciphertexts.

#### Outsider Authenticity Of Messages

First, we evaluate the authenticity of `veil.mres` in the multi-user outsider setting (see [Outsider
Authenticity](#outsider-authenticity)), in which the adversary `A` knows the public keys of all
users but none of their private keys ([[BS10]](#bs10), p. 47).

Because the Schnorr signature scheme is sUF-CMA secure, it is infeasible for `A` to forge a
signature for a new message or modify an existing signature for an existing message. Therefore,
`veil.mres` provides authenticity in the multi-user outsider setting.

#### Insider Authenticity Of Messages

Next, we evaluate the authenticity of `veil.mres` in the multi-user insider setting (see [Insider
Authenticity](#insider-authenticity)), in which the adversary `A` knows some receivers' private keys
in addition to the public keys of both users ([[BS10]](#bs10), p. 47).

Again, the Schnorr signature scheme is sUF-CMA secure and the signature is created using the
ephemeral private key, which `A` does not possess. The receiver (or `A` in possession of the
receiver's private key) cannot forge signatures for new messages. Therefore, `veil.mres` provides
authenticity in the multi-user insider setting.

### Limited Deniability Of Messages

The only portion of `veil.mres` ciphertexts which are creating using the sender's private key (and
thus tying a particular message to their identity) are the `veil.sres`-encrypted headers. All other
components are creating using the data encryption key or ephemeral private key, neither of which are
bound to identity. `veil.sres` provides limited deniability (see [Limited
Deniability](#limited-deniability)), therefore `veil.mres` does as well.

### Indistinguishability Of Messages From Random Noise

`veil.mres` ciphertexts are indistinguishable from random noise. All components of an
`veil.mres` ciphertext are Cyclist ciphertexts; a successful distinguishing attack on them
would require Cyclist to not be IND-CPA secure.

### Partial Decryption

The division of the plaintext stream into blocks takes its inspiration from the CHAIN construction
[[HRRV]](#hrrv15), but the use of Cyclist allows for a significant reduction in complexity. Instead
of using the nonce and associated data to create a feed-forward ciphertext dependency, the Cyclist
duplex ensures all encryption operations are cryptographically dependent on the ciphertext of all
previous encryption operations. Likewise, because the `veil.mres` ciphertext is terminated with a
Schnorr signature (see [`veil.schnorr`](#digital-signatures)), using a special operation for the
final message block isn't required.

The major limitation of such a system is the possibility of the partial decryption of invalid
ciphertexts. If an attacker flips a bit on the fourth block of a ciphertext, `veil.mres` will
successfully decrypt the first three before returning an error. If the end-user interface displays
that, the attacker may be successful in radically altering the semantics of an encrypted message
without the user's awareness. The first three blocks of a message, for example, could say `PAY
MALLORY $100`, `GIVE HER YOUR CAR`, `DO WHAT SHE SAYS`, while the last block might read `JUST
KIDDING`.

## Passphrase-based Encryption

`veil.pbenc` implements a memory-hard authenticated encryption scheme to encrypt private keys at
rest.

### Initialization

Initializing a keyed duplex requires a passphrase `P`, salt `S`, time parameter `N_T`, space
parameter `N_S`, delta constant `D=3`, and block size constant `N_B=1024`.

```text
function HashBlock(C, [B_0..B_n], N):
  Absorb("veil.pbenc.iter") // Initialize an unkeyed duplex.
  Absorb(C)                 // Absorb the counter.
  C ← C + 1                 // Increment the counter.

  for B_i in [B_0..B_n]:    // Absorb each input block.
    Absorb(B_i)

  return Squeeze(N)         // Squeeze N bytes of output.

procedure InitFromPassphrase(P, S, N_T, N_S):
  C ← 0 // Initialize a counter.
  B ← [[0x00 ✕ N_B] ✕ N_S] // Initialize a buffer.

  B[0] ← HashBlock(C, [P, S], N_B) // Expand input into buffer.
  for m in 1..N_S:
    B[m] ← HashBlock(C, [B[m-1]], N_B) // Fill remainder of buffer with hash chain.

  for t in 0..N_T: // Mix buffer contents.
    for m in 0..N_S:
      m_prev ← (m-1) mod N_S
      B[m] = HashBlock(C, [B[(m-1) mod N_S], B[m]], N_B) // Hash previous and current blocks.

      for i in 0..D:
        r ← HashBlock(C, [S, t, m, i], 8) // Hash salt and loop indexes.
        B[m] ← HashBlock(C, [[B[m], B[r]]], N_B) // Hash pseudo-random and current blocks.

  Absorb("veil.pbenc") // Initialize an unkeyed duplex.
  Absorb(B[N_S-1]) // Extract output from buffer.
  Cyclist(SqueezeKey(64), ε, ε) // Convert to a keyed duplex.
```

### Encrypting A Private Key

Encrypting a private key requires a passphrase `P`, time parameter `N_T`, space parameter `N_S`, and
private key `d`.

```text
function EncryptPrivateKey(P, N_T, N_S, d):
  S ← Rand(16) // Generate a random salt.
  InitFromPassphrase(P, S, N_T, N_S) // Initialize the duplex.
  C ← Encrypt(d) // Encrypt the private key.
  T ← Squeeze(16) // Squeeze an authentication tag.
  return N_T || N_S || S || C || T
```

### Decrypting A Private Key

Decrypting a private key requires a passphrase `P` and ciphertext `C=N_T || N_S || S || C || T`.

```text
function DecryptPrivateKey(P, N_T, N_S, d):
  InitFromPassphrase(P, S, N_T, N_S) // Initialize the duplex.
  d' ← Decrypt(C)                    // Decrypt the ciphertext.
  T' ← Squeeze(16)                   // Squeeze an authentication tag.
  if T ≠ T':                         // Return an error if the tags are not equal.
    return ⊥
  return d'
```

### Constructive Analysis Of `veil.pbenc`

`veil.pbenc` is an integration of a memory-hard key derivation function (adapted for the
cryptographic duplex) and a standard Cyclist authenticated encryption scheme.

The `InitFromPassphrase` procedure of `veil.pbenc` implements balloon hashing, a memory-hard hash
function intended for hashing low-entropy passphrases [[BCGS16]](#bcgs16). Memory-hard functions are
a new and active area of cryptographic research, making the evaluation of schemes difficult. Balloon
hashing was selected for its resilience to timing attacks, its reliance on a single hash primitive,
and its relatively well-developed security proofs. The use of a duplex as a wide block labeling
function is not covered by the security proofs in Appendix B.3 of [[BCGS16]](#bcgs16) but aligns
with the use of BLAKE2b in Argon2 [[RFC9106]](#rfc9106).

The `EncryptPrivateKey` and `DecryptPrivateKey` functions use `InitFromPassphrase` to initialize the
duplex state, after which they implement a standard Cyclist authenticated encryption scheme, which
is IND-CCA2 secure.

## References

### ABHKLR21

Joël Alwen, Bruno Blanchet, Eduard Hauck, Eike Kiltz, Benjamin Lipp, and Doreen Riepel. 2021.
Analysing the HPKE standard.  In Annual International Conference on the Theory and Applications of
Cryptographic Techniques, Springer, 87–116.  <https://eprint.iacr.org/2020/1499.pdf>

### AR10

Jee Hea An and Tal Rabin. 2010. Security for Signcryption: The Two-User Model. In Practical
Signcryption. Springer, 21–42.

### AOTZ20

Diego F Aranha, Claudio Orlandi, Akira Takahashi, and Greg Zaverucha. 2020. Security of hedged
Fiat–Shamir signatures under fault attacks. In Annual International Conference on the Theory and
Applications of Cryptographic Techniques, Springer, 644–674. <https://eprint.iacr.org/2019/956.pdf>

### Lan16

Adam Langley. 2016. Cryptographic Agility. <https://www.imperialviolet.org/2016/05/16/agility.html>

### Ngu04

Phong Q Nguyen. 2004. Can we trust cryptographic software? Cryptographic flaws in GNU Privacy Guard
v1.2.3. In International Conference on the Theory and Applications of Cryptographic Techniques,
Springer, 555–570. <https://link.springer.com/content/pdf/10.1007%252F978-3-540-24676-3_33.pdf>

### BSW21

Jenny Blessing, Michael A. Specter, and Daniel J. Weitzner. 2021. You Really Shouldn’t Roll Your Own
Crypto: An Empirical Study of Vulnerabilities in Cryptographic Libraries. CoRR abs/2107.04940,
(2021). <https://arxiv.org/abs/2107.04940>

### BGB04

Nikita Borisov, Ian Goldberg, and Eric Brewer. 2004. Off-the-record communication, or, why not to
use PGP. In Proceedings of the 2004 ACM workshop on Privacy in the electronic society, 77–84.
<https://otr.cypherpunks.ca/otr-wpes.pdf>

### BHKL13

Daniel J Bernstein, Mike Hamburg, Anna Krasnova, and Tanja Lange. 2013. Elligator: Elliptic-curve
points indistinguishable from uniform random strings. In Proceedings of the 2013 ACM SIGSAC
conference on Computer & communications security, 967–980.
<https://elligator.cr.yp.to/elligator-20130828.pdf>

### BS10

Joonsang Baek and Ron Steinfeld. 2010. Security for signcryption: the multi-user model. In Practical
Signcryption. Springer, 43–53.

### YHR04

Tom Yu, Sam Hartman, and Kenneth Raeburn. 2004. The Perils of Unauthenticated Encryption: Kerberos
Version 4. In NDSS, 4–4. <https://web.mit.edu/tlyu/papers/krb4peril-ndss04.pdf>

### RD10

Juliano Rizzo and Thai Duong. 2010. Practical padding oracle attacks. In 4th USENIX Workshop on
Offensive Technologies (WOOT 10).
<https://www.usenix.org/legacy/event/woot10/tech/full_papers/Rizzo.pdf>

### CHK03

Ran Canetti, Shai Halevi, and Jonathan Katz. 2003. A forward-secure public-key encryption scheme. In
International Conference on the Theory and Applications of Cryptographic Techniques, Springer,
255–271. <https://eprint.iacr.org/2003/083.pdf>

### Str06

Maurizio Adriano Strangio. 2006. On the resilience of key agreement protocols to key compromise
impersonation. In European Public Key Infrastructure Workshop, Springer, 233–247.
<https://eprint.iacr.org/2006/252.pdf>

### RFC9180

R. Barnes, K. Bhargavan, B. Lipp, and C. Wood. 2022. Hybrid Public Key Encryption.
<http://www.rfc-editor.org/rfc/rfc9180.html>

### Por20

Thomas Pornin. 2020. Double-Odd Elliptic Curves. <https://eprint.iacr.org/2020/1558>

### Por20+1

Thomas Pornin. 2020. Optimized Lattice Basis Reduction In Dimension 2, and Fast Schnorr and EdDSA
Signature Verification. <https://eprint.iacr.org/2020/454>

### Por22

Thomas Pornin. 2022. Double-Odd Jacobi Quartic. <https://eprint.iacr.org/2022/1052>

### DHPVV20

Joan Daemen, Seth Hoffert, Michaël Peeters, Gilles Van Assche, and Ronny Van Keer. 2020. Xoodyak, a
lightweight cryptographic scheme. IACR Transactions on Symmetric Cryptology 2020, S1 (June 2020),
60–87.
<https://csrc.nist.gov/CSRC/media/Projects/lightweight-cryptography/documents/round-2/spec-doc-rnd2/Xoodyak-spec-round2.pdf>

### Aum19

Jean-Philippe Aumasson. 2019. Too Much Crypto. (2019). <https://eprint.iacr.org/2019/1492>

### BBM18

Christian Badertscher, Fabio Banfi, and Ueli Maurer. 2018. A Constructive Perspective on
Signcryption Security. In IACR Cryptol. ePrint Arch. <https://ia.cr/2018/050>

### BBKS07

Mihir Bellare, Alexandra Boldyreva, Kaoru Kurosawa, and Jessica Staddon. 2007. Multi-recipient
encryption schemes: Efficient constructions and their security. IEEE Transactions on Information
Theory 53, 11 (2007), 3927–3943. <https://faculty.cc.gatech.edu/~aboldyre/papers/bbks.pdf>

### BBS03

Mihir Bellare, Alexandra Boldyreva, and Jessica Staddon. 2003. Randomness re-use in multi-recipient
encryption schemes. In International Workshop on Public Key Cryptography, Springer, 85–99.
<https://www.iacr.org/archive/pkc2003/25670085/25670085.pdf>

### BDPV11

Guido Bertoni, Joan Daemen, Michaël Peeters, and Gilles Van Assche. 2011. Duplexing the sponge:
single-pass authenticated encryption and other applications. In International Workshop on Selected
Areas in Cryptography, Springer, 320–337. <https://keccak.team/files/SpongeDuplex.pdf>

### BDPVVV18

Guido Bertoni, Joan Daemen, Michaël Peeters, Gilles Van Assche, Ronny Van Keer, and Benoı̂t Viguier.
2018. KangarooTwelve: Fast Hashing Based on Keccak-p. In International Conference on Applied
Cryptography and Network Security, Springer, 400–418. <https://eprint.iacr.org/2016/770.pdf>

### BDPV08

Guido Bertoni, Joan Daemen, Michaël Peeters, and Gilles Van Assche. 2008. On the Indifferentiability
of the Sponge Construction. In Advances in Cryptology – EUROCRYPT 2008, Springer Berlin Heidelberg,
Berlin, Heidelberg, 181–197. <https://keccak.team/files/SpongeIndifferentiability.pdf>

### BDPV11+1

Guido Bertoni, Joan Daemen, Michaël Peeters, and Gilles Van Assche. 2011. Cryptographic sponge
functions. In SHA-3 competition (round 3). <https://keccak.team/files/CSF-0.1.pdf>

### RFC9106

A. Biryukov, D. Dinu, D. Khovratovich, and S. Josefsson. 2021. Argon2 Memory-Hard Function for
Password Hashing and Proof-of-Work Applications. <http://www.rfc-editor.org/rfc/rfc9106.html>

### BCGS16

Dan Boneh, Henry Corrigan-Gibbs, and Stuart Schechter. 2016. Balloon hashing: A memory-hard function
providing provable protection against sequential attacks. In International Conference on the Theory
and Application of Cryptology and Information Security, Springer, 220–248.
<https://eprint.iacr.org/2016/027.pdf>

### BCJZ21

Jacqueline Brendel, Cas Cremers, Dennis Jackson, and Mang Zhao. 2021. The Provable Security of
Ed25519: Theory and Practice. In 2021 IEEE Symposium on Security and Privacy (SP), 1659–1676.
<https://eprint.iacr.org/2020/823.pdf>

### RFC4880

J. Callas, L. Donnerhacke, H. Finney, D. Shaw, and R. Thayer. 2007. OpenPGP Message Format.
<http://www.rfc-editor.org/rfc/rfc4880.html>

### CGN20

Konstantinos Chalkias, François Garillot, and Valeria Nikolaenko. 2020. Taming the many EdDSAs. In
International Conference on Research in Security Standardisation, Springer, 67–90.
<https://eprint.iacr.org/2020/1244.pdf>

### Ham17

Mike Hamburg. 2017. The STROBE protocol framework. <https://eprint.iacr.org/2017/003.pdf>

### HRRV15

Viet Tung Hoang, Reza Reyhanitabar, Phillip Rogaway, and Damian Vizár. 2015. Online
authenticated-encryption and its nonce-reuse misuse-resistance. In Annual Cryptology Conference,
Springer, 493–517. <https://eprint.iacr.org/2015/189.pdf>

### KL20

Jonathan Katz and Yehuda Lindell. 2020. Introduction to Modern Cryptography. Chapman.
DOI:<https://doi.org/10.1201/9781351133036>

### Kur02

Kaoru Kurosawa. 2002. Multi-recipient public-key encryption with shortened ciphertext. In
International Workshop on Public Key Cryptography, Springer, 48–63.
<https://eprint.iacr.org/2001/071>

### NSW09

Gregory Neven, Nigel P Smart, and Bogdan Warinschi. 2009. Hash function requirements for Schnorr
signatures. Journal of Mathematical Cryptology 3, 1 (2009), 69–87.
<http://www.neven.org/papers/schnorr.pdf>

### PS00

David Pointcheval and Jacques Stern. 2000. Security arguments for digital signatures and blind
signatures. Journal of Cryptology 13, 3 (2000), 361–396.
<https://www.di.ens.fr/david.pointcheval/Documents/Papers/2000_joc.pdf>

### SWP04

Ron Steinfeld, Huaxiong Wang, and Josef Pieprzyk. 2004. Efficient extension of standard Schnorr/RSA
signatures into universal designated-verifier signatures. In International Workshop on Public Key
Cryptography, Springer, 86–100. <https://www.iacr.org/archive/pkc2004/29470087/29470087.pdf>