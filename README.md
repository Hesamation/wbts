# Who Broke the Swarm

Staked attribution on tamper-evident run logs, using the optimistic challenge model
with deterministic adjudication for direct T2 F3/F1 claims.

**Killer beat:** coordinator falsely blames a passing worker; a watcher challenges;
the claim is overturned — the reporter loses 0.25 NEAR, the challenger earns 0.20 NEAR,
and no innocent agent is blamed: the corrected attribution is `none`.

The 3-agent pipeline is the worked example. The current adjudicator supports direct
T2 F3/F1 claims; nested traversal and post-hoc reputation updates outside the dispute
window are not implemented.

## Status
Contract Phase 2.18 adds claimant-burden expiry for unresolved challenges.
Fresh deployment and CLI S2 regression passed with exact money and
baseline-relative reputation checks. See the [live evidence](codex-log/2.18-live.md),
[CLI instructions](cli/README.md) and [local status](STATUS.md).

## Tracks
swarms (primary) + business + security.

## Local build and simulated S2

Run all commands from the repository root. The tested Linux toolchain is
**Rust/Cargo 1.99.1, cargo-near 0.23.0**, with the
`wasm32-unknown-unknown` rustup target installed. The historical Rust 1.97.1 /
cargo-near 0.21.1 combination is not the B1-tested toolchain. Keep the committed
dependency lockfiles (including near-sdk 5.29.1).

Prerequisites: Git, rustup, the toolchain above, a C/C++ build toolchain, Clang/libclang, CMake,
pkg-config, OpenSSL development files and D-Bus development files (`dbus-1.pc`).
On the tested Arch Linux host these system files are supplied by base-devel,
clang, cmake, pkgconf, openssl and dbus. Debian/Ubuntu package names include build-essential,
clang, libclang-dev, cmake, pkg-config, libssl-dev and libdbus-1-dev. Dependency downloads require
network access. No NEAR keys, account funding or testnet RPC are needed.

For Ubuntu (including the CI runner), install the native prerequisites and the
B1-tested tools with these commands. Rustup must already be installed. The
`rustup default` command selects this version for subsequent Cargo commands,
including nested simulator builds. Cargo-near installation keeps its published
lockfile; repository builds keep the committed lockfiles.

```sh
sudo apt-get update
sudo apt-get install -y build-essential clang libclang-dev cmake pkg-config libssl-dev libdbus-1-dev
rustup toolchain install 1.99.1 --profile minimal --component clippy --target wasm32-unknown-unknown
rustup default 1.99.1
cargo install cargo-near --version 0.23.0 --locked
```

The Debian/Ubuntu package mapping is pending confirmation on GitHub Actions;
B1 validated the equivalent prerequisites on Arch Linux.

Check the installed tools, then follow [contract build and simulator tests](contract/README.md#local-build-and-tests)
and [CLI build, tests and simulated S2](cli/README.md#local-build-and-simulated-s2),
in that order. The two jobs in [CI](.github/workflows/ci.yml) mirror these
README commands, including the Clippy commands below. The live testnet and individual-command examples are
separate, credentialed operations and are not part of this path.

```sh
rustc --version
cargo --version
cargo near --version
rustup target list --installed
pkg-config --modversion dbus-1
export CARGO_BUILD_JOBS=2
```

The job limit also applies to the contract builds launched by simulator tests.
CI sets `CARGO_BUILD_JOBS=2` at workflow scope, equivalent to the export above.
CI runs on pushes and pull requests targeting `codex/align-agents-20261108`;
it uses no NEAR credentials or testnet calls.

## License
TBD (any license allowed by hackathon rules).

---

*Further submission documentation remains Phase 9 work. Every external claim needs
supporting evidence.*
