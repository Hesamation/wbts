# Arena research 1 — preserved WBTS continuation evidence

Integrated November 14, 2026. WBTS remains paused. This package records a possible redesign, its checked technical path and the product objections that remain. It does not select that redesign or modify WBTS's contract/CLI.

## Read this package

- [REPORT.md](REPORT.md): original Arena research 1 report, unchanged. Its author had the two handoff documents, not the WBTS repository; claims about WBTS reuse were not source-verified by that author.
- [research.zip](research.zip): complete original archive, unchanged. Contains the separate research contract, harness, dependency locks, supplied WASM, recorded receipts and provenance, saved attestation reports, comparison results and source snapshots. Extract into a separate temporary directory. Do not merge its code into production merely by unzipping it.
- [offchain-fixtures-rechecked.json](offchain-fixtures-rechecked.json): earlier review's independent recheck of two recorded request/response hashes and EIP-191 signatures.
- [wasm-verifier-recheck.json](wasm-verifier-recheck.json): earlier review's narrow local-sandbox checks on the supplied WASM.
- [MANIFEST.json](MANIFEST.json): archive/report hashes, source locations and explicit verification limits.

No private planning checkout is required to use these files. The archive supplies the source, fixtures and original reproduction instructions. Dependency downloads may be needed. Scripts can install tooling, refresh external observations or replace result files; inspect them before running. No credentials/private keys are included in the copied file list; recorded public signatures and signer addresses are evidence, not API access.

## What the author proposes

Replace the arithmetic task with settlement against a NEAR AI Cloud request/response signature. The buyer commits exact request bytes, a model and job terms; the seller supplies a model-serving signature binding request and response hashes. An owner-registered key is checked on-chain. Input faults and invalid receipts are evaluated immediately instead of becoming penalties when nobody challenges.

Add a job price and an agreed invalid-input kill fee. Preserve one window for response-byte availability: acknowledge delivery, or require publication of matching bytes after a bonded challenge, or refund if the seller does not reveal. Fault classification and who owes money remain separate contractual terms. This is an evaluator escrow redesign, not nested attribution or a proof that arbitrary agent work is correct.

It is a new **attested-service trust model**, distinct from the earlier expensive cryptographic proof-on-demand proposal. Read it as a new branch of that investigation, not evidence the original optimistic inference economics were vindicated.

## What actually survived review

The complete package is substantive; its supplied WASM digest matches the report. In the earlier November 14 review:

- Both recorded request/response byte hashes and EIP-191 signer recovery were independently rechecked. The signer matches the identity in the supplied later attestation record.
- The supplied receipt-verifier WASM was executed on local near-sandbox 2.14.0. An owner-registered key accepted the recorded receipt and rejected a changed request hash.
- Contract/harness source and saved comparison results were inspected. The archive records 42 scenarios and 143 lifecycle steps; this full matrix was **not independently rerun**.

These are earlier review observations, **not new passes from this integration task**. The original recorded completions are from November 1, not fresh model calls. A later attestation match does not replace contemporaneous preflight evidence. Intel collateral/TDX verification, source-to-WASM clean rebuilding, NVIDIA/image provenance and measurement allowlists were not independently repeated. Do not advertise a general verified-TEE system or live v2 WBTS integration from these checks.

## Objections still unresolved

1. **Why buy the relay?** When the buyer specifies every request byte, the seller principally provides an API account. The report does not establish why the intended buyer would choose this over direct API access. Useful wallet-only access or a substantive seller contribution is possible but unvalidated.
2. **Same-capability escrow remains the comparator.** An ordinary evaluator escrow can implement the same acknowledge/timeout/reveal terms. Differentiation would need demonstrated workflow or integration value, not a capability monopoly.
3. **Privacy is conditional.** A challenge forces public disclosure. A user who requires confidentiality even against a malicious counterparty may not accept that remedy. “Private until challenged” must be stated literally.
4. **Cheap checking favors immediate checks here.** The archive's measured cost hypothesis supports its eager-check choice, not optimism for expensive generation. Its allegation-policy model is a reimplementation of the handoff, not measured WBTS v2 execution.
5. **Headline costs are partial.** The recorded 143 lifecycle steps exclude 69 separately recorded withdrawal calls, signer registration and storage funding. Contracts are prefunded. Internal ledger conservation and cleared credit fields do not establish exact wallet reconciliation or safe failure recovery.
6. **Trust is explicit.** The owner registers keys after an off-chain check; the contract does not validate TDX quotes. API acceptance, signer freshness, accepted runtime/image, job nonce/domain binding and data availability require stated policy. A valid signature proves neither answer usefulness nor who performed a fresh job.
7. **The probe is not production accounting.** Its withdrawal path lacks failure callbacks. Do not replace WBTS's tested credit/recovery machinery with this probe's simpler implementation without a deliberate design and relevant failure tests.
8. **Commercial terms are a reported dependency.** The author flags possible restrictions on pure service resale. This package supplies a research lead, not a legal determination or authorization to operate such a service.

## What this changes for WBTS

It adds an executable candidate verifier and a concrete eager-settlement design to the handoff. It does **not** resolve useful demand, show an advantage over capable escrow, justify nested liability, prove favorable monitoring economics, or deliver an integrated WBTS submission. The arithmetic baseline and current v2 behavior remain unchanged.

The author's runner-up—evaluate all filed T2 claims—could remove one false-allegation risk, but still leaves the weak arithmetic job. Missing API access is not a product reason to select that fallback.

## If the human explicitly revives WBTS later

The first material question is one useful delegated job where responsibility/evidence changes acceptance, compensation, retry, recourse or diagnosis. Resolve that before porting this evaluator, extending nesting or polishing a UI. Then compare the same job and trust assumptions with the best existing evaluator/escrow flow.

If this receipt direction survives that question, test a fresh nonce-bound completion under the stated preflight policy, with separately authorized spending if needed; specify privacy and key governance; and choose an implementation slice with payment, evidence and recovery tests. No model-key purchase, external message, public deployment/release or submission is authorized by preserving this archive.

**Closeout:** knowledge and evidence are preserved; no runtime change, regression rerun or renewed WBTS build. Remaining work belongs to a deliberate future revival, not this documentation closeout.
