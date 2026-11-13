# WBTS research and evidence register

Companion to [START-HERE](../START-HERE.md), prepared November 13, 2026. This is a portable synthesis of previous work, not a fresh independent validation of every linked source. Follow links only for a question that matters to the chosen continuation.

## Evidence hierarchy and interpretation

Use the latest explicit human instructions for authority; use current executable source and primary evidence for factual claims. A saved agent conclusion is a hypothesis until its supporting evidence is inspected. A paper can motivate a mechanism without validating our chosen implementation, reward calibration or liability rule.

Separate (1) source/documentation claims, (2) inspected code, (3) executed simulation, (4) live chain observation and (5) customer demand. None substitutes automatically for the next. Report dates, versions and trust boundaries. Do not treat bounded searches with no result as proof of no market or no prior art.

## 1. Mechanism and economics

| Source | Prior finding / why consult it | Limit |
| --- | --- | --- |
| [Current t2 checker](../contract/src/t2.rs) and [adjudication/settlement](../contract/src/lib.rs) | Direct input-validity/output-byte predicate; authenticated role mapping; challenge runs semantics; pending allegations can finalize without it | Inspected at baseline `aa1f531`; no new tests/audit in this handoff |
| [ERC-8183 draft](https://eips.ethereum.org/EIPS/eip-8183) | Contract evaluators and extensibility support a capable-escrow comparator | Draft/reference design, not proven equivalent security, deployment, UX, cost or NEAR integration |
| [Intelligent AI Delegation](https://arxiv.org/html/2602.11865v1), especially §4.8 | Recursive checking and recourse along delegations; relevant to contractual responsibility | The described A-holds-B-liable/B-pursues-C policy differs from automatically exonerating B to A |
| [UMA optimistic oracle](https://docs.uma.xyz/protocol-overview/how-does-umas-oracle-work) | Optimistic settlement includes monitoring and escalation incentives | Voting/oracle trust differs from WBTS's whole-task arithmetic checker |
| [TrueBit paper](https://arxiv.org/html/1908.04756) | Interactive verification and verification incentives | WBTS does not implement its interactive computation game |
| [MAST](https://arxiv.org/abs/2503.13657) | Adjacent multi-agent failure research; discovery lead for real failure categories | No causal accuracy, economic need or benchmark result is inherited by WBTS |
| [NEAR gas](https://docs.near.org/protocol/transactions/gas), [storage staking](https://docs.near.org/protocol/storage/storage-staking) | Historical conversions used 0.0001 NEAR/Tgas floor and 0.00001 NEAR/byte | Refresh prices; attached gas, burned gas, retained storage and recoverable credit are distinct |

The November 9 mechanism review established an implementability argument for same-evaluator escrow, not an executed rival, security equivalence or full cost dominance. It also challenged the economic need to delegate simple fee arithmetic at all. A potential advantage in integration, selective evidence publication or a genuinely useful task remains open.

## 2. Measurement assets included here

The two files are byte-preserved from previously tracked planning evidence:

- [mechanism-measurements-20261109.json](mechanism-measurements-20261109.json): exact four-fixture gas/storage observations and explicit interpretation limits.
- [mechanism-cost-probe.rs](mechanism-cost-probe.rs): reproducible integration test, copied into `swarm-tests/tests/research_cost.rs` only when absent. Uses the existing `common` module there; it is not a standalone Rust program.

[Asset manifest](ASSETS.json) records original relative paths and SHA-256 hashes. The JSON's `probe` field is the **original planning-repository path**, not a missing local dependency; the local probe is the adjacent file above. Preserve original bytes rather than silently rewriting historical results. Reproduction instructions are in START-HERE.

Measured checks were simulated transactions reading already-filed evidence, asserting verdicts and unchanged claim state, followed by challenge/finalize. The original test had four fixture cases in one test. No all-in eager escrow, production monitoring, actual dispute rate or real customer experiment was measured. Historical cost conversions and the illustrative 4/9 false-report threshold must not be mistaken for a safety proof.

Additional historical v2 checks were reported in local lifecycle evidence: 28 simulator plus six CLI passes, strict lint/format/pin checks, failed-transfer callback recovery, callback-OOG ambiguous state and exact accounting/boundary cases. Original detailed logs were deliberately excluded from Git. The summary in START-HERE avoids depending on those ignored files. Refresh behavioral claims by running the current tests; do not recreate or invent missing historical live receipts.

## 3. Candidate A: coding work and receipts

| Primary lead | What prior reviews actually supported |
| --- | --- |
| [IronClaw #2077](https://github.com/nearai/ironclaw/issues/2077) | Operator request for direct contract/market access and independently held keys. April issue, October follow-up; adjacent demand, not a receipt purchase or blame requirement. |
| [budget-skynet](https://github.com/worksOnMyFridge/budget-skynet) | First-person coding-agent operation and sandbox/test reliability work. Representative public code and earnings were not independently validated. |
| [Refund cancellation fix](https://github.com/fadeevab/market-agent-template/pull/1) | Merged November 3 PR describes wasted compute after refunded hires and cancels the matching session. Previously checked primary source; production success remains author-reported. It identifies integration pain, not an adapter-only solution. |
| [Agent Market payments/disputes](https://docs.market.near.ai/concepts/payments-and-disputes), [MCP hiring](https://docs.market.near.ai/a2a/hire-agent-over-mcp) | Existing evaluation/payment/dispute workflow is a baseline. Documentation is not source/deployment or dispute-quality verification. |
| [Suwappu Arbiter](https://github.com/0xSoftBoi/suwappubot) | At prior pin `e60e022678d568cc6b7f606213bda199946365bb`, inspected v3 callback/rubric code records verdict/reason/evidence and gates settlement. GitHub rubric checks PR metadata/merge conditions, not arbitrary code quality. No deployment/audit/adoption inferred. |

The independent A report, lead comparison and external follow-up all left the integration advantage unmeasured. They did not produce a paired coding-job experiment. The later external review therefore withdrew the default recommendation to spend two hours on A absent a specific benefit/evaluator prerequisite.

Important corrections retained: hashing a fixture proves its identity, not validity; test success is not general quality; an LLM/platform resolver is not a deterministic on-chain checker; unexecuted trial time is an estimate. Secondary done-claim statistics and unverified platform insolvency stories are not accepted decision evidence.

## 4. Candidate B: proof-on-demand inference

- [FlashProve release](https://www.anthropic.com/research/provable-inference): previous review distinguished proof-generation overhead from CPU verification and a bounded exact integer/greedy predicate. Recheck current limitations; do not transfer those benchmarks to NEAR or a different model.
- [EZKL](https://github.com/zkonduit/ezkl): prior source pin `e196b111c1bafaa61b92ae431cd3c3fe9371da05`; proof workflow lead, not a verified NEAR deployment.
- [opML](https://github.com/ora-io/opml): prior inspected challenge pin `2fa43311a4a95bcd96f5092d0a3b34e84da49eb0`; bisection/MIPS verification family with limitations in inspected code. No executed or audited integration here.
- [NEAR AI verification](https://docs.near.ai/cloud/verification/cloud-api/response-signatures): different hardware/service trust baseline, not general truth of inference.

No same-semantics cost comparison, useful workload buyer, viable funded monitor or compatible ready NEAR verifier was delivered. Rejection was a remaining-work decision. No external B report arrived in the follow-up; lack of one is not extra negative evidence about every optimistic application.

## 5. Candidate C: preventive budgets

- [Leash](https://github.com/Mattbusel/leash), prior inspected pin `7ee76d42cf07d8a2b0a1160b74aac55fff71728a`: flat policy wallet with separately restricted keys, shared caps/allowlists and reservation before asynchronous payments. No child hierarchy in inspected paths. Keys on one account are not independently authenticated account identities.
- [Espalier](https://github.com/vigohan95/espalier): prior review confirmed public claims of grant trees, narrowed authority, ancestor accounting, revocation and a hiring market. Advertised functionality is close competition; runtime/claimed testnet evidence was not rerun.
- [Rootbound](https://github.com/wahab09033/rootbound): hierarchy, reservation/recovery and ancestor revocation advertised. The earlier plural repository URL redirected here. README claims, not an independent execution pass.
- [MetaMask Delegation Framework](https://github.com/MetaMask/delegation-framework), prior inspected pin prefix `b4b8c41`: parent-chain validation, ancestor caveats and delegation-keyed cumulative native-transfer restriction. Existing hierarchy machinery, EVM-specific, not drop-in NEAR or an audit.
- [NEAR access keys](https://docs.near.org/protocol/accounts-contracts/access-keys): gas allowance versus contract-enforced token authority is a necessary distinction.

An OutLayer documentation/source discrepancy was unresolved; never cite it as a vulnerability or advantage. No paid cross-operator procurement trace needing the proposed hierarchy was established by the bounded searches. This does not prove absence of demand or invalidate product packaging.

External C report 2 referred to simulation source/results that were never delivered. Do not claim an accepted benchmark. Its simple sibling-cap example had unspecified unequal cap assumptions; its conclusion that WBTS must therefore be strongest was unsupported. A model of desired policy behavior would not independently verify actual contracts even if later supplied.

## 6. Earlier competitive sample

Historical bounded review also inspected [FlashFirm](https://github.com/0xCaptain888/flashfirm), [NEXUS AiOS](https://github.com/ykbojak-alt/nexus-aios) and [Loca](https://github.com/redaicodes/loca). Useful lessons: account for refunds/retries, make evidence and authorized actors inspectable, and give judges a short reproducible path. They were not established as identical attribution systems; their live receipts and product demand were not exhaustively validated.

No count of directory entries or accessible GitHub repositories is a calibrated competitor census, quality ranking or prize probability. Historical claims that the attribution lane was empty are superseded by the need to compare real implementations; no novelty claim survives merely by adding “on NEAR.”

## 7. Review history and independence

November 9: pitch review initially advised finishing evidence UX; the deeper escrow/verification investigation superseded that recommendation. Current code, arithmetic measurements and explicit economic limitations are the useful results to retain.

November 10: fresh A/B/C reports were drafted; interruptions and effort changes prevented the planned cyclic cross-reviews. A human-authorized single lead checked six recommendation-changing claims and synthesized the reports instead. This is reduced independent error detection, not three completed independent validations. Two external C reports and one external A report were later reviewed; provider identities/independence could not be established from the files alone.

The later review corrected our own overstrong research gate. Neither a publicly documented financial loss nor a capability impossible in existing software is mandatory. A credible need, observable operational improvement, meaningful NEAR contribution and finishable demo are the right test. None was demonstrated just by model agreement.

Private originals may exist in the separate planning checkout, but are not dependencies for this handoff: mechanism investigation/measurements, dated A/B/C comparison, external-results review and historical WBTS baseline. This register preserves the WBTS-relevant conclusions and primary leads rather than importing unrelated private records or broken sibling links.

## 8. Event verification boundary

The live [rules](https://nearhackathon.com/HACKATHON.md) and [swarm prompt](https://github.com/nearai/near-hackathon-prompt-examples/blob/main/tracks/05-swarms.md) were opened November 13 for this handoff. The deadline and scope interpretation in START-HERE reflect that check. All other external links above are historical leads from November 9–10 unless explicitly stated otherwise. Refresh the decisive ones before product claims; recheck rules before submission. No user outreach or external posting occurred.
