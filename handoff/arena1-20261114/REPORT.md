# WBTS revival: recommendation, evidence and first slice

Prepared 2026-11-14 at about 06:50 UTC, roughly 17 hours before the submission deadline: 2026-11-14 23:59 UTC, which is 03:29 on 15 November in Tehran, re-read today from the live [organizer rules](https://nearhackathon.com/HACKATHON.md).

**Terms.** Buyer and seller are WBTS's delegator and worker. A model enclave is the trusted execution environment that serves a model on NEAR AI Cloud. A receipt is that enclave's signature over a request hash and a response hash. Tgas is 10^12 gas units; at today's mainnet gas price 1 Tgas costs 0.0001 NEAR, about $0.0003.

**Limits of this investigation.** The WBTS repository was not available to me. I had `START-HERE.md` and `RESEARCH.md` only, so every statement about WBTS code is taken from those files and marked "handoff says". I had no NEAR AI API key and made no new model call. Nothing was deployed, spent, published or sent. Everything executed is in `research/probe/` and reruns with `research/probe/reproduce.sh`.

## 1. Verdict

**Do not carry forward the optimistic challenge mechanism or the fee-split task.** Both main objections hold, and this investigation adds measurements to them. Checking a real receipt costs about 0.6 Tgas, while any design that postpones the check needs a later transaction costing at least 1.9 Tgas, so postponing cannot save gas. Under the policy the handoff describes, a false allegation against a valid, real receipt became final and took 0.25 NEAR from the innocent worker when nobody challenged.

**A stronger design exists.** Replace the fee-split evaluator with one that agents on NEAR can satisfy today: the per-response signature that a NEAR AI Cloud model enclave produces over the hash of a request and the hash of its response. A buyer agent commits the hash of the exact request it wants answered and escrows the price. The seller agent is paid when it posts the enclave's signature for that request. Every other outcome has a named fault and a fixed payout, decided in the transaction that raises it. One time window survives, for the one question a contract cannot answer from hashes: whether the buyer received the response bytes.

**How much stronger.** Against WBTS as built, substantially: objections 2 and 3 no longer apply to any claim the evaluator can decide, and the mechanism gets a real evidence type. Against the strongest baseline, a capable evaluator escrow with the same verifier, only narrowly, because the recommended design is such an escrow. Its remaining edge is that the buyer keeps recourse for withheld bytes without any response being published, and that a failed job records whether the request or the receipt was at fault and pays accordingly. A buyer who does not need payload privacy gains nothing from the window, and in that case no design here earns a meaningful advantage over a capable evaluator escrow.

**Two cautions.** The category is crowded: a bounded search today found at least four hackathon entries that verify this same signature on-chain and four more that check it off-chain. And I found no evidence that anyone wants to buy a fully specified model call from another operator; that remains a hypothesis.

**Recommendation.** Build the slice in section 9 only if a NEAR AI API key with credits is available today and no stronger project competes for the same hours. Without a key, the fallback is the runner-up in section 12, which repairs the mechanism but still has no useful job.

## 2. Recommended behavior and user

**User.** The operator of an agent that pays other operators' agents for model calls and must account for each payment. This is the "general contractor" of the organizers' [Agent Economy prompt](https://github.com/nearai/near-hackathon-prompt-examples/blob/main/tracks/07-agents.md), which asks that the agent "checks the work" and keeps "a receipt for every task ... and whether the work passed the agent's check". The counterparty is the operator of a seller agent who wants to be paid without depending on the buyer's goodwill.

**One job.**

1. The buyer composes the exact request bytes, including a per-job nonce. It opens a job naming the seller, the model, the SHA-256 of the request, a price, a kill fee (what the seller receives if the buyer's request turns out to be unusable), a deadline and a window, and escrows price plus kill fee. It sends the bytes to the seller off-chain.
2. The seller forwards those exact bytes to NEAR AI Cloud, fetches the signature, returns bytes and signature to the buyer, and posts the response hash and signature on-chain. The contract recovers the signer and requires that it is registered for that model, that the registration has not expired, and that the receipt has not been used before.
3. The buyer acknowledges, which pays the seller at once, or stays silent, which pays the seller when the window ends.

**Faults.** The evaluator reports where the fault originated. The payout is a separate per-job term.

| Fault origin | Raised by | Decided by | Payout in the probe |
| --- | --- | --- | --- |
| `DelegatorInput` | Seller reveals the committed request bytes | Template check, same transaction | Seller gets the kill fee, buyer gets the price back |
| `WorkerNoReceipt` | Anyone, after the deadline | No accepted receipt exists | Buyer refunded in full |
| `WorkerWithheld` | Buyer, with a bond, inside the window | Seller does not publish bytes matching the delivered hash in time | Buyer refunded, bond returned |
| False claim by seller | Seller claims a valid request is invalid | Evaluated at filing | Nothing moves, counter on the seller |
| False claim by buyer | Buyer claims non-delivery | Seller publishes the bytes | Seller gets price plus the bond |

**What changes for the operator.**

| | Agent Market today, per its docs | Recommended |
| --- | --- | --- |
| Acceptance | Manual, or automatic 3 to 7 days after submission. On the wallet-only x402 path: "settle and auto-accept at once, no rework, no dispute" | Automatic on the receipt, a few blocks |
| Compensation | Refund or payout, or a split decided by an automated judge that runs about every 12 hours | Fixed per fault; the seller is compensated when the buyer's request caused the failure |
| Retry | Request changes | The fault says which: fix the request, or change seller |
| Recourse | Dispute with a 5% deposit, then optional human review | Terms fixed when the job opens |
| Diagnosis | Job thread | One log line with fault and reason |
| Access | Reviews and a success rate | Counters written only by evaluated verdicts |

Sources: [payments and disputes](https://docs.market.near.ai/concepts/payments-and-disputes), [x402 vs MCP](https://docs.market.near.ai/a2a/x402-vs-mcp), both fetched today. This compares documented behavior with a probe; I did not run an Agent Market job.

## 3. Strongest baseline and the concrete difference

The baseline gets the same verifier, evidence, trust and capabilities. I ran three of them, because "capable escrow" has two honest forms.

- **Binary escrow.** Money outcomes of the [ERC-8183 draft](https://eips.ethereum.org/EIPS/eip-8183): completed pays the provider, rejected or expired refunds the client.
- **Capable escrow, hashes only.** Typed verdicts and a kill fee; delivery is a hash and a signature; paid at once.
- **Capable escrow, bytes published.** The same, but the response bytes must be in the delivery transaction.

Same job in every column: one real recorded receipt for `Qwen/Qwen3.8-27B`, price 0.05 NEAR, kill fee 0.005. Only rows where the designs differ are shown.

| Case | Binary escrow | Capable, hashes only | Capable, bytes published | Recommended |
| --- | --- | --- | --- | --- |
| Success | Seller +0.05. 2 transactions, 4.96 Tgas | Seller +0.05. 2 transactions, 5.15 Tgas | Seller +0.05. 2 transactions, 5.34 Tgas. 900 response bytes public | Seller +0.05. 3 transactions, 7.15 Tgas. Nothing public |
| Buyer's request cannot be attested (seeded: `stream` flipped to `true`) | Expires at the deadline, refund. On-chain record identical to a seller that never delivered | `DelegatorInput`. Seller +0.005 after 6 blocks | Same as hashes only | Same as hashes only |
| Valid receipt, bytes never handed over | Seller keeps 0.05. Buyer has no claim to file | Same as binary | Cannot happen | `WorkerWithheld`. Buyer refunded |
| Buyer falsely claims non-delivery | No such claim | No such claim | No such claim | Seller publishes the bytes and receives 0.055 |

For a tampered response, a receipt from another model, a receipt for another request, a reused receipt and an unregistered signer, all four designs refuse the delivery in its own transaction and refund the buyer at the deadline. A seller that never delivers leads to the same refund.

**The actual improvement over the strongest baseline** is one property. The hashes-only escrow gives the buyer no recourse when bytes are withheld. The bytes-published escrow gives recourse but publishes every response. The recommended design gives recourse and publishes nothing unless challenged. It costs one more transaction, about 2 Tgas or 0.0002 NEAR, and either an acknowledgement or a wait.

Full table: `research/probe/results/02_paired_probe.md`.

## 4. Mechanism alternatives and the two objections

### What happened to each alternative

| Alternative to the original mechanism | Disposition | Reason |
| --- | --- | --- |
| Evaluate every filed claim | Adopted | Removes the false allegation that becomes final. A false claim costs its author one transaction and moves no money |
| A useful second verifier | Adopted: the NEAR AI receipt | The evidence exists today and verifies on-chain for about 0.6 Tgas |
| Explicit contractual recourse | Adopted | Payouts are per-job terms; the evaluator only reports fault origin |
| Optimism | Kept in one place | The reveal claim, where the eager alternative means publishing every payload |
| Bonds | Kept in one place | The reveal bond, sized to cover the seller's reveal transaction rather than to deter |
| Receipts or integration on an existing escrow | Not the product | Agent Market's published API exposes no evaluator hook, and a custody-free receipt anchor already exists as a public entry (finding 13). The verifier here is a view call, so another escrow could use it later |
| Nested obligations | Not built | Each hop's request would have to commit to the previous hop's response, and a single edge has no demonstrated user yet |

### Escrow objection: accepted for custody

The recommended design is an evaluator escrow and should be described as one. What WBTS contributes is the evaluator's output (fault origin, kept apart from payout terms) and the one rule an eager escrow cannot have without publishing payloads. The ERC-8183 draft's core has two money outcomes; a typed payout schedule is an extension there too.

### Always-check objection: accepted for every claim decidable from commitments

The handoff's test is whether eager checking costs more than everything optimism adds. Measured on nearcore 2.14.0, protocol 88, the version mainnet runs today:

| Term | Postponed check (handoff policy, modelled here) | Check at once |
| --- | --- | --- |
| Checking | Avoided unless challenged | 0.62 Tgas per receipt, about 0.00006 NEAR |
| Normal-path overhead | One more transaction at 2.4 to 2.6 Tgas. A clean job took 4 transactions and 8.3 Tgas | None. 2 transactions, 5.2 Tgas |
| Monitoring | The accused or a watcher must watch every window | None |
| Disputes | 0.25 NEAR bond on each side | A false claim costs its author one transaction, 2.7 Tgas |
| Capital and delay | 2 NEAR per edge plus bonds; 600 blocks is about 6 minutes at today's 0.6 s block time | The price; about 3 blocks |
| Missed wrong outcome | 0.25 NEAR taken from an innocent worker when unchallenged, reproduced on a real receipt | None for decidable claims |

The cheapest state-changing transaction in the probe cost 1.92 Tgas. A check that costs 0.62 Tgas cannot be made cheaper by moving it into a transaction that would not otherwise exist.

### Where optimism is retained, and what it costs

A contract cannot tell from hashes whether the buyer received the bytes. The cheapest acceptable eager alternative is to publish the bytes with every delivery.

| | Always publish | Reveal on challenge |
| --- | --- | --- |
| Gas, normal job | 5.34 Tgas at 0.9 KB, 6.76 Tgas at 8 KiB | 7.15 Tgas at any size |
| Payload made public | Every job | Challenged jobs only |
| Settlement delay | None | An acknowledgement, or the window |
| Monitoring | None | The buyer checks its own delivery. No third party |
| Evidence availability | Not needed | The seller must keep the bytes and answer within the window |
| Dispute | Not needed | 2 more transactions: 5.0 Tgas at 0.9 KB, 6.4 Tgas at 8 KiB. Bond of 10% of the price |
| Capital | The price | The price for the window, plus the bond when challenging |
| Wrong-outcome risk | None | One job's price: a buyer who ignores non-delivery, or a seller offline for a whole reveal window |

Always publishing is cheaper in gas below roughly 10 KiB of response (linear extrapolation from two sizes) and settles at once. Reveal on challenge is therefore not a cost optimisation. It is worth keeping only when responses must stay private. NEAR AI Cloud sells private inference, so that is plausible for this workflow, but it is a judgment and not a measurement.

### What changes, stated as the handoff asks

- **New guarantee.** No allegation the evaluator can decide is settled without the evaluator running. A valid receipt for the committed request pays the seller whether or not the buyer cooperates. A buyer who did not receive the bytes can force their publication or a refund within one window.
- **Financial consequences.** The 1 NEAR collateral per role and the 0.25 NEAR bonds go away for decidable claims. Funds at stake become the price and kill fee, plus a 10% bond only when a buyer challenges delivery. The watcher role and its reward disappear, because nothing is left for a watcher to do. No treasury cut arises unless a penalty is added on purpose.
- **Code impact (handoff says; not inspected).** The evaluation now inside the bonded challenge moves to completion and to claim filing. The challenge path, reward split and defense window are unused for decidable claims. One window type remains for the reveal claim. A price escrow is new, since WBTS holds collateral and not a job price.

### Fault origin and liability stay separate

The evaluator returns an origin. The job's terms map it to money. A job could set the kill fee to zero and still record `DelegatorInput`. In a chain A to B to C, a receipt can show the fault arose at C, while B still owes A under their terms and recovers from C under its own. Nothing in this slice passes liability through automatically, and nesting is not built.

## 5. Primary-source evidence

Basis column: **Run** means executed here today; **Code** means source read at a pinned commit; **Doc** means the source says so; **Handoff** means taken from the two uploaded files and not checked.

| # | Finding | Basis |
| --- | --- | --- |
| 1 | NEAR AI Cloud returned an attestation report for `Qwen/Qwen3.8-27B` bound to my nonce. The Intel TDX quotes of the gateway and the model verified with `dcap-qvl` 0.7.1: TCB up to date, debug off, report data equals signer plus my nonce, configuration measurement and RTMR3 replay match. | Run. `results/01_attestation_check.json` |
| 2 | Two receipts recorded by a third party on 2026-11-01 hash correctly and recover to `0x835a…aa26`, the signer in the report from finding 1, thirteen days later. | Run. `results/00_fixture_check.json` |
| 3 | A contract on nearcore 2.14.0 accepts that real signature with `env::ecrecover` and refuses every altered variant. 42 scenario runs, 143 transactions, ledgers balanced, all credits withdrawn, 15 of 15 required failures failed with the expected message. | Run. `results/02_paired_probe.json` |
| 4 | A third party's live testnet contract returns `true` for the same signature and `false` for a wrong model or altered hash. | Run, read-only RPC. `live/testnet_third_party_contract_view.json` |
| 5 | Mainnet today: protocol 88, gas price 0.0001 NEAR per Tgas, storage 0.00001 NEAR per byte, `ecrecover` 0.279 Tgas, block time about 0.6 s (0.592 and 0.597 in two samples). | Run. `live/`, `results/03_economics.json` |
| 6 | Two-transaction settlement costs about $0.0017. The recorded call cost about $0.00013 at list price. Settlement drops below 15% of the call only for calls above roughly $0.012. | Run plus list prices. `results/03_economics.json` |
| 7 | One of the two real receipts is a validly signed response with no answer text: `max_tokens` was 16 and the model spent it reasoning. | Run. Same file |
| 8 | The model proxy signs `model:request_hash:response_hash`. Only a trusted-token caller, the gateway, can override the request hash; a customer key cannot. The proxy keeps a signature in memory for 1,200 s by default; I did not establish how long the gateway keeps its copy. | Code. `sources/excerpts/CODE_EXCERPTS.md` |
| 9 | Streamed replies, aliased models and the Responses API are gateway-signed by default, so they yield no model-enclave signature. Instances of a model share one signing key. The evidence does "not yet form a complete model-to-Gateway-to-final-response chain". | Doc. [Response signatures](https://docs.near.ai/cloud/verification/cloud-api/response-signatures) |
| 10 | Users of model APIs "have no guarantee that providers deliver them faithfully"; output-based detection of substitution is unreliable; enclaves are proposed as the practical fix. | Paper. [arXiv:2504.04715](https://arxiv.org/abs/2504.04715) |
| 11 | "We're building sovereign agents that hold their own keys and operate directly on-chain ... the agent ... has to trust your backend to execute correctly." Open since April, updated 2026-10-21. | First-person request. [IronClaw #2077](https://github.com/nearai/ironclaw/issues/2077) |
| 12 | "The shop delivers before it is paid ... The remedy is an escrow that releases on a verified proof, and it is not built." | First-person, a competing entry. [InferPay README](https://github.com/taejeekim/InferPay), 2026-11-14 |
| 13 | Entries that verify this signature on-chain: [tee-receipts](https://github.com/tmdry4530/tee-receipts), [mandate-near](https://github.com/Makabeez/mandate-near), [PatchBond](https://github.com/PatchBond/patchbond), [pactum](https://github.com/s70239176-ctrl/pactum). Off-chain: InferPay, [SwarmGuild](https://github.com/0xNexuz/SwarmGuild), [surety](https://github.com/surety-protocol/surety), [keyhole](https://github.com/mohamedwael201193/keyhole). SwarmGuild already escrows against enclave-signed work, decided by a reviewer panel. Several use the signature to authenticate an AI judge's verdict rather than the purchased work. | Doc: their READMEs. Only finding 4 was run. Not a census |
| 14 | Agent Market's public listing returned 50 agents with 480 delivered jobs in total. Two listings mention enclave inference. A keyword search of its published OpenAPI (122 paths) found no path for filing or deciding a dispute and no evaluator hook. | Run, bounded. `results/04_market_snapshot.json` |
| 15 | NEAR AI Cloud terms, updated 2026-10-29: a customer may make the services available to end users "as integrated into or through the Customer Application", and may not "sell, resell, sublicense, transfer, or distribute any or all of the Services". | Doc. [Terms](https://near.ai/terms-of-service), §4.1 and §4.2(x) |
| 16 | Organizer rules re-read today: deadline 2026-11-14 23:59 UTC; testnet-only and local demos are eligible (Q-010, Q-012); no integration is mandatory (Q-011); entries are screened by coding agents for reproducibility (Q-017). | Doc. [Rules](https://nearhackathon.com/HACKATHON.md), copy in `sources/HACKATHON.md` |
| 17 | WBTS defaults, accounting table, T2 gas of 2.18 to 5.32 Tgas, reusable components. | Handoff |

Paths in this table are under `research/probe/` or `research/`.

**Why this reopens the inference direction.** The handoff closed it for lack of a ready NEAR verifier and an explicit economic experiment. Findings 1 to 6 supply both, for the attested-service trust model. The result goes against the old pitch: the enclave signs every response at no extra cost, so there is no expensive proof to postpone and no case for optimistic inference. I did not reopen coding receipts or policy wallets. I found no new evidence for either, only more entries occupying both.

## 6. The probe

`research/probe/` contains a small contract with one evaluator and six settlement policies, a sandbox harness, two evidence checks and a cost script. Section 3 comes from four of the policies. The other two model the handoff's policy and its "evaluate every filed claim" variant on the handoff's defaults:

| Same false allegation against a valid real receipt | Delegator | Worker | Challenger |
| --- | ---: | ---: | ---: |
| Nobody challenges in the window | +0.20 | −0.25 | |
| Worker challenges in time | −0.25 | +0.20 | |
| Watcher challenges in time | −0.25 | 0 | +0.20 |
| Evaluated at filing | 0 | 0 | |

These figures reproduce the handoff's accounting table. They come from my re-implementation of the described policy and are not measurements of the WBTS contract.

Two further checks ran separately and passed: an expired signer registration is refused, and a signer registered for one model cannot satisfy a job for another (`results/05_extra_checks.json`).

To rerun: `cd research/probe && ./reproduce.sh` (15 to 20 minutes), or `SKIP_PROBE=1 ./reproduce.sh` (about 2 minutes, keeps saved probe results). The contract build reproduced byte for byte here.

Three build faults surfaced and are worth knowing before porting: near-sdk 5.30.2 gates `env::ecrecover` behind `unstable`; current Rust emits WebAssembly features that nearcore 2.14.0 rejects; and near-sdk 5.30 uses host functions only when compiled with `--cfg near`. The third one produced a first run in which every panic lost its message. I caught it, rebuilt and reran; all reported results are from the corrected build. `cargo-near` may handle all three, which I did not test.

## 7. Minimum demo and honest claim boundary

**Demo, one continuous run on testnet or a local sandbox** (the rules allow a local demo with a video, Q-012):

1. Preflight. Fetch an attestation report with a fresh nonce, verify it with `scripts/01_attestation_check.py`, register the signer.
2. Success. The buyer agent commits a request hash and escrows; the seller agent makes a live call; the contract accepts the signature; the buyer acknowledges; the seller withdraws.
3. Attack with real evidence. The seller calls a cheaper model and submits that genuine receipt. The contract refuses it and the buyer is refunded at the deadline. This is model substitution, and it needs no seeding.
4. Buyer fault, labelled seeded. The buyer commits a request with `stream: true`. The seller reveals it and receives the kill fee.
5. False claim. The buyer claims non-delivery; the seller publishes the bytes and receives the bond.

Track fit, for the human to decide: `agents` first and `privacy` second. `swarms` would need several sellers competing for a job, which this slice does not have.

**What the demo may claim.** A key that NEAR AI Cloud's attestation binds to a verified TDX quote for model M signed this request hash and this response hash, and the request hash is the one the buyer committed.

**What it must not claim.**

- That the answer is correct or useful. Finding 7 is a valid receipt for an empty answer.
- That the response was the only sample. A seller can call several times and submit the reply it prefers.
- Independently verified GPU evidence, image provenance or serving-instance identity. Only the TDX quote and its bindings were checked.
- Trustless signer registration. In this slice the contract owner registers the signer after an off-chain check.
- Independent customers, live usage or production safety. Demo accounts under one operator show the mechanism, not a market.
- Novelty for verifying NEAR AI signatures on NEAR.

## 8. Reusable and new

The left column is what the handoff says exists. I did not inspect it.

| Reusable (handoff says) | Changed | New |
| --- | --- | --- |
| Offers with deadlines; worker acceptance with its own signature | The evaluator runs at completion and at filing, not inside a bonded challenge | Receipt verifier: about 90 lines in the probe (`recover_address`, `check_receipt`, `check_request_template`) |
| Sequential event commitments; spec and verifier pinning | F3 means the committed request fails the template; F1 means no valid receipt or withheld bytes | Signer registry: address, model, attestation hash, expiry |
| Completion anchored by the accepted worker | Bonds, rewards and treasury split are unused for decidable claims | Price escrow and kill fee |
| Hash-authenticated evidence at filing, 8 KiB per payload | One window type remains | Reveal claim and its bond |
| One active claim, deduplication, one settlement per edge | The S2 false-accusation scenario becomes "rejected at filing" | Seller script: forward exact bytes, fetch signature, post |
| Permissionless finalization; exact credit allocation | | Buyer script: compose with nonce, commit, verify locally, acknowledge or challenge |
| Release and withdrawal with failure callback | | Attestation preflight (exists: `scripts/01_attestation_check.py`) |
| CLI commands, participant journals, simulator test structure | | |

If moving the evaluation call proves awkward in the WBTS lifecycle, the alternative is to harden the probe contract. It lacks WBTS's withdrawal failure handling, storage accounting, deduplication and any security review.

## 9. First implementation slice and acceptance cases

One job type, one model (`Qwen/Qwen3.8-27B`, confirmed to return model-enclave signatures in the recordings), one buyer, one seller.

| # | Case | Must hold |
| --- | --- | --- |
| A1 | Live receipt for a request carrying a job nonce | `NoFault`; seller receives the price; buyer gets the kill fee back |
| A2 | Committed request fails the template | `DelegatorInput`; seller receives the kill fee; revealing any other bytes is rejected |
| A3 | Genuine receipt from another model | Refused in the delivery transaction; `WorkerNoReceipt` at the deadline; full refund |
| A4 | Altered response hash, receipt for another request, reused receipt, unregistered signer, expired registration | Each refused; no state change beyond a counter |
| A5 | Seller claims a valid request is invalid | Rejected at filing; job stays open; seller can still deliver |
| A6 | Buyer claims non-delivery; seller reveals | Seller receives price plus bond; wrong bytes are rejected |
| A7 | Buyer claims non-delivery; seller silent | `WorkerWithheld`; buyer refunded with bond |
| A8 | No delivery | Refund at the deadline, not before |
| A9 | Buyer never acknowledges | Anyone can finalize after the window, not before; seller paid |
| A10 | Exit | Every credit withdrawable; contract ledger sums to zero; WBTS's withdrawal-failure test still passes |
| A11 | Policy regression | No decidable allegation can settle unevaluated |

The probe passes A2 to A10 on its own contract, with recorded receipts in place of live ones (A4's expiry case is in `results/05_extra_checks.json`). A1 and the port to WBTS are untested. One improvement to make in the port: a receipt that does not match the job surfaces in the probe as `signer_not_registered` with an arbitrary recovered address; name it "receipt does not verify for this job".

## 10. Critical dependency and remaining work

**Critical dependency.** A NEAR AI Cloud API key with credits, and one live call showing that a request with a per-job nonce still returns `signature_kind: provider_tee` whose signed text matches the exact bytes sent. Until that call is made, the only real receipts are a third party's recordings.

**Prerequisites, exactly.**

- API: `POST /v1/chat/completions` with the canonical model id, `x-no-aliasing: true`, non-streaming, `Accept-Encoding: identity`; `GET /v1/signature/{id}?signing_algo=ecdsa` with the same key; `GET /v1/attestation/report` (no key needed).
- Formats: signed text `model:sha256(request):sha256(response)` in lowercase hex; EIP-191 secp256k1 signature of 65 bytes, or Ed25519.
- Evidence access: only the organization that made the call can fetch its signature. The model proxy keeps it in memory for 20 minutes by default and I did not establish the gateway's retention, so the seller should fetch it immediately.
- Keys: the seller's API key; testnet accounts for buyer, seller and contract.
- Approvals: human authorization for testnet deployment, repository publication, the X post and the submission.
- Terms: a seller that only relays calls could be read as reselling the service (finding 15). A seller that integrates the call into its own application for its own end users is the permitted pattern. This needs a human reading before any claim beyond a demo.

**Remaining work.** The live call; the verifier, registry and price escrow in the contract; moving evaluation to completion and filing; the reveal claim; seller and buyer scripts; acceptance tests A1 to A11; testnet run with transaction links; a README written for a machine reader as "do X, see Y" (Q-017); license file; the demo video and post.

**Economics to state plainly.** Settlement costs more than a small call on a cheap model (finding 6). One job per call suits calls above about a cent. Small calls need one job covering a batch of receipts, which is not built or measured.

## 11. Strongest objection

The evaluator is strongest exactly where the service is thinnest. The receipt binds work to a job only when the buyer fixes every byte of the request. Then the seller contributes an API key and little else, and the buyer could call NEAR AI Cloud itself and verify the same signature for free. As soon as the seller adds a private prompt, private data or tools, the buyer can no longer commit the request, and nothing short of revealing the seller's prompt ties the receipt to the job.

What can be said in reply is limited. Two buyer types do fit: agents and contracts that hold a wallet but no API account, and services whose prompt is public by design, which is how several hackathon entries run their enclave judges. Neither is shown to want to buy the call from another operator. I rate this objection as unanswered on demand and answered only on mechanism.

## 12. Runner-up

**Evaluate every filed claim in the existing T2 contract.** Run the evaluator in the filing transaction and delete the challenge game. The probe shows the behavioral change: the false allegation that took 0.25 NEAR moves nothing. It needs no API key and the least new code. It still asks a worker to do arithmetic the contract can do, so objections 1 and 4 stand. Choose it only if no key can be had today.

## 13. Next implementation action

Make one live call and check its receipt against the probe contract. Not executed here, for lack of a key.

```sh
# with NEAR_AI_API_KEY set by the human; costs well under one cent
printf '%s' '{"model":"Qwen/Qwen3.8-27B","messages":[{"role":"user","content":"job:<contract>:<id>:<32-byte hex nonce>\nReply with one word: hello"}],"max_tokens":400,"temperature":0,"stream":false}' > request.bin
curl -sS https://cloud-api.near.ai/v1/chat/completions \
  -H "Authorization: Bearer $NEAR_AI_API_KEY" -H 'Content-Type: application/json' \
  -H 'x-no-aliasing: true' -H 'Accept-Encoding: identity' --data-binary @request.bin -o response.bin
ID=$(python3 -c "import json; print(json.load(open('response.bin'))['id'])")
curl -sS -G "https://cloud-api.near.ai/v1/signature/$ID" --data-urlencode 'signing_algo=ecdsa' \
  -H "Authorization: Bearer $NEAR_AI_API_KEY" -o signature.json
```

Pass when `signature.json` has `signature_kind` equal to `provider_tee`, its `text` equals `Qwen/Qwen3.8-27B:<sha256 of request.bin>:<sha256 of response.bin>`, and `check_receipt_view` on the sandbox contract returns `ok` for those values (see `research/probe/harness/smoke.mjs`). Keep the nonce inside the message text for this first test. The gateway's request type keeps unknown top-level fields, but whether the model backend accepts them was not tested.

Three choices need the human before the build, because they change strategy or economics: dropping the challenge game and its bonds; adding a price escrow, which WBTS never had; and accepting an owner-registered signer for the demo.

## 14. Not verified

- WBTS source, tests and CLI. Every reuse statement rests on the handoff.
- A fresh receipt, nonce binding, Ed25519 receipts and the encrypted request path. No API key.
- NVIDIA GPU evidence, image provenance and measurement allowlists.
- Every gateway code path that computes the request hash. I read the model proxy's rule and the gateway's forwarding of the header.
- Competing entries beyond their READMEs, apart from one live contract query.
- Agent Market behavior beyond its documentation and public listing.
- The build through `cargo-near`.
- Demand. A bounded search that finds no buyer does not show there is none.
