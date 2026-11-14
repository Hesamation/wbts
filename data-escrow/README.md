# Proof-backed fact escrow

Fresh-state contract and participant client. This is a separate ABI from the
preserved T2 attribution contract. No old account, state or stake migration exists.

## Terms and trust

A seller publishes a dataset name and root under its own authenticated account.
The `(seller, name)` record is immutable. A buyer opens a job naming that seller,
dataset, 1–8 distinct fact keys, a positive price and absolute block deadlines.
The root is copied into the job. The seller must accept the terms before delivery.

The supported proof is `inferpay-keyed-merkle-sha256-v1`, restricted to string-valued
`fact` entries and authenticated empty slots. Keys are lowercase ASCII slugs
containing letters, digits, `_`, `-` or `.`, up to 80 bytes. Figure evaluation,
regular-expression rules and arbitrary model text are outside this predicate.

Dataset membership establishes consistency, not truth or source authenticity.
The buyer selects the seller/root it trusts. Delivered entries and proofs are
public, including calldata, logs and retained state. There is no private delivery
or TEE assumption. The seller's unpublished full dataset can remain off chain;
the bundled sample data is constructed and public.

| Action | Authority and effect |
| --- | --- |
| `publish_dataset(name, root, source_uri)` | Caller publishes its immutable root; pays retained storage. |
| `open_job(seller, dataset, keys, price, accept_deadline, work_deadline)` | Buyer attaches price plus storage. Requires current height < acceptance < work deadline. |
| `accept_job(job_id)` | Named seller only, through the inclusive acceptance deadline. No extra bond. |
| `deliver(job_id, index, opening)` | Accepted seller only, through the inclusive work deadline. Verifies pinned root and requested key before allocating money. Caller pays retained proof storage. |
| `cancel_job(job_id)` | Buyer only, before seller acceptance. Credits the remaining price back. |
| `close_job(job_id)` | Seller may refund the remaining work early. |
| `expire_job(job_id)` | Anyone, strictly after the acceptance deadline for unaccepted work or work deadline for accepted work. Refunds the remaining price. |
| `withdraw(amount)` | Caller only; exactly 1 yoctoNEAR, at least 30 Tgas. `null` withdraws all credit, including the attached yocto. |

Invalid proofs roll back the entire call. The seller can submit a corrected proof
before the deadline. Each requested index can resolve once. A present entry
credits that key's share to the seller; a proven empty slot refunds its share to
the buyer. Unresolved keys wait for expiry or early closure. All resolved keys
close the job immediately. There is no unchecked allegation, watcher, treasury
cut or reputation penalty.

For price `P` and `n` requested keys, index `i` gets `floor(P/n)` plus one yocto
when `i < P % n`. All shares sum exactly to `P`, independent of delivery order.
Zero-price individual shares are possible when `P < n`; the seller sees these
terms before acceptance.

## Storage, accounting and transfers

Funded calls measure actual new storage after flushing persistent collections.
Insufficient deposits revert. Excess deposits become credit; records are retained
and their storage charges are excluded from recovery. Credit slots are funded in
advance, so acceptance, closure, expiry and withdrawal callbacks need no new
record storage.

`get_accounting` reports the allocation invariant:

```
deposits = locked job prices + available credits + in-flight withdrawals
         + confirmed withdrawals + retained storage charges
```

This ledger identity is not an audit or sufficient evidence of wallet solvency.
The views also expose contract balance and storage usage. Deployment/base storage,
operator funding and external data acquisition are outside per-job accounting.

Withdrawals use one in-flight slot/account, a nonce, the exact amount and a private
callback. Failed transfers restore credit. A callback that itself fails leaves
an ambiguous pending slot; automatic retries and administrator rescue are absent.
The test harness labels fault injection and tests both behaviors. An account's
new credits can accumulate while a withdrawal is pending, but it cannot start a
second transfer until the first callback resolves.

Views: `get_protocol`, `get_dataset`, `get_job`, `get_delivery`, `get_credit`,
`get_job_count`, `get_accounting` and `check_opening`. The latter accepts both valid
membership and valid vacancy proofs. `get_protocol` includes the current block
height and limits. There is no global listing scan in a settlement call.

## Build and exercise

From the repository root, with the main README's pinned tools and prerequisites:

```sh
export CARGO_BUILD_JOBS=2
node data-escrow/compat/generate.mjs
cargo near build non-reproducible-wasm --manifest-path data-escrow/Cargo.toml --locked
cargo test --manifest-path swarm-tests/Cargo.toml --locked --test data_escrow -- --test-threads=2
cargo build --manifest-path cli/Cargo.toml --locked --bin data-cli
cargo run --manifest-path swarm-tests/Cargo.toml --locked --example data_demo -- demo/evidence
python3 scripts/check-data-demo.py
```

