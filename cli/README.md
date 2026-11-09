# Testnet CLI — fresh-state v2

The CLI reads existing named `.testnet` account keys, checks RPC chain identity,
checks the compiled verifier source pin and saves public intent before signing.
An uncertain submission is never automatically retried. It creates no accounts,
keys or deployments. Historical v1 testnet contracts cannot serve this ABI.

## Local build and simulated S2

From the repository root, after installing the main README prerequisites:

```sh
python3 scripts/sync-verifier-pin.py --check
cargo build --manifest-path cli/Cargo.toml --locked
cargo test --manifest-path cli/Cargo.toml --locked
cargo test --manifest-path swarm-tests/Cargo.toml --locked --test cli_core -- --test-threads=2
cargo clippy --manifest-path cli/Cargo.toml --locked --all-targets
cargo run --manifest-path cli/Cargo.toml --locked -- --help
```

These tests use the simulator, without keys, RPC or testnet funding. After editing
contract source or its dependency manifest/lockfile, run `python3 scripts/sync-verifier-pin.py`
then build again. CI checks pin consistency; this identity is not the deployed WASM hash.

## Commands and accounting

Use `--contract` to select a separately authorized fresh v2 testnet deployment.
`register` requires three distinct demo agents, but buys no stakes for them.
`offer` requires signer, worker, input file, absolute acceptance/work block deadlines;
`accept` requires the worker's own signature/deposit. `complete` anchors that worker's
exact output bytes. Other `anchor` records also hash exact bytes, without JSON rewriting.
The canonical JSON helper remains available for callers who deliberately canonicalize
before producing the committed bytes.

`file-claim` takes blamed account (checked against the edge role), F1/F3 and ordered
assignment/completion evidence sequences from the journal. Exact bytes are included
at filing. `challenge` persists adjudication atomically. `preview`, `finalize`, `edge`,
`release`, `balance` and `withdraw` expose the rest of the lifecycle. Each command's
`--help` lists arguments; monetary CLI inputs are integer yoctoNEAR.

`edge` shows deadlines, active liability and per-role reservations. `balance` shows
available credit, in-flight transfer and retained paid storage; inspect edges for
reserved collateral. `withdraw` requires a positive amount up to available credit,
attaches exactly one yoctoNEAR and verifies the callback result and remaining credit.
Accept/complete/challenge/finalize/release/withdraw can start from chain state on a
separate participant machine without the creator’s journal. Such journals mark missing
historical payload bytes explicitly and cannot claim full journal verification.
Filing still requires exact producer evidence; availability before filing is not guaranteed.

Failed transactions stop the command chain and preserve a failure record; inspect
chain and journal before any manual retry.

Storage allowance defaults to 0.05 NEAR per funded call and can be set up to 0.25
NEAR with `--storage-allowance` for maximum-size evidence. Unused allowance becomes
credit, not an immediate refund. Paid record storage remains retained. Journals
separate attached deposits, measured storage and `excess_credited_yocto`; the legacy
`excess_refunded_yocto` field is zero for new transactions. Economic proofs must count
credits/reservations as well as wallets; no allocated credit is labeled a received payout.
Account or contract activity concurrent with a measured live call invalidates its
balance/storage reconciliation; the CLI stops instead of inventing an explanation.

`run-demo s1|s2|s3|s5|s6` uses labeled seeded arithmetic fixtures. Each participant
funds its own collateral. S1 accepts a faulty-output claim optimistically; S2 overturns
a false claim against passing work; S3 detects edited local bytes without punishing
an editor; S5 upholds bad-input attribution. S6 overturns worker blame and then files
a fresh F3 claim against the delegator, with a new defense window before its penalty.
Both watcher and worker self-challenge are supported. A registrar funds registration
storage only. The harness checks settlement, releases residual reservations, withdraws
credits and records confirmed receipts. Seeded fixtures demonstrate the mechanism;
they are not evidence of real customer demand or arbitrary agent reasoning.
