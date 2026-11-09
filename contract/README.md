# Direct-T2 attribution contract (fresh-state v2)

This ABI replaces creator-funded run stakes. Existing deployments are not migrated.
Historical v1 receipts are not evidence of v2 behavior. Only direct fee-split F1/F3
is supported, using the optimistic challenge model.

## Local build and tests

From the repository root, using the prerequisites in the main README:

```sh
python3 scripts/sync-verifier-pin.py --check
cargo near build non-reproducible-wasm --manifest-path contract/Cargo.toml --locked
cargo test --manifest-path swarm-tests/Cargo.toml --locked --tests -- --test-threads=2
cargo clippy --manifest-path contract/Cargo.toml --locked --target wasm32-unknown-unknown
cargo clippy --manifest-path swarm-tests/Cargo.toml --locked --all-targets
```

Host build success is not a clean or Docker reproducibility claim. The simulator
builds current source; archived tests in `swarm-tests/legacy-v1/` document the old ABI
and are not counted as current passes. Active tests cover independent edge funding,
causal findings, exact money flows, duplicates, deadlines, integer vectors,
withdrawal failure/retry and the actual CLI command engine.

## Lifecycle and interface

`register` stores the validated spec/verifier pin and agent list; it charges storage,
not participant stakes. `offer_assignment` reserves the caller/delegator's stake;
`accept_assignment` reserves the named worker's own stake. Defaults are 1 NEAR per
role per edge, and 0.25 NEAR for each claim/challenge bond. Concurrent edges require
separate reservations, including when the same account occupies both roles.

IDs are `{run_id, assignment_seq}`. Offers set immutable absolute acceptance/work
block deadlines with creation < acceptance < work. Admission is inclusive at a
deadline; expiry requires a later block. `anchor` may record completion only by an
accepted worker before the work deadline; assignment creation cannot bypass `offer_assignment`.

Completion starts a 600-block filing window (demo 60). Unfinished assignments use
the work deadline plus that window, without inferring F1/F3 from absence alone.
`get_edge` exposes `normal_claim_deadline`, `release_after`, active claim and residual
reservations. `release_after` is an exclusive time boundary; an active claim still
blocks release. A participant never freezes unrelated edges.

`file_claim(edge_id, failure_class, payloads, evidence_tx_hashes)` stores exact
hash-authenticated input and any existing output. F1 derives the worker; F3 derives
the delegator. F1 needs a completion. Payloads are ordered assignment then completion;
each is at most 8 KiB. The spec is at most 1 KiB and the run at most 100 events.
Malformed payload bytes may themselves prove a fault and are not rejected as JSON.
Transaction references are syntactically checked provenance, not inclusion proofs.

`preview_adjudication(claim_id)` evaluates stored evidence. `challenge(claim_id)`
posts the snapshotted matching bond and atomically persists the deterministic verdict;
there is no disclosure expiry verdict or separate adjudication transaction.
Permissionless `finalize` requires height > claim's defense deadline and allocates
credits/reputation once. A corrected finding alone imposes no new blame penalty.

One active claim and one attempt per F1/F3 allegation are permitted per edge,
regardless of reporter or reference formatting. One accepted blame closes liability.
After an overturn naming the other unattempted cause, finalization opens one fresh
filing window for its independent bonded claim/defense. No appeals follow accepted
settlement: an unchallenged false claim can become final.

`release_edge` credits residual reservations after eligibility closes and disputes
settle (or immediately after accepted blame settlement). Abandoned offers release
the delegator after acceptance expiry. `get_credit` exposes available, in-flight,
nonce and paid retained-storage amounts. All excess deposits are credits; they are
not detached refunds. Record storage is retained and is not recoverable.

`withdraw(amount)` requires exactly 1 yoctoNEAR and >=30 Tgas. That yoctoNEAR is also
credited. One transfer may be in flight per account, to the same account only.
The private nonce/amount-checked callback consumes a successful transfer or restores
a failed one. If the callback itself fails unexpectedly, the in-flight record stays
pending; no unsafe retry/rescue is provided for that ambiguous result.

For slashed x, reward is floor(4x/5), treasury the remainder. Defaults yield winner
credits/treasury of 0.45/0.05 NEAR (unchallenged accepted), 0.65/0.10 (upheld challenge)
and 0.45/0.05 (overturn). Blamed slash is min(bond, its edge reservation). Storage/gas
are separate; allocated credit is not a successful wallet transfer. Reputation
remains a score, with no admission/capital consequence or post-hoc updates outside
these windows. Treasury has no spending method.