The tests include JavaScript-to-Rust proof compatibility, generated dataset/client
integration, wallet reconciliation, eight-key rounding, exact deadlines, forged
values, wrong keys, invalid shapes, unauthorized callers, duplicate delivery,
underfunding rollback, independent dataset identities, partial/absent delivery,
refunds, failed transfers and ambiguous failed callbacks.

## Prepare your own dataset

Node 22.23.0 is the tested runtime. No npm packages are required.

```sh
node scripts/data-facts.mjs seal data-escrow/compat/coverage.json .wbts-data/seller
node scripts/data-facts.mjs prove .wbts-data/seller/sealed.json mri-copay .wbts-data/opening.json
```

The first command is a public sample. For real use, supply your own array of
`{topic, value, source}` strings. Values are at most 512 UTF-8 bytes, sources at
most 1,024. Fresh salts prevent simple sibling-hash guessing. The tooling refuses
duplicate slots and overwriting existing commitments. Preserve `sealed.json`
privately; `catalog.json` contains only the root and key names.

The tool supports up to 10,000 dataset entries; a job buys at most eight keys.
The 32-bit sparse key space permits collisions. The seller tool detects occupied
collisions; it cannot produce a vacancy for a different key in an occupied slot.
Such a requested key must remain unresolved and be refunded. This is a bounded
inherited proof format, not a collision-free dictionary guarantee.

## Separate testnet participants

No deployment has been established by the bundled local evidence. A fresh
authorized deployment must be initialized with `new`. Do not target an old T2
contract. Its full-access deployment key can replace the code unless removed by
its owner; root immutability is a property of this code, not a promise about an
operator who can upgrade it.

The following commands are for **an already authorized testnet deployment** with
existing independently held keys. Set `DATA_CONTRACT`, `DATA_BUYER` and
`DATA_SELLER` to the actual named testnet accounts. They are deliberately unset
here. Every transaction needs separately funded balance for attached deposits and
gas; the client does not create keys, accounts or funding.

Read-only chain/ABI inspection:

```sh
cli/target/debug/data-cli --contract "${DATA_CONTRACT:?set the actual data contract}" protocol
cli/target/debug/data-cli --contract "$DATA_CONTRACT" accounting
```

Publish the catalog root from the seller account:

```sh
DATA_ROOT=$(python3 -c 'import json; print(json.load(open(".wbts-data/seller/catalog.json"))["root"])')
cli/target/debug/data-cli --contract "$DATA_CONTRACT" publish \
  --seller "${DATA_SELLER:?set the actual seller}" --name my-data-v1 --root "$DATA_ROOT"
```

Read the current block from `protocol`, then select absolute acceptance/work
deadlines with enough time for participants. The example price is 0.02 NEAR:

```sh
cli/target/debug/data-cli --contract "$DATA_CONTRACT" open \
  --buyer "${DATA_BUYER:?set the actual buyer}" --seller "$DATA_SELLER" \
  --dataset my-data-v1 --keys mri-copay --price 20000000000000000000000 \
  --accept-deadline "${DATA_ACCEPT_DEADLINE:?set a future block}" \
  --work-deadline "${DATA_WORK_DEADLINE:?set a later block}"
```

Use the actual returned job ID; no shared creator journal is needed:

```sh
cli/target/debug/data-cli --contract "$DATA_CONTRACT" accept \
  --seller "$DATA_SELLER" --job-id "${DATA_JOB_ID:?use the actual returned ID}"
cli/target/debug/data-cli --contract "$DATA_CONTRACT" deliver \
  --seller "$DATA_SELLER" --job-id "$DATA_JOB_ID" --index 0 --proof .wbts-data/opening.json
cli/target/debug/data-cli --contract "$DATA_CONTRACT" job --job-id "$DATA_JOB_ID"
cli/target/debug/data-cli --contract "$DATA_CONTRACT" delivery --job-id "$DATA_JOB_ID" --index 0
cli/target/debug/data-cli --contract "$DATA_CONTRACT" withdraw --signer "$DATA_SELLER"
cli/target/debug/data-cli --contract "$DATA_CONTRACT" withdraw --signer "$DATA_BUYER"
```

The buyer can read the stored delivered fact and proof even if it was offline for
settlement. If the seller never accepts or leaves a key unresolved, any existing
funded caller can invoke `expire --signer <account> --job-id <id>` after the
applicable deadline. Use `balance` to inspect credit and pending withdrawals.

Funded client calls attach a configurable storage allowance, default 0.05 NEAR,
capped by the client at 0.25 NEAR. This is a deposit allowance; unused portions
become withdrawable credit. The client checks testnet identity before signing,
matches deployed bytes to `--wasm` (default the built data-escrow artifact), and
records public intent/outcomes under ignored `.wbts-data/transactions/`. It stops
on an uncertain submission instead of automatically retrying. Investigate the
transaction hash and receipts before deciding whether to retry manually.
