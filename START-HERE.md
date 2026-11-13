# Who Broke the Swarm — standalone continuation guide

Prepared November 13, 2026. Read this before old plans, local STATUS files or agent proposals. This guide is for an agent with **only this repository**, no prior conversation and no access to the private planning workspace. No other project is needed to understand or continue this one.

**Mission:** find and deliver the strongest credible WBTS-based hackathon submission possible. Preserve the useful engineering; challenge the product assumptions. The goal is competitive merit, not protecting a chosen mechanism or maximizing feature count. No research establishes numerical prize odds or guarantees a win.

WBTS was set aside because its demonstrated task and optimistic dispute policy did not establish a practical advantage over capable escrow and straightforward checking. That is a substantive product/mechanism objection, not just weak marketing. It is also **not a proof that accountable delegation has no useful future**. This guide explains both sides and gives concrete ways to move forward.

This handoff preparation does not itself resume implementation, choose an extension or authorize deployment. The human can assign this repository to a new builder using the launch prompt at the end. Once implementation is assigned, work continuously through relevant tests and demo preparation; do not impose repeated batch approval or documentation ceremonies.

## 1. Read these first; avoid the historical traps

1. This guide: product, built boundary, objections, opportunities and continuation options.
2. [Contract interface](contract/README.md) and [CLI guide](cli/README.md): current v2 behavior and commands.
3. [Research register](handoff/RESEARCH.md): primary-source leads, external-review corrections and measurement provenance.
4. Relevant code/tests in the map below. Read the whole historical archive only for a specific unresolved question.

Implementation baseline inspected for this handoff: **`aa1f531164351e9de832758aeb9f7cf568e42c78`**, on `master`, incorporating lifecycle implementation **`b8e811cd3fa106d7c7179a1ed369562bfe4dc46c`**. The checkout was clean before this documentation addition. Run `git status` and inspect HEAD when you arrive; subsequent commits may supersede this snapshot.

Important traps:

- This is a **fresh-state v2** contract/CLI. Historical live receipts are v1. No state migration or recovery of old deployed stakes was implemented.
- Root `STATUS.md`, `CLAIMS.md`, `HACKATHON.md` and `codex-log/` are ignored local records, possibly absent in a clone. The local STATUS is older than v2. Their test counts, ABI, expiry behavior and funding model must not override current source and interface docs.
- The historical AGENTS body includes creator-funded stakes, old method names and stale scope/tool estimates. Its current preface points here. Latest human instructions govern authorization; actual code establishes behavior. Keep the money-safety/evidence discipline, not obsolete facts or retired process.
- `swarm-tests/legacy-v1/` preserves old tests; these are not active v2 coverage or extra current passes.
- **Two different A/B/C label sets exist.** Old build-scope A/B/C mean finish direct T2 / add another verifier / add nested attribution. Research-direction A/B/C mean coding receipts / optimistic inference / preventive budgets. They are not the same decisions.
- Neither a hash nor an on-chain event proves that external facts are true, that off-chain code ran correctly, or that evidence remains retrievable.

## 2. The original idea, in plain language

Agents owned by different people should be able to hire each other without sharing keys or relying entirely on one platform operator. NEAR can provide shared authenticated state, collateral and enforceable settlement rules between those principals.

WBTS asks: **when delegated work fails, can we distinguish bad execution from bad supplied input, contest a false accusation and make the agreed financial consequences inspectable?**

The aspirational system is a chain of recorded delegations with evidence and explicit obligations. In a deeper chain A → B → C, an eventual system might locate where an invalid result first entered and distinguish it from downstream propagation. But deciding that B fulfilled its checking/delegation obligations, or that C owes A directly, needs a defined contract and evidence. Traversing a graph cannot invent those facts.

The actual code implements **independent direct delegation edges**, not that general chain. An edge is a delegator and worker, their commitments, their independently funded collateral, deadlines and claim lifecycle. A third party may report/challenge. Multiple processes or demo accounts do not demonstrate independent commercial operators or real customer demand.

Original demo beat: a coordinator falsely blames a worker whose result passes the agreed check. A watcher or the worker challenges. Deterministic adjudication overturns the accusation; the reporter loses its bond, the challenger receives a reward, and the innocent worker avoids the blame penalty. If bad input is the actual finding, a separate fresh claim is needed to penalize its supplier.

