# FactSettle execution room

[Open the hosted demo](https://factsettle.memorabletulip2.chatgpt.site) or the
[two-minute judge guide](https://factsettle.memorabletulip2.chatgpt.site/guide.html).
No wallet, sign-in or API key is needed.

From the repository root, with Python 3 installed:

```sh
python3 -m http.server 8765 --bind 127.0.0.1 --directory demo
```

Open <http://127.0.0.1:8765/>. This static viewer reads the included execution
records. It makes no RPC calls, loads no wallet and sends no transactions.

1. Open **Buyer won't approve**. The specialist earns 0.02 virtual NEAR from two
   valid fact openings, with no buyer approval call. Both parties withdraw credit.
2. Select **Seller invents a value**. The contract rejects the altered fact. The
   job price is refunded after expiry.
3. Select **One fact never arrives**. Only the delivered key earns its share;
   the remainder returns to the buyer.
4. Change a fact value under **Check the evidence** and press **Check proof**.
   Your browser recomputes the proof; the changed value fails to open to the root.
5. Inspect the wallet table and download a full execution record. Job payments,
   gas, retained storage, wallet changes and withdrawal callbacks are separate.

The sample policy entries are constructed public fixtures from InferPay, plus
additional compatibility vectors. They are neither real insurance terms nor
private customer data. Membership proves consistency with a seller-published
dataset; it does not establish the dataset's external truth.

## Execute new runs

Use the native prerequisites and pinned toolchain in the repository README.

```sh
node data-escrow/compat/generate.mjs
export CARGO_BUILD_JOBS=2
cargo run --manifest-path swarm-tests/Cargo.toml --locked --example data_demo -- demo/evidence
python3 scripts/check-data-demo.py
```

The exporter builds current contract source, runs signed calls in the pinned NEAR
simulator, asserts settlement and participant wallet accounting, executes actual
withdrawal callbacks, and exports the outcomes. A scenario fails the command if
its checks do not pass. No NEAR keys or testnet funding are used.

`evidence/manifest.json` pins each scenario file and the executed WASM digest.
The browser checks scenario-file integrity before displaying it. A committed
record remains historical evidence; regenerate to exercise current source.

## Boundaries

- This is a replay of real local contract execution, not a live testnet deployment.
- Scripted agents have distinct simulator signing accounts. This does not prove
  independent commercial operators or customer demand.
- All delivered entries and proofs are public. There is no confidentiality claim.
- The price in the examples is illustrative. Retained storage is substantial for
  tiny purchases; the wallet table includes it. Contract deployment/base storage
  and external data acquisition are outside the displayed per-run totals.
- The ledger check is an accounting invariant, not an audit or production-safety claim.
- Code owners retain NEAR deployment-key powers. Check the exact deployed code
  and trust the chosen operator before using an external deployment.
