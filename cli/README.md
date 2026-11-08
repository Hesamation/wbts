# Testnet CLI core

Build with `cargo build --manifest-path cli/Cargo.toml --locked`. Run
`cli/target/debug/cli --help` for `register`, `anchor`, `file-claim`, `challenge`
and `run-demo`. Linux builds require the D-Bus development library (`dbus-1`
via pkg-config) for the existing secure keystore. The lockfile pins CLI dependencies;
contract dependencies are unchanged.

## One-command S2

```sh
cli/target/debug/cli --contract s2.coord-wbts.testnet run-demo s2
```

`--contract` (or `WBTS_CONTRACT`) is overridable. `--rpc-url` (or `WBTS_RPC_URL`)
selects an RPC; the CLI checks `chain_id=testnet` at connection and before every
signature. Only named `.testnet` accounts are accepted. There is no deployment path.

The recorded live attempt used `--registrar watcher-wbts.testnet` because the
coordinator lacked registration funding. It stopped on RPC HTTP 429 during
adjudication; the live end-to-end gate remains incomplete. Read
[the live log](../codex-log/cli-s2-live.md) before spending more funds or recovering it.

Roles default to coordinator/reporter `coord-wbts.testnet`, worker
`worker-wbts.testnet`, challenger `watcher-wbts.testnet`; override with `--reporter`,
`--worker`, `--challenger`. They must be distinct. `--registrar ACCOUNT` selects
the registration stake sponsor, defaulting to reporter, without changing settlement
roles. Registration locks **3 NEAR** supplied by that signer. Stake release is not
implemented in the contract; repeated runs accumulate locked principal and storage.
Each signer must have sufficient available balance, including storage and gas.

A run ID is generated from Unix nanoseconds, or supplied with `--run-id`.
Never reuse a run ID. S2 checks 1 NEAR participation stakes, 0.25 NEAR bonds,
the pinned T2 implementation, and a 600-block challenge window. It submits seven
transactions, sequentially: register, two anchors, false F1 claim, challenge,
adjudicate, finalize. The view-only preview must return Overturned with null
finding before adjudication. Finalize waits until a finalized block is strictly
greater than the claim deadline; elapsed time varies with chain progress.

The worker output is correct: entries 10000/25000/15000 total 50000; fee
floor(50000 × 250 / 10000) = 1250, net 48750. There is no injected failure.

## Individual commands

```sh
cli/target/debug/cli register --run-id example-unique --signer coord-wbts.testnet \
  --agents coord-wbts.testnet worker-wbts.testnet watcher-wbts.testnet
cli/target/debug/cli anchor --run-id example-unique --signer coord-wbts.testnet \
  --kind task_assigned --payload /tmp/assignment.json \
  --worker worker-wbts.testnet --assignment-seq 0
cli/target/debug/cli anchor --run-id example-unique --signer worker-wbts.testnet \
  --kind settlement --payload /tmp/completion.json \
  --worker worker-wbts.testnet --assignment-seq 0
cli/target/debug/cli file-claim --run-id example-unique --signer coord-wbts.testnet \
  --blamed worker-wbts.testnet --failure-class F1 --evidence-seqs 0,1
cli/target/debug/cli challenge --run-id example-unique --signer watcher-wbts.testnet \
  --claim-id CLAIM_ID
```

Assignment JSON: `{"entries":["10000","25000","15000"],"declared_total":"50000"}`.
Completion JSON: `{"fee":"1250","net":"48750"}`. Payload canonicalization is
compact UTF-8 JSON with recursively sorted object keys using serde_json's number
serialization, not a claim of RFC 8785 conformance. The canonical bytes are saved
locally, hashed with SHA-256 and used unchanged in adjudication. The event head
is the contract's returned Borsh event hash, including execution block height;
it is distinct from the payload hash. Every append checks the saved head against
the chain. `file-claim` cites saved event sequences, payload hashes and actual
anchor transaction hashes. The contract ABI has no verifier-output-hash field;
S2 saves the full preview locally instead.

## Keys, deposits and evidence

The CLI **only reads** existing full-access keys, first from the near-cli secure
keychain (`near-testnet-ACCOUNT`, username `ACCOUNT:PUBLIC_KEY`), then from
`~/.near-credentials/testnet/ACCOUNT.json` or `ACCOUNT/PUBLIC_KEY.json`. It matches
the private key's public key to an on-chain full-access key. It never generates,
writes, accepts as an argument, or prints private keys. Credential parse errors
are redacted. Keep credentials exclusively in the local keystore.

Parameters come from `get_parameters`; the source/dependency-manifest verifier
hash is pinned to `08fc983ac89ff77b22129158b22eb7bd461eb04215955b51d433331385c2bf58`.
This is an identity pin, not evidence that an arbitrary deployment ran that source.
The contract and its historical deployed-WASM evidence remain separate artifacts.

Each storage-growing call attaches its principal plus a storage allowance,
default **0.05 NEAR**, overridable by `--storage-allowance` in yoctoNEAR with a
hard upper bound of 0.1 NEAR. The contract charges measured growth and refunds
excess. The CLI reads protocol storage price, measures byte growth, sums gas/token
costs from transaction and receipt outcomes, and reconciles each payable call's
signer balance to principal + storage + gas before reporting the excess refund.
The allowance bounds the funding attempt; a too-small allowance stops the command.

Every transaction prints its hash, contract execution block height, Unix start
time, elapsed milliseconds, gas, storage and refund amounts. S2 saves a fixed-block
account/reputation/treasury baseline **after registration and anchors**, then
another after settlement. In exact integer arithmetic it checks economic deltas
reporter −0.25, worker 0, challenger +0.20, treasury +0.05 NEAR, excluding gas and
storage; payout is 0.45 NEAR including returned challenge principal. All three
stakes must remain 1 NEAR, bonds clear, and corrected attribution stays null.
Reputation uses starting scores: reporter max(0, score−15), worker min(120, score+2),
challenger min(120, score+5). Fresh simulator state asserts 85/102/105; live repeats
preserve previous scores. Use otherwise idle accounts and an idle contract for
inspectable accounting: concurrent activity invalidates these balance measurements.

Public journals default to ignored `.wbts/`; override with `--journal-dir`.
The per-run filename hashes contract account + run ID. Journals retain canonical
payloads, confirmed heads, transactions, preview, snapshots and verification.
`transactions/HASH.json` saves a public submission intent **before broadcast**,
then the finalized outcome, including refund receipts, when available.
No signed transaction bytes or credential material are written.

Any error stops the scenario. Submission errors preserve the hash and are marked
uncertain; the CLI never automatically resubmits. Read-only RPCs retry at most
three times. Inspect chain state and saved artifacts before manual recovery.
There is no automatic resume/recovery command. Use one process per journal/run.
The minimal journal is not the deferred signed event format, per-run keys, full
log store, or full logging discipline. Other deferred commands and the harness/UI
are not implemented here.

## Validation

```sh
cargo test --manifest-path cli/Cargo.toml --locked
cargo test --manifest-path swarm-tests/Cargo.toml --test cli_core --locked
```

Simulator tests execute the actual CLI command engine against freshly built,
unchanged contract source. They cover every command, three separate anchor
transactions, head mismatch, exact S2 settlement on fresh state, a second run
with persistent reputation, and stopping after failed registration. Unit tests
cover canonical payloads, network/account restrictions, bounded allowances,
redacted credential errors and reputation cap/floor accounting. See
[live-run evidence](../codex-log/cli-s2-live.md) for the real testnet result.