This is an understandable accountability demonstration. Its practical superiority still needs proof.

## 3. What is built, exactly

### The evaluator: narrow and deterministic

`contract/src/t2.rs` implements one integer-ledger fee-split task, T2:

- Parse ledger entries and declared total; reject malformed decimal strings, overflow, inconsistent totals or invalid entry counts as a precondition fault.
- Compute `fee = floor(total * basis_points / 10000)` and `net = total - fee`, with overflow-safe integer arithmetic.
- Compare output with the **exact expected canonical bytes**. Equivalent JSON formatting can fail this byte predicate; do not describe it as general semantic JSON validation.
- Invalid input maps to **F3/delegator**. Valid input plus wrong output maps to **F1/worker**. Passing work or an incomplete valid task gives no finding. Mere non-completion does not incur an F1/F3 penalty.

It checks internal ledger consistency, **not the truth of the ledger's entries in the real world**. It does not evaluate arbitrary code changes, reasoning, research, model identity, invoices or service delivery. A spec hash pins a specification; it does not authenticate its external facts.

Limits include 100 events/run, 128 ledger entries, 1 KiB spec and 8 KiB per task payload. Consult constants for other aggregate/call bounds. Parent assignment links are explicitly rejected; nested adjudication is absent.

### Contract and accounting

The v2 implementation includes:

- Spec/verifier pinning and sequential tamper-evident event commitments.
- Offers with immutable acceptance/work deadlines; worker acceptance with its own signature and collateral. Registration buys storage, not everyone's stakes.
- Separate delegator/worker collateral per edge; concurrent edges reserve independently even when roles overlap.
- Completion anchored by the accepted worker; completion starts a filing window. Unfinished edges eventually release without inventing a fault from absence alone.
- Full hash-authenticated task evidence stored when a claim is filed. Syntactically checked transaction references are provenance fields, not independent inclusion proofs.
- One active claim; semantic deduplication of F1/F3 attempts; at most one accepted blame settlement per edge. A changed reporter/reference format cannot repeat the same penalty.
- A bonded challenge **atomically** runs deterministic evaluation and persists the verdict. No old separate-disclosure timeout-default verdict is part of v2.
- Permissionless finalization after the defense deadline, exact integer credit allocation and reputation changes.
- A fresh filing/defense window for an eligible corrected-culprit claim after an overturn. Corrected attribution itself does not impose that culprit's penalty.
- Independent edge release and withdrawal credits for unused collateral, returned bonds, rewards and excess deposits. Retained paid storage is excluded from recovery.
- One withdrawal in flight/account; private nonce/amount-checked callback restores credit on transfer failure. An unexpectedly failed **callback itself** can leave ambiguous pending state; no blind retry/rescue guarantee exists.

Defaults: 1 NEAR collateral per role/edge; 0.25 NEAR filing and challenge bonds; 600-block windows, or 60 in demo mode. These are parameter snapshots, not economically calibrated values. Block windows are not guaranteed wall-clock durations. Owner parameter updates affect new snapshots; inspect code before assuming immutable governance.

For slashed amount `x`, reward is `floor(4*x/5)` and treasury receives the remainder. At defaults, before separate gas/storage:

| Outcome | Principal returned to winner | Slashed pool | Winner's total credit | Treasury |
| --- | ---: | ---: | ---: | ---: |
| Unchallenged accepted allegation | Reporter bond 0.25 | Blamed reservation 0.25 | 0.45 | 0.05 |
| Challenged, upheld allegation | Reporter bond 0.25 | Reservation 0.25 + challenger bond 0.25 | 0.65 | 0.10 |
| Overturned allegation | Challenger bond 0.25 | Reporter bond 0.25 | 0.45 | 0.05 |

Credits are claims against contract-held funds, not already received wallet transfers. Do not conflate 0.45 returned-plus-reward with 0.45 profit. Reputation is bounded stored scoring, with **no admission or capital consequence**; it is not Sybil-resistant market reputation. Treasury has no spending method.

### CLI and demos

The Rust CLI handles offer/accept/complete/claim/preview/challenge/finalize/release/balance/withdraw, participant journals and accounting checks. It reads existing named testnet keys; it does not create accounts, deploy contracts or automatically retry uncertain submissions. Participants can run several commands without the creator's journal; missing historical bytes are marked partial. Claim filing still needs exact evidence bytes.

Seeded simulator/harness scenarios:

| Scenario | What it demonstrates |
| --- | --- |
| S1 | Faulty output; unchallenged accepted worker allegation |
| S2 | Passing work; false worker allegation overturned |
| S3 | Edited local bytes detected against commitments; no editor attribution/punishment |
| S5 | Invalid supplied input attributed to delegator |
| S6 | Wrong worker blame overturned; a fresh F3 claim/window precedes delegator punishment; watcher and self-challenge variants |

Fixtures are labeled, constructed arithmetic cases. No real customer workflow, production monitor, general reasoning verifier or nested subcontractor demo has been established.

## 4. Evidence already obtained, and its limits

**Historical v2 checks, November 9:** 28 simulator tests (23 lifecycle, four CLI-engine and one preflight), six standalone CLI tests, strict Clippy, formatting and verifier-pin checks were recorded passing. The lifecycle suite covers role ownership, duplicate/finality/deadline boundaries, credit conservation, overlapping roles, integer vectors, independent edges and withdrawal failure behavior. These are useful prior results, not passes in your environment or an independent audit.

A stress fixture with 100 events and two 8-KiB payloads measured about 5.322 Tgas for atomic challenge. Filing two maximum payloads added 16,950 storage bytes (0.16950 NEAR at the simulator byte price). This is a measured case, not a worst-case theorem. CLI storage allowance needs to cover such evidence; unused allowance becomes credit.

Historical v2 artifact: 303,250 bytes; SHA-256 `ab5da468b809186c870e5374bd493d12223804cce6f5f7073d3d3ed3c05568a8`. This digest is distinct from the CLI's source/manifest verifier identity. Neither establishes a deployment match today.

Historical live v1 S1/S2/S3/S5 journals exist locally. Historical S6 live attempts were funding-blocked. Fresh-state v2 deployment, live end-to-end evidence and clean reproducibility remain unestablished in this handoff. The fresh CI run was last recorded pending; no new CI/RPC check was performed for this document. The evidence UI is unbuilt.

The separate four-fixture verification-cost experiment is preserved portably in [measurements](handoff/mechanism-measurements-20261109.json) and [probe source](handoff/mechanism-cost-probe.rs). It measured roughly 2.18–2.81 Tgas for checking already-filed evidence. At the historical documented floor of 0.0001 NEAR/Tgas, that is about 0.000218–0.000281 NEAR. Check current pricing before live-cost claims.

These measurements include a transaction reading stored evidence; they do **not** measure the exact inline cost of eager verification or a complete rival escrow. Full happy paths, all retained storage, watchers, capital and delays were not compared. “Cheap checker” is supported; “eager escrow dominates all costs” is not.

## 5. Why the project was set aside

### Objection 1: capable escrow can express the same finding

The strongest baseline is not an escrow blindly paying after a human clicks accept. Give the baseline the **same evaluator, authenticated submissions, evidence, deadlines and accepted trust model**. It can check input preconditions, check output, emit responsibility/reason receipts and apply agreed collateral consequences.

