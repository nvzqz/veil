# Command Line Tool

The Veil cryptosystem is implemented as a command line tool `veil-cli`.

## Installation

To install it, check out this repository and build it yourself:

```shell
git clone https://github.com/codahale/veil-rs
cargo install
```

Because this is a cryptosystem designed by one person with no formal training and has not been audited, it will never be
packaged conveniently. Cryptographic software is primarily used in high-risk environments where strong assurances of
correctness, confidentiality, integrity, etc. are required, and `veil-cli` does not provide those assurances. It's more
of an art installation than a practical tool.

## Shell Completion

`veil-cli` comes with shell completion scripts for Bash, Zsh, Fish, and Powershell. Find them in `veil-cli/share`.