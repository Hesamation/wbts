# Testnet CLI core

## Local build and simulated S2

After the [contract checks](../contract/README.md#local-build-and-tests), run
these commands from the repository root with the [root prerequisites](../README.md#local-build-and-simulated-s2):

```sh
cargo build --manifest-path cli/Cargo.toml --locked
cli/target/debug/cli --help
cargo test --manifest-path cli/Cargo.toml --locked
cargo test --manifest-path swarm-tests/Cargo.toml --test cli_core --locked
cargo test --manifest-path swarm-tests/Cargo.toml --test cli_core --locked run_demo_s2_watcher_sponsors_exact_money_and_fresh_reputation_then_repeat -- --exact
```

The final command invokes the actual `run-demo s2` command engine using the
in-process simulator and freshly built contract source. It checks the 2.18
verifier PIN below, exact settlement and reputation on a fresh run, then a second
run with persistent reputation. It needs no keys, funds or testnet connection.
There is no standalone binary simulation flag. The standalone CLI suite has six
tests; the full simulator suite has 39, including 11 CLI-engine tests. The
`cli_core` simulator suite also exercises S1, S3, S5, both S6 challenger
variants and the funding gate. The focused S2 command reruns the existing S2 test.

The CLI provides `register`, `anchor`, `file-claim`, `challenge`, `run-demo` and `verify-run`.
Linux builds require the D-Bus development library (`dbus-1`
via pkg-config) for the existing secure keystore. The lockfile pins CLI dependencies;
contract dependencies are unchanged.

Run Clippy for all standalone CLI targets:

```sh
cargo clippy --manifest-path cli/Cargo.toml --locked --all-targets
```

## Live testnet S2 (outside local validation)

```sh
cli/target/debug/cli --contract s218.worker-wbts.testnet \
  --rpc-url https://rpc.testnet.fastnear.com \
  --storage-allowance 25000000000000000000000 run-demo s2 \
  --registrar worker-wbts.testnet
```

`--contract` (or `WBTS_CONTRACT`) is overridable. `--rpc-url` (or `WBTS_RPC_URL`)
selects an RPC; the CLI checks `chain_id=testnet` at connection and before every
signature. Only named `.testnet` accounts are accepted. There is no deployment path.
For the 2.18 pin, pass the fresh contract explicitly as above; the historical
default contract remains the 2.10 deployment and has a different identity.

The current 2.18 deployment and S2 proof are recorded in [the live log](../codex-log/2.18-live.md).
Its initial attempt stopped before filing on insufficient maximum-gas funding;
the fresh worker-sponsored run passed with the allowance shown above.

The historical CLI-core attempt on the default RPC stopped on HTTP 429. Claim 1 was
subsequently settled with builder approval through the alternate endpoint above.
Read [the live log](../codex-log/cli-s2-live.md) and
[recovery/funding evidence](../codex-log/cli-s2-recovery.md) before repeating runs.

Roles default to coordinator/reporter `coord-wbts.testnet`, worker
`worker-wbts.testnet`, challenger `watcher-wbts.testnet`; override with `--reporter`,
`--worker`, `--challenger`. The registered coordinator, worker and watcher must
be distinct. `--challenger watcher|worker` selects S6’s external watcher or worker
self-challenge; explicit challenger account IDs remain supported for existing S2 commands. `--registrar ACCOUNT` selects
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

## Seeded-failure harness

`run-demo s1|s2|s3|s5|s6` uses the same testnet settings as the S2 example above.
Use `--registrar worker-wbts.testnet` for the sponsor-funded demo. For S6 run both
`run-demo s6 --challenger watcher` and `run-demo s6 --challenger worker`, each with
a fresh run ID. No scenario deploys or modifies the contract. Current local/live
status and exclusions: [harness log](../codex-log/harness-s1-s6.md).

| Scenario | Ground truth and settlement |
| --- | --- |
| S1 | Valid input, incorrect 1225/47775 output; unchallenged F1 accepted. Worker run stake loses 0.25; coordinator receives 0.45 including its bond; treasury gains 0.05; worker reputation −10. Optimistic acceptance does not adjudicate truth. |
| S2 | Correct output; false F1 overturned with no finding. Existing exact-money checks retained. |
| S3 | Diagnostic only. Verify intact journal, edit payload at seq 1 in a separate local copy, detect seq 1 and name the on-chain signer. Three setup transactions; no claim or settlement. |
| S5 | Declared total 49000 differs from entries summing to 50000; output 1225/47775. Worker’s F3 claim against coordinator upheld after coordinator self-challenges. Payout 0.65 to worker; coordinator stake −0.25 and challenge bond −0.25; treasury +0.10; coordinator reputation −10 only. |
| S6 | Same malformed input; coordinator’s F1 against worker overturned. Stored finding F3/coordinator, result origin Propagation, finding origin Introduction. Coordinator bond −0.25 and reputation −15; challenger economic gain +0.20; treasury +0.05. Worker +2 reputation, watcher +5, or self-challenging worker +7 (subject to caps). All stakes unchanged. |

Balances use a baseline after setup and exclude measured gas and storage costs.
The registrar supplies **all** locked stakes: stake slashing reduces sponsor-funded
principal. Thus S1’s sponsor/reporter aggregate is −0.05 NEAR even for true blame.
The harness does not assert agent-owned stake or a profit for honest reporting.
S6’s corrected finding records attribution only; a fresh bonded claim and defense
window would be required for a further slash or blame penalty.

Before any take, the CLI checks registrar ≥3.1 NEAR and each other participating
account ≥0.30, plus 0.01 gas headroom. A sponsor also posting a bond needs another
0.30 NEAR, checked before registration (S5 and S6 worker self-challenge when the
worker sponsors). Each take locks another 3 NEAR; check funding for every run.
Existing per-transaction spendable/storage/gas checks still apply.

For a saved journal:

```sh
cli/target/debug/cli --contract s218.worker-wbts.testnet \
  --rpc-url https://rpc.testnet.fastnear.com verify-run --run-id RUN_ID
cli/target/debug/cli --contract s218.worker-wbts.testnet \
  --rpc-url https://rpc.testnet.fastnear.com verify-run --run-id RUN_ID \
  --journal PATH_TO_TAMPERED_COPY
```

`verify-run` recomputes the stored Borsh event chain and compares local payload
bytes with immutable on-chain commitments and head. It prints `OK` on success;
a mismatch prints `MISMATCH at seq N signer=ACCOUNT` and exits nonzero. The signer
is the original on-chain actor, not proof of who edited the local copy. S3 keeps
the intact journal and writes a sibling `*.tampered.json` for inspection.
No signing key is read by `verify-run`.

S4 cut — no F2 verifier; deadline/timeout attribution unimplemented. F4 is a local
diagnostic, with no punishment. F2/F4/F5a adjudicators, nested traversal, panel tier,
and ratchet remain outside this work. Fault tests cover unrelated task failures,
spam challenges and collusion against the deterministic verifier. Sybil ratchet
dodging and panel latency are record-only. The depth/branching limit probe stays
gated until all six live takes succeed.

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
hash is pinned to `6c8f4458183fc6f6e5c3d889d7f9ad07ef26ffbaafaa59d67406e15980b90444`
for the 2.18 build. Earlier deployment identities are preserved in their run logs.
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

Execution height comes from the root receipt referenced by the transaction
outcome. A successful contract call can return `SuccessValue` or forward a promise
as `SuccessReceiptId`; finalize forwards the payout this way. The CLI checks FINAL
execution and all receipt failures, including the forwarded payout and refunds.

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
log store, or full logging discipline. The full `inspect` command and UI remain deferred.

## Validation

Run the [local validation sequence](#local-build-and-simulated-s2) above.

Simulator tests execute the actual CLI command engine against freshly built,
unchanged contract source. They cover every command, three separate anchor
transactions, head mismatch, exact S2 settlement on fresh state, a second run
with persistent reputation, and stopping after failed registration. Unit tests
cover canonical payloads, network/account restrictions, bounded allowances,
redacted credential errors and reputation cap/floor accounting. A captured-response
regression covers finalize forwarding its payout receipt. See
[live-run evidence](../codex-log/cli-s2-live.md) for the real testnet result.
