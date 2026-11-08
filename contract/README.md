# Contract status through 2.9

The contract supports registered run logs, owner-adjustable amounts, bonded claims/challenges, and **persisted causal adjudication of direct T2 fee-split tasks**. `preview_adjudication` computes a verdict without changing state. Permissionless `adjudicate` persists that exact result once, without transfers or reputation effects. Challenged-claim payouts remain 2.10; only the earlier unchallenged `finalize` path is implemented.

## Verification architecture and production direction

The demo uses fixed Rust verifier code inside the main contract, with JSON task specifications as data. Local CLI checks can help prepare inputs but are not the authority for challenged-claim verdicts. This clarifies the earlier local-only verifier decision with builder approval.

**Our intended production architecture is a separate NEAR verifier contract**, for independent review and versioning. It is planned, not implemented or demonstrated here. Production still needs enforceable code identity/version binding, an upgrade policy preserving open claims, and authenticated callbacks with explicit failure handling. Separation alone does not establish those properties. See the [NEAR cross-contract documentation](https://docs.near.org/smart-contracts/anatomy/crosscontract) and [private callback guidance](https://docs.near.org/smart-contracts/security/cross-contract-calls/callbacks).

`get_verifier_info()` publishes the supported task type, bounds and `verifier_hash`. For this embedded implementation, the hash is SHA-256 of the domain `wbts/embedded-t2/source-manifest/v1` followed by a zero byte and four length-prefixed byte strings: `src/lib.rs`, `src/t2.rs`, `Cargo.toml`, `Cargo.lock`. Lengths are u64 little-endian. It fingerprints the source/dependency manifest; **it is not the deployed WASM hash or an execution attestation**. Reviewed build output and the full deployed-code hash must be checked separately. Main-contract deployment/upgrade control remains part of the demo's trust assumptions.

A copied metadata hash cannot select arbitrary code: dispatch only executes the compiled-in implementation if the run and claim pins match its published identity. Registration rejects unknown identities before retaining stake or storage deposits; metadata-only registration is no longer supported (issue #4, builder-approved). Any source/manifest change changes this identity, including unrelated source changes. Carrying old versions through upgrades is not implemented; do not upgrade a funded instance in place.

## Task admission rule

T2 here means exactly this task: **a nonempty ledger of at most 128 unsigned integer amounts, whose declared total equals their checked u128 sum, is settled under a pinned fee rate; the output must equal the canonical fee/net bytes.** Within this admitted task, another output does not pass. The verifier redoes the arithmetic; the worker is computationally redundant. This does not establish whether the ledger describes real transactions.

Pinned spec bytes, shown as JSON before base64 encoding:

```json
{"task_type":"t2_fee_split_v1","fee_basis_points":250}
```

The fee rate is an integer from 0 through 10000. Duplicate/unknown spec fields or unsupported task types are rejected before registration funds are locked in the embedded mode. Spec hashing covers exact bytes, including whitespace.

Assignment payload:

```json
{"entries":["10000","25000","15000"],"declared_total":"50000"}
```

Amounts are canonical decimal strings: no sign, leading zeroes except `"0"`, decimals or exponent notation. Input sum overflow, duplicate/unknown fields, an empty/oversized ledger, or inconsistent total is an F3 input-contract failure. The delegator is the on-chain actor that anchored the assignment, not necessarily the run's creator/funder.

Compute `total = sum(entries)`, `fee = floor(total * fee_basis_points / 10000)`, `net = total - fee`. The implementation uses quotient/remainder arithmetic to avoid overflowing the multiplication for a valid u128 total. Exact output bytes for the example are:

```text
{"fee":"1250","net":"48750"}
```

Output order is fee then net, values are decimal strings, and no whitespace/newline or extra fields are accepted. A mathematically equal but differently serialized output is an F1 violation of this byte-exact task contract. This rule is intentionally explicit rather than hidden JSON normalization.

## Binding the evidence to the right task

`register(run_id, agents, verifier, task_spec?)` keeps creator-funded collateral. For the published embedded verifier identity, exact base64-encoded `task_spec` bytes are required: their hash must equal `verifier.task_spec_hash` and their schema must be supported. The verifier object remains `{task_spec_hash, verifier_hash, verifier_key}`. Its nullable key is unused metadata here; no signed off-chain-verdict path is implemented.

`anchor` adds optional `task` metadata to the earlier caller-bound ABI. Embedded T2 assignments require:

```json
{"assignment_seq":0,"worker":"worker.testnet","parent_assignment_seq":null}
```

For `task_assigned`, assignment_seq must equal the event's sequence and worker must be a registered agent. For `settlement`, the link must match that earlier assignment and only its assigned worker may anchor the completion. Exactly one completion is accepted; a corrected result needs a new task assignment. The task link is part of the Borsh event hash. Other event kinds may appear as unrelated notes without task metadata.

This first causal slice admits direct independent delegations. Explicit parent links are rejected; nested-delegation attribution remains parked until ancestor evidence semantics are implemented. Multiple independent assignments and any registered agent count are supported within the event/gas bounds. No inference is made about undeclared off-chain dependencies.

`file_claim` still validates referenced sequence/hash pairs. In embedded T2 mode it only admits F1/F3 allegations; the **first cited event must identify the assignment**. Other cited task events must belong to that same assignment. The derived assignment sequence is stored with the claim. An adjudication caller cannot choose a passing task in place of the reporter's claimed task. Historical transaction references remain format-checked references, not independent on-chain proofs of transaction linkage.

## Snapshot and in-call evidence

The first successful, fully funded `challenge(claim_id)` freezes the run's event count, head hash and challenge height. Defense anchored before that challenge is eligible, even if anchored after filing. Later appends are excluded. The worker should anchor its defense before challenging. Snapshot creation, result-slot preallocation and the matching challenge bond roll back together if storage funding is insufficient. The challenger pays measured storage separately from the bond, including a bounded result slot sized from the Borsh result/attribution schema with a maximum 64-byte account ID.

`preview_adjudication(claim_id, task_spec, payloads)` requires a Challenged claim. Spec and payload bodies are exact base64-encoded bytes:

```json
{
  "claim_id":0,
  "task_spec":"<base64 of exact pinned JSON>",
  "payloads":[
    {"seq":0,"bytes":"<base64 of assignment bytes>"},
    {"seq":1,"bytes":"<base64 of completion bytes>"}
  ]
}
```

The verifier checks the frozen stored-event chain, implementation/spec identities and each payload hash before interpreting task contents. It derives the required assignment and unique completion from authenticated task links within the snapshot, rather than trusting a reporter-selected suffix. Only the relevant task's payloads are needed; unrelated sibling failures cannot steer the verdict. Their event metadata still participates in chain verification. Payload argument order does not affect the result.

If a completion exists in the snapshot, both payloads are required, including when the input later proves invalid. If no completion exists, invalid authenticated input may still establish F3. Valid input without a completion produces no verdict; timeout logic is not implemented here. Missing, forged, mismatching, oversized, cross-task or unsupported evidence rejects without changing the claim, reputation or money. A caller's bad bytes are not an F4 conviction of the original agent. Withholding responsibility and recovery remain for 2.18.

## Persisting the decision

`adjudicate(claim_id, task_spec, payloads) -> Verification` takes the same exact-base64-byte ABI as preview. The first call requires Challenged status and invokes the existing preview computation directly in Rust. It stores the full result as `adjudication_result`, sets status to AdjudicatedUpheld or AdjudicatedOverturned, and sets `corrected_attribution` to the finding agent (or null for passing S2). The field also names the original blamed agent when its allegation is upheld; it is not necessarily a different agent.

Anyone may call after challenge, including after the challenge deadline; there is no separate adjudication deadline. Subsequent decodable calls return the stored result without rechecking evidence or writing state. SDK JSON/type and nonpayable-deposit checks still apply. Preview itself continues to require Challenged status; read the persisted result through `get_claim` afterward.

Adjudication consumes the challenger-funded padding, preserving the serialized claim length and asserting unchanged flushed storage usage. It accepts no deposit, sends no transfers, and changes no stakes, bonds, treasury or reputation; its caller pays gas. Unused padding remains paid physical storage, with no deletion/refund policy added here. Both bonds remain locked, and the existing finalize guard still rejects adjudicated claims until 2.10. A missing or unauthenticatable evidence submission stays Challenged and may be retried.

## Causal result and scope boundary

F3 input validation precedes F1 output checking. In S6, invalid input produces a finding against its delegator; an F1 claim against the worker is Overturned, even if the output also fails. For valid input and bad output, the finding is F1 against the assigned worker. Passing input/output produces no finding. Upheld requires **both** the finding's class and agent to match the claim.

`finding.origin` describes the decisive fault and is Introduction in this slice. Top-level `origin` describes the reported check relative to that fault: S6's reported F1 is Propagation, while the actual F3 finding is Introduction. All-passing evidence has no finding/origin. Corrected attribution is a finding; it never directly slashes the newly named agent. U8: a newly named agent receives no reputation effect here; a later penalty needs its own bonded claim, defense window and applicable finalization.

## Evidence and gas bounds

| Bound | Current value |
| --- | --- |
| Embedded T2 events per run | 100 |
| Ledger entries | 1–128 |
| Task spec bytes | 1024 |
| Each relevant payload | 8192 bytes |
| Relevant payloads per call | At most 2 |
| Aggregate decoded evidence guard | 32768 bytes |
| Serialized JSON call-input guard | 49152 bytes |

The effective supported spec + two payload bodies total at most 17408 bytes. The wider aggregate/wire guards also reject malformed calls. SDK deserialization precedes method entry; these guards do not claim to prevent all parsing work on a malicious transaction. Callers pay their transaction gas.

The measured 100-event case uses 128 ledger entries summing to u128::MAX, a 1024-byte spec and two 8192-byte bodies. See `../codex-log/2.9-sim-gas.json` for stateful adjudication simulator gas/storage and `../codex-log/2.9-preview-stress.json` for preview; these are not measured testnet prices or a proof of all possible gas paths. Single-call gas bounds the evidence that can be processed. **Chunked upload is future work.** The state-changing `adjudicate(claim_id, task_spec, payloads)` caller pays that call's gas; adjudication gas is not priced into the existing economics. Do not describe those figures as all-in costs.

Hashes may commit to off-chain bytes that later exceed these bounds or become unavailable. The hash-only anchoring API cannot prove preimage size/availability. Such a submission gets no verdict here; there is no automatic withholding penalty or resolution guarantee. Supporting production data availability and recovery is outstanding work.

## Remaining state and work

`new(demo?)` assigns the initializing predecessor as owner; default S=1 NEAR, B=C=0.25 NEAR; default window 600 blocks, explicit demo=true selects 60. `update_parameters` is owner-only; existing stakes and filed bonds do not change. New claims use current B; challenges match the filed bond. S must cover the fixed 0.25 slash and B=C>0. These adjustable amounts do not justify extending default economic thresholds to arbitrary settings.

Registration, filing and challenge fund their measured storage separately from principal and refund excess. Remaining stakes stay locked. Failed reporter-payout recovery is unimplemented. Preview leaves claims Challenged; adjudicate persists their decision, while challenged payouts remain unimplemented until 2.10.

Eligibility currently permits self-claims and challenges by any account, including the reporter and blamed agent. Blamed-agent self-challenge is an intended defense. Reporter self-challenge is also admitted; there is no timeout or recovery for unresolved challenged claims in this slice. This remains a lifecycle limitation to review when implementing resolution, rather than an established safe self-dealing policy. `get_reputation` returns the baseline 100 for an unknown account; that value does not prove registration or participation.

Parked: T1/T3 verifiers, F2 with RECV/timeout evidence, F4 attribution, F5a composition, nested-delegation verification, production separate-verifier dispatch, and chunked evidence upload. These are not silently counted as supported. Current state/event/claim layouts require a fresh deployment; there is no migration. The funded Gate-1 deployment and its evidence remain untouched.
