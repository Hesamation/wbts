# FactSettle

**Proof-backed payments for agent data jobs on NEAR.** A specialist should not have
to reveal a useful answer and then hope the buyer sends payment. A buyer should not
pay for an invented or missing answer.

FactSettle escrows a job price against a seller-published dataset root and explicit fact
keys. The specialist accepts the terms. Each valid Merkle opening earns its agreed
share directly; an authenticated empty slot or expired undelivered work refunds
that share. Buyer approval is not required after acceptance. Every allocation is
withdrawable credit, with a checked transfer callback.

## Judges: start here

**[Try the hosted demo](https://hesamation.github.io/wbts/)** ·
[Two-minute judge guide](https://hesamation.github.io/wbts/guide.html) ·
[Execution manifest](demo/evidence/manifest.json)

Start with **Buyer won't approve**: the seller receives the agreed payment without
another approval call. Use **Fund → Accept → Deliver → Withdraw** to jump through
the recorded deal. Then try the altered-answer and partial-delivery cases.

No wallet, API key, funding or npm install is needed to inspect the included runs.
From the repository root, with Python 3:

```sh
python3 -m http.server 8765 --bind 127.0.0.1 --directory demo
```

Open **http://127.0.0.1:8765/**.

1. **Buyer won't approve:** two valid fact openings earn 0.02 virtual NEAR with no
   buyer approval call. Inspect both confirmed withdrawals.
2. **Seller invents a value:** the altered opening is rejected and the price is
   refunded on expiry.
3. **One fact never arrives:** only the delivered fact is paid; the remainder is
   refunded.
4. Change a fact value and press **Check proof**. Your browser independently
   recomputes the Merkle path and shows the mismatch.
5. Inspect the final wallet table and download an execution record. It separates
   job payment, gas, retained storage, wallet changes and withdrawal results.

The other cases show authenticated absence, an unaccepted offer and cancellation.
[Viewer instructions](demo/README.md) explain replay versus re-execution.

The new proof-backed contract's [release CI](https://github.com/Hesamation/wbts/actions/runs/43631842501)
passed 42 simulator tests and seven CLI tests, builds/lint, and the six-case
export check at `3c19289`. The viewer checks file hashes against the manifest;
its editable proof check independently recomputes the Merkle root.

**Evidence boundary:** these are recorded executions of the compiled contract in
NEAR's pinned local simulator, with distinct signing accounts and constructed
public datasets. They are not testnet receipts, live customers or a live agent
market. The browser reads execution files; it does not sign or execute jobs.

## What the proof means

The implemented predicate is narrow: a requested string-valued fact (or an empty
slot) must open to the immutable dataset root pinned when the job opened. The
contract checks the requested key, exact committed entry and 32-level path.

A proof establishes dataset consistency. It does **not** establish the dataset's
external truth, source authenticity, answer quality, general agent reasoning or a
fresh model execution. The buyer chooses the seller and dataset it trusts.
Delivered entries, salts and paths are public. The unopened dataset is not
published by this contract; the bundled sample dataset is public.

Only the named seller can accept or deliver. Funds have one destination per key:
a valid present entry credits the seller; a valid empty slot credits the buyer.
Rejected calls roll back. After the applicable deadline, anyone may close the
remaining work and credit the buyer. A buyer may cancel before acceptance; a
seller may close early and refund unfinished work. Payments use integer yoctoNEAR,
including an exact remainder allocation.

## Why build this

[InferPay's documented seller-first workflow](https://github.com/taejeekim/InferPay/tree/c7f773f144ff01092348185ed657dfe729fcec44#limits)
verifies data proofs before a buyer-signed transfer and explicitly identifies
proof-verifying escrow as unfinished. We inspected its payout and payment code.
FactSettle addresses that concrete integration gap with enforceable settlement.

A capable escrow can implement these rules; this is such an implementation. The
contribution is a bounded, reproducible fact-proof settlement module, participant
client, recovery behavior and inspectable evidence. There is no novelty claim,
upstream partnership or validated customer demand. Compatibility covers the
`fact`/vacancy subset of InferPay's proof scheme, not its full application.

NEAR supplies authenticated callers, shared escrow state, atomic proof checking
and credit allocation, block deadlines, and asynchronous transfer callbacks.
Scripted agents exercise buyer, specialist and permissionless observer roles.
Suggested track fit: **Agent Economy** and **AI Security & Defense**.

## Execute current source

Tested tools: Rust/Cargo **1.99.1**, cargo-near **0.23.0**, Node **22.23.0**, Python 3,
and the `wasm32-unknown-unknown` target. Keep all committed dependency locks,
including near-sdk **5.29.1**. There is no root Cargo workspace.

Native prerequisites: C/C++ build tools, Clang/libclang, CMake, pkg-config, OpenSSL,
D-Bus and libudev development files. On Ubuntu these are `build-essential clang
libclang-dev cmake pkg-config libssl-dev libdbus-1-dev libudev-dev`.

```sh
rustc --version
cargo near --version
node --version
export CARGO_BUILD_JOBS=2
node data-escrow/compat/generate.mjs
cargo near build non-reproducible-wasm --manifest-path data-escrow/Cargo.toml --locked
cargo test --manifest-path swarm-tests/Cargo.toml --locked --tests -- --test-threads=2
cargo test --manifest-path cli/Cargo.toml --locked
cargo run --manifest-path swarm-tests/Cargo.toml --locked --example data_demo -- demo/evidence
python3 scripts/check-data-demo.py
```

The exporter runs current contract WASM and asserts job settlement, participant
wallet reconciliation and completed withdrawals before writing the six cases.
The independent Python check rechecks file hashes, proof roots and exact monetary
arithmetic. Dependency downloads need network; simulator tests need no NEAR keys.
Host builds are not a Docker or cross-toolchain reproducibility claim.

Strict checks for the new contract/client/harness:

```sh
cargo clippy --manifest-path data-escrow/Cargo.toml --locked --target wasm32-unknown-unknown -- -D warnings
cargo clippy --manifest-path cli/Cargo.toml --locked --all-targets -- -D warnings
cargo clippy --manifest-path swarm-tests/Cargo.toml --locked --all-targets -- -D warnings
```

[CI](.github/workflows/ci.yml) exercises this local path. Historical test results
and live v1 journals are not new-contract evidence.

## Use your own data and separate participants

The local dataset tool accepts arrays of `{topic, value, source}` string fields.
It creates fresh salts, a catalog and a private sealed dataset, then produces only
the requested opening. Existing commitments are protected from overwriting.

```sh
node scripts/data-facts.mjs seal data-escrow/compat/coverage.json .wbts-data/seller
node scripts/data-facts.mjs prove .wbts-data/seller/sealed.json mri-copay .wbts-data/opening.json
cargo run --manifest-path cli/Cargo.toml --locked --bin data-cli -- --help
```

Keep `sealed.json` private. Publishing an opening reveals that entry and its proof.
The client supports publish/open/accept/deliver/expire/cancel/close/withdraw and
read-only views. Each participant can run it independently with its own existing
named testnet key. Before signing, it checks testnet identity and requires deployed
code to match the supplied local WASM. It saves public submission intent and never
automatically resends an uncertain submission. It creates no accounts and deploys
nothing. [Contract and client guide](data-escrow/README.md).

## Costs, recovery and limits

- Price is configurable; the example 0.02 NEAR is an illustrative parameter.
  Retained storage is substantial for tiny purchases. The viewer includes it,
  alongside gas and actual simulator wallet changes. It is not a micropayment
  cost or profitability claim. Deployment/base storage and obtaining the external
  data are outside the displayed per-run totals.
- Unused storage deposits and refunds become credits. Paid records are retained;
  their storage charge is excluded from recovery. All eight requested-key slots
  have exact integer allocation, even for prices smaller than the key count.
- A failed transfer restores credit through its private nonce/amount-checked
  callback. A failed callback itself can leave an ambiguous in-flight record;
  blind retries and an administrative rescue path are absent.
- Permissionless expiry still requires a caller. There is no background keeper.
  Block deadlines are not guaranteed wall-clock durations.
- This is a prototype, not an audit or production-safety claim. The ledger invariant
  checks allocations; it is not by itself a wallet solvency proof. A deployment's
  full-access key retains upgrade powers unless its owner removes them.
- No general marketplace, nested liability, subjective quality arbitration, TEE,
  Intents integration, reputation economy or mainnet client is implemented.

## Project history and independence

**FactSettle** is the current product name. The repository URL and internal WBTS
identifiers are retained for continuity; they do not imply that the old swarm
attribution mechanism is the current demo.

The original optimistic T2 attribution contract and CLI remain under
[`contract/`](contract/README.md) and [`cli/`](cli/README.md). They demonstrate
bonded false-accusation defense, with an explicitly unsafe missed-defense outcome:
false unchallenged claims can become final. The new `data-escrow/` contract is a
fresh-state successor with eager proof checks and job-price settlement; it does
not migrate prior stakes, state or deployments. [Historical handoff](WBTS-HISTORY.md).

Another project by the same solo developer is **[Where's the Rest?](https://github.com/Hesamation/wheres-the-rest)**,
a NEAR-to-Base payout investigation tool. It is a separate codebase and product; no
payout-reader implementation is included here. This disclosure supports the event's
independent multiple-project rule and does not assert submission of either project.

Built during NEAR HACKATHON with Codex assistance. The Rust contract uses near-sdk;
tests use pinned near-contract-sim; the client uses the libraries listed in
`cli/Cargo.toml`. The sparse Merkle scheme, compatibility JS and public sample facts
are adapted from Taejee Kim's MIT-licensed InferPay. See
[third-party notices](THIRD_PARTY_NOTICES.md). The new contract, accounting,
participant client, execution exporter and viewer are project implementation work.

## License

[MIT](LICENSE), with third-party notices retained. Repository publication and live
deployment are separate from the local evidence supplied here.