For our direct arithmetic predicate, no additional attribution capability has been demonstrated. [Draft ERC-8183](https://eips.ethereum.org/EIPS/eip-8183) permits contract evaluators and extensible checks; it is evidence against an artificially weak baseline, not proof of an equivalent audited NEAR deployment. No complete rival was implemented or benchmarked.

The even simpler baseline for T2 is to compute fee/net in the contract. Why hire an autonomous worker to perform arithmetic the contract can already cheaply redo? “Agents collaborate” does not answer that question.

### Objection 2: optimism has unproved all-in economics

Hash-only normal jobs can avoid publishing/evaluating full payloads. That is a real potential benefit. But detection, monitoring, availability, disputes, delays and locked capital still cost something. An expensive task is not enough: generating an answer may be hard while checking its proof is cheap.

For each task, after genuinely common costs, compare:

`eager checking cost > extra normal-path overhead + monitoring + expected claims/disputes + capital/delay + missed-wrong-outcome risk`

Only if that inequality favors optimism at the required safety level is an economic advantage supported. Normal-path overhead may be negative. No field claim/error rate, paid watcher business, customer risk budget or full break-even was established. Subjective truth or inaccessible data is not solved by adding bonds.

### Objection 3: false unchallenged allegations can become final

Filing authenticates evidence but does not run the semantic check. Challenges do. A pending unchallenged claim can settle even when the stored evidence would disprove it. Accepted settlement has no appeal. This is the selected optimistic policy, not an old duplicate bug that is still unfixed.

At defaults, ignoring fees and outside incentives, a false reporter gains 0.20 NEAR if unchallenged and loses 0.25 if successfully challenged. With effective successful-challenge probability `p`, expected payoff is `0.20*(1-p) - 0.25*p`; nonpositive payoff requires `p >= 4/9`, about 44.44%. This conditional arithmetic is **not** a measured detection rate, equilibrium or security guarantee.

Watchers do not automatically become profitable. If invalid claims occur with fraction `e` and a watcher wins reward with conditional probability `w`, expected gross reward per inspected claim is `e*w*0.20 NEAR`, before all monitoring and transaction costs. The accused worker has its own stake-protection incentive, but must remain available with evidence and funds.

One plausible redesign is checking **every filed claim** immediately: keep hash-only undisputed jobs, but do not accept an allegation the available checker disproves. This would change settlement/reward policy. It is an option, not implemented behavior or a complete economic solution.

### Objection 4: no demonstrated operational need for this task

No confirmed buyer need for agent-performed fee arithmetic, recurring losses needing our blame layer, adoption willingness or measurable integration advantage was established. Direct-contract marketplace access requests are adjacent evidence; they do not validate this product.

### Objection 5: the grander chain story exceeds the implementation

Nested traversal, subcontractor obligation checking and general reasoning verification are absent. Even a correct fault origin is not automatically who owes the customer: A may hold B liable while B pursues C. Automatically exonerating B requires an agreed pass-through policy, adequate collateral and evidence that B fulfilled its own duties. The delegation paper does not prove our preferred liability rule.

### Objection 6: completion/readiness risk

No UI, fresh v2 live demonstration or final release/license/claim reconciliation existed when work paused. These remaining tasks matter, but polishing them would not establish the missing product value. Starting a proof stack, marketplace or arbitrary-code evaluator introduces substantial new dependencies.

Together these justified pausing WBTS. They do not establish a numerical collapse in prize odds, a fatal flaw in every optimistic mechanism, or that no WBTS-based product can succeed.

## 6. The strongest case for continuing

There is reusable work here: authenticated evidence tied to independently funded obligations; corrected-culprit due process; tested exact credit accounting; release/withdrawal behavior; independent participant CLI journals; and a legible false-accusation defense story. Building these correctly is more than writing a hash log.

The strongest remaining hypotheses are:

- **Operational accountability:** a useful receipt changes who gets paid, retried, refunded, excluded or compensated in a real cross-operator job, while reducing diagnosis/integration effort.
- **Cheap normal path, strong claim path:** scarce data publication or expensive proof production genuinely favors selective settlement checking under a credible monitoring model.
- **Compositional responsibility:** recorded preconditions, checks and evidence transfers distinguish introducing a defect from propagating it, across agreed contractual obligations.
- **Developer integration:** a clear SDK/receipt schema and recovery workflow make existing evaluation/escrow machinery much easier to use. No new cryptographic primitive is necessary for product merit.
- **Inspectable adversarial experience:** one real task, normal success, induced failure, false allegation, correction and capital exit are visible and reproducible within a short judge path.

These are opportunities to test, not achievements to advertise. An emerging-market project need not have a mature customer base or a publicly documented financial loss before it can be competitive. A concrete operator request or reproducible workflow friction can suffice. Equally, prettier evidence or another agent cannot substitute for the actual benefit.

## 7. Continuation options and what would justify each

### Existing engineering scope A/B/C

| Option | Reuse / new work | What must be established |
| --- | --- | --- |
| **Scope A: finish direct T2 lifecycle/evidence UX** | Reuse most contract/CLI; add honest state/evidence UI, reproducible demo and readiness work | A defensible bounded submission story despite unresolved arithmetic demand and optimistic risk. Lowest new mechanism work, not a validated strategic winner. |
| **Scope B: second useful deterministic verifier** | Reuse lifecycle; add precise task semantics, evidence binding and vectors | An actual useful delegated job, meaningful responsibility distinction and advantage over ordinary checks/evaluator escrow. Not just more complicated arithmetic. |
| **Scope C: nested attribution** | New parent admission, obligation/causal semantics, traversal and collateral/liability handling | A concrete residual failure that edge-local contracts/recourse do poorly; check B's duties explicitly. Nesting alone does not rescue economics or demand. |

These were historical expansion labels, not blanket authorization for a new agent to implement all three.

### Research direction A — coding-job acceptance/responsibility receipts

Pin repository revision, environment, acceptance commands, party submissions and expected fixture preconditions. Distinguish invalid supplied fixture, failing patch, passing patch and unavailable evaluator. Potential benefit: reproducible evidence and simpler cross-operator settlement/diagnosis.

Prior research found adjacent coding-agent operations and a direct-contract marketplace request, but no demonstrated receipt-adapter advantage. Existing CI, signed receipts, evaluator-equipped escrow and Suwappu Arbiter are strong comparators. Suwappu's inspected rubric checked PR metadata/merge rules, not arbitrary code quality. Tests passing is compliance with a predicate, not proof of general software quality or moral fault.

An external report found a real refund/cancellation integration bug, fixed by stopping work on the refund event. It supports lifecycle integration pain; it does not establish demand for bonded coding blame. Its proposed paired trial was unexecuted; ignore language implying a measured duration.

Reopen for one actual job plus an observable integration/diagnosis/recovery advantage. Use the **same isolated evaluator** in both arms. The prerequisite evaluator/sandbox and authenticated settlement path must exist; a fixture hash is not fixture validity and a container is not by itself a proven malicious-code boundary. Previous 20–36-hour estimates were unmeasured and assumed a ready sandbox.

### Research direction B — optimistic exact-model inference / proof on demand

Possible benefit: avoid producing an expensive proof for every honest inference, then enforce the pinned statement on challenge. This is different from claiming inference is expensive so optimism wins.

Need a useful exact workload, committed model/runtime/tokenizer/input/output semantics, accessible evidence, working NEAR-compatible verifier, data-availability failure rules, monitor funding, griefing protection and total-cost comparison against recomputation, eager proof and an accepted attested service. A cryptographic primitive or EVM verifier is not a ready NEAR integration. Model execution provenance does not prove its answer is wise or true.

Prior report constructed a public short-context moderation scenario, identified relevant proof research and alternatives, but delivered no workload buyer, break-even or compatible integration. Previous 4–10 developer-day estimate was unvalidated and assumed verifier readiness. This was rejected for delivery risk, not a general impossibility theorem. Reopen only with ready executable dependencies and an explicit economic experiment.

### Research direction C — preventive delegated procurement budgets

Sponsor → coordinator → independent buyers, with a finite root budget, narrowed child authority, allowlisted vendors, ancestor revocation, reservations and recovery. Potential benefit: contain losses before a dispute, with a complete procurement journey.

It is substantially a new payment/delegation subsystem; WBTS provides experience, not that implementation. NEAR function-call key allowance pays gas, not a general token budget; a restricted zero-deposit call can still spend vault-held funds. Compare a trusted gateway, separate prefunded accounts, capable policy escrow, Leash, Espalier, Rootbound and MetaMask's EVM delegation machinery.

No observed buyer workflow requiring this hierarchy was established. Public competitor descriptions cover much of it. Neither hierarchy nor independent keys prove real demand; a budget vault cannot cap off-platform API bills or ensure an allowed merchant delivers good work. External C simulation files were missing, and some sibling-budget comparisons used unequal assumptions. Do not inherit their claims as benchmarks. Previous 20–35-hour estimate was unmeasured.

Reopen for a specific purchased service and demonstrably better setup/revocation/recovery at equal custody and trust requirements. Missing hierarchy in one flat wallet is not evidence of an empty NEAR niche.

### Other WBTS-shaped routes worth considering, not a second mandatory roadmap

- Evaluate filed allegations automatically, retaining cheap hash-only normal jobs; redesign rewards/finality explicitly rather than pretending this is UI polish.
- Make evidence/receipts a useful integration module on existing escrow or market contracts instead of building a new marketplace. Show the saved work or improved recovery on the same job.
- A debugging/flight-recorder tool can help operators even without slashing, but needs a real workflow and a clear reason for NEAR anchoring over ordinary signed logs. Removing economics changes the product claim.
- Choose another bounded deterministic work family only after obtaining its preconditions, output predicate, evidence source and business consequence. Do not invent a new oracle by naming it a verifier.

## 8. How a new agent should turn this into progress

Start with one concrete user/job and the consequence of failure. Ask what the customer actually needs: payment acceptance, compensation, attribution for retry, auditability, or authority limits. Define a task whose evaluator/evidence can really be supplied. Give the baseline the same resources and trust assumptions.

For the best candidate, run one paired executable comparison. Include normal success, invalid input, invalid output, false allegation, missing evidence/evaluator and safe capital exit where applicable. Measure something meaningful: integration steps, setup/reproduction effort, time to a useful diagnosis, retained data cost, all-in settlement cost or failed-payment recovery. Include an honest control; a system refusing every job is not safer useful service.

Do not claim universal superiority from a thin harness, or call a self-authored simulation a benchmark of real contracts. State measured versus estimated work. Verify the few primary claims that could change the decision, instead of repeating broad competitor searches or provenance bookkeeping.

Choose one end-to-end slice with a credible remaining-work inventory. The minimum persuasive submission needs a real task/result, independently authenticated parties, understandable accepted obligations, inspectable verdict/money effects, a demonstrated attack/defense, release/recovery and a short reproducible judge path. If changing the evaluator or economic policy is necessary, present one concrete recommendation and its consequences to the human; routine implementation then runs through tests and demo preparation without phase stops.

Do not preserve optimism, nested delegation, the name or the current architecture solely because work already exists. Do not reject a worthwhile integration simply because programmable escrow could implement it. Use evidence and remaining delivery risk. If no candidate clears that standard, say so honestly; a polished arithmetic demo does not become a validated product because alternatives are weak.

## 9. Code map and local bootstrap

| File | Purpose |
| --- | --- |
| [contract/src/lib.rs](contract/src/lib.rs) | v2 admission, log, claims, adjudication mapping, settlement, release/withdrawal, views |
| [contract/src/t2.rs](contract/src/t2.rs) | Actual predicate, canonical output and verifier bounds/identity |
| [cli/src/lib.rs](cli/src/lib.rs) | Command engine, journal/evidence behavior |
| [cli/src/live.rs](cli/src/live.rs) | RPC/signing/accounting reconciliation |
| [cli/src/harness.rs](cli/src/harness.rs) | Seeded scenarios and money checks |
| [swarm-tests/tests/lifecycle.rs](swarm-tests/tests/lifecycle.rs) | v2 state/economics/boundary/failure checks |
| [swarm-tests/tests/cli_core.rs](swarm-tests/tests/cli_core.rs) | CLI engine through simulator |
| [swarm-tests/tests/common/mod.rs](swarm-tests/tests/common/mod.rs) | Simulator fixture/setup helpers |
| [swarm-tests/fixtures/t2_integer_vectors.json](swarm-tests/fixtures/t2_integer_vectors.json) | Preserved integer cases |
| [scripts/sync-verifier-pin.py](scripts/sync-verifier-pin.py) | CLI source/manifest identity synchronization |
| [.github/workflows/ci.yml](.github/workflows/ci.yml) | Two build/test jobs and native prerequisites |

No root Cargo workspace: use the three explicit manifests. Preserve lockfiles/dependency pins, including near-sdk 5.29.1. Historical tested tooling: Rust/Cargo 1.99.1, cargo-near 0.23.0 and `wasm32-unknown-unknown`. Inspect installed versions before installing/upgrading; report compatibility failures rather than disguising them as test failures. Native dependencies are detailed in [README](README.md), including libclang, OpenSSL, D-Bus and libudev prerequisites.

From the repo root, using a compatible installed toolchain:

```sh
git status --short --branch
git log -3 --oneline
rustc --version
cargo --version
cargo near --version
rustup target list --installed
python3 scripts/sync-verifier-pin.py --check

export CARGO_BUILD_JOBS=2
cargo near build non-reproducible-wasm --manifest-path contract/Cargo.toml --locked
cargo test --manifest-path swarm-tests/Cargo.toml --locked --tests -- --test-threads=2
cargo test --manifest-path cli/Cargo.toml --locked
cargo run --manifest-path cli/Cargo.toml --locked -- --help
```

Use the documented Clippy/formatting checks for touched crates. Simulator checks need no NEAR keys, account funding or testnet RPC. Dependency downloads may need network. Host build success is not clean-clone or Docker reproducibility. In the original RTK environment prefix shell invocations with `rtk` (`rtk proxy` for raw commands); an external environment need not install RTK solely for this guide.

After authorized source/manifest edits that affect verifier identity, run the pin synchronization script, inspect the diff and rerun the relevant checks. Merely editing this handoff must not change the verifier pin or runtime source.

To reproduce the historical cost probe, ensure `swarm-tests/tests/research_cost.rs` does not exist, copy [handoff/mechanism-cost-probe.rs](handoff/mechanism-cost-probe.rs) there, and run:

```sh
CARGO_BUILD_JOBS=2 cargo test --manifest-path swarm-tests/Cargo.toml --locked --test research_cost -- --nocapture
```

Preserve results separately; remove only your newly copied test afterward. Existing measurements were not rerun to prepare this guide. The extra file adds a test; do not mix its count into historical active-suite results.

## 10. Track, deadline, claims and authority

The live [organizer instructions](https://nearhackathon.com/HACKATHON.md) were rechecked November 13: development/submission ends **November 14, 2026, 23:59 UTC**, which is **November 15, 03:29 Tehran**. Verify again before acting. Do not plan against the obsolete November 11 internal target. Eligible local/testnet work still needs reproducible evidence and the required public materials; release/submission are human actions or require explicit authorization.

The official [swarm example](https://github.com/nearai/near-hackathon-prompt-examples/blob/main/tracks/05-swarms.md) permits coordinated economic agents and a scripted simulation. Strict spontaneous emergence is not the eligibility bar. WBTS can plausibly fit delegated economic coordination; security or agent-economy fit depends on the final product. Eligibility alone does not make it competitive. Do not assume hints are mandatory integrations, invent prize odds or rely on old directory counts. Multiple-project eligibility/independence must be checked with the human before submission; this guide selects no submission arrangement.

Current user working preferences: never use Astra; do not spawn agents without explicit authorization. Routine reversible engineering/private commits are autonomous once work is assigned. Ask for material strategy/scope/economic changes, spending, deployments, public release, external messages and submission. Do not revive the retired collab/OMP workflow or mandatory batch planning/decision ledgers. Keep one short status when stopping: what works, checks/failures, next action.

Do not publish credentials, participant keys, private local notes or unreviewed journals. Existing local-only files remain local-only; this guide and its small research assets were deliberately prepared for portability. Check repository visibility/remote before pushing; a private working push and a public release are separate actions. No force-push or destructive history edits.

Claims that remain prohibited without new evidence: general agent-reasoning verification, nested causal adjudication, guaranteed fair/correct blame, always-profitable watchers, production safety, formal verification, independently verified TEE attestation, a full job-fee payment marketplace, current v2 live success or novelty/priority. Do not resurrect the discarded reserve/BlameDAG payout model. Call the implemented mechanism an optimistic challenge model, not TrueBit's interactive verification game.

## 11. Ready-to-paste standalone launch prompt

The human should paste this when actually assigning continuation; its presence in the repository is not authorization by itself:

> Read START-HERE.md, handoff/RESEARCH.md, the v2 contract/CLI guides and relevant source. You are taking WBTS forward from this baseline, not reconstructing our previous conversation. Challenge both the positive and negative conclusions with concrete evidence. Aim for its strongest credible, finishable hackathon submission; do not preserve sunk-cost features or invent winning odds.
>
> I authorize read-only investigation and bounded local comparisons. Establish one useful workflow, the strongest same-trust baseline, the precise improvement and the critical evaluator/integration prerequisite. Use existing research instead of restarting a broad survey. Recommend one concrete implementation slice, showing remaining work and policy changes; resolve material strategy/economic choices with me. After I assign that implementation, complete it through appropriate tests and demo preparation without batch approvals. Do not spend, deploy, publicly release, contact others or submit without authorization. Do not use Astra or spawn extra agents. Keep status short and evidence inspectable. If no defensible improvement emerges, say so rather than turn a weak premise into a larger build.

If the human already specifies/authorizes a concrete build slice in their message, do not ask them to authorize the same work again. This prompt is a starting aid, not a restriction on their later instructions.

## Handoff verification

Prepared from current source/interface inspection, the recorded November 9 implementation evidence, mechanism measurements and November 10 candidate/external reviews. Critical negative findings were retained alongside their limitations; no new product benchmark, regression suite, CI refresh, deployment or customer validation was performed. Supporting numerical probe files are byte-preserved from the private planning repository; their hashes/provenance are recorded in the research register. No private planning checkout or ignored historical log is required to start from this guide.
