# AGENTS.md — Who Broke the Swarm (updated 2026-11-08)

## Goal and authority
- Deliver a working, honestly documented staked-attribution demo with correct money
  flows and inspectable evidence. Complete approved work through implementation,
  necessary supporting changes, tests and documentation.
- The builder's latest explicit instructions and confirmed decisions supersede
  this file and historical briefs. Do not ask again about settled decisions.
- Distinguish design choices from factual claims: the builder decides scope and
  policy; primary evidence establishes what is true and what the code does.
- Ask interactive questions for material new decisions or conflicting requirements.
  Resolve routine implementation choices independently and continue unrelated work
  while awaiting an answer. Do not invent approval gates.

## CRITICAL THINKING DIRECTIVE (read this first)
The handover (codex-handover-reframe-20261107.md) is a PROPOSAL, not gospel. It was
written by an agent ("muse") and has NOT been independently verified. You are a
critical thinker, not an executor. When you encounter a claim, number, or design
decision in the handover:
- If it involves money flow correctness → verify the math yourself before coding it.
- If it cites a paper → verify the quote/number against the actual paper.
- If it says "first" or "novel" → search for counterexamples before asserting.
- If a number seems off → flag it in codex-log/, do not silently use it.
- If you're unsure whether something is true → write "UNVERIFIED" in the code/README
  and note it in STATUS.md. Never assert an unverified claim as fact.
- The builder (human) wants rigor over speed. A flagged uncertainty is better than
  a false claim that a judge catches.
- Source of truth order: (1) primary sources (papers, NEAR docs, explorer), (2)
  morning-update-synthesis, (3) reframe-sharpening-addendum, (4) handover. The
  handover is LAST. If it conflicts with a primary source, the primary source wins.

## Verified nuances (from independent verification, 2026-11-07)
- The DeepMind paper describes TWO mechanisms: (a) optimistic challenge model (which we
  implement) and (b) game-theoretic verification game (Schelling point/TrueBit). We
  implement (a) only. README must say "optimistic challenge model," not "verification game."
- The paper says "post-hoc error discovery—even outside the dispute window—triggers a
  retroactive update to reputation." We do NOT implement post-hoc updates outside the
  dispute window (deliberate simplification, design principle 4). README must acknowledge this.
- Historical directory count reported on 2026-11-07: 257 projects, with no
  blame/attribution entries identified in that review. These are not current facts
  or proof of novelty. Re-check the directory and prior art before external claims.
- Prior art details for Writ, FlashFirm, CartelBreaker are UNVERIFIED. Spot-check repos
  before citing specific numbers (3 levels/16 agents, 2-of-3 evaluators).

## Pick
Who Broke the Swarm — staked attribution on tamper-evident run logs. Tracks:
swarms (primary) + business + security. Solo builder + Codex (ChatGPT Plus).

## Frozen scope (do not widen)
- Conceptual mechanism: arbitrary agent count/delegation depth, deterministic-verifier
  tasks and checkable failure classes F1–F5a. This is not an implementation claim:
  the current adjudicator supports direct T2 F3/F1 only; nested traversal is absent.
- The 3-agent planner/worker/reviewer pipeline is the WORKED EXAMPLE, not the limit.
- No reserve, no marketplace, no token, no reputation layer beyond the §1 score,
  no live users, no depth >2, no Intents integration, no cross-chain.
- Anything outside scope is a README roadmap line, not a build item.

## Approved defaults and settlement decisions
- Latest builder-confirmed decisions take precedence over Appendix A or old briefs.
  Parameterized amounts are snapshots, not hardcoded settlement constants.
- S = 1.0 NEAR stake/agent/run; B = C = 0.25 NEAR; panel bond = 0.1 NEAR.
- Blamed stake slash = min(claim.bond, remaining run stake); zero stake still
  permits finalization and the applicable reputation penalty. Upheld also slashes
  the full claim.challenge_bond. Split slashed funds 80% winner / remainder treasury,
  using exact integer arithmetic. Clear both bond fields on finalization.
- Conservation including returned principal: Pending: bond + blamed_slash =
  payout + treasury; Upheld: bond + blamed_slash + challenge_bond = payout + treasury;
  Overturned: bond + challenge_bond = payout + treasury, with stake unchanged.
- Challenge window 600 blocks (~6 min); demo 60 blocks (~36 s); panel window 86,400 blocks (~14.4 h); F2 timeout D = 60 blocks (demo).
- Historical model estimates: p* = 44.3% (WITH gas convention), r* = 55.7%,
  t* = 55.7% (p=1). Verify assumptions/calculations before citing; these do not
  establish all-in costs or thresholds for adjusted parameters.
- Reputation at finalize: base 100, cap 120, floor 0; accepted blame -10;
  overturned claim reporter -15;
  successful challenge +5 (capped +5/run); exoneration +2.
- Overturned reputation order: reporter penalty, challenger bonus, blamed
  exoneration; each reads the latest score and saturates/caps. Roles may overlap:
  self-challenging blamed agent gains +7 from base 100. Corrected attribution
  causes no additional penalty; an independently applicable reporter penalty still
  applies. A new blame penalty requires a fresh bonded claim and defense window.
- Prepay missing reporter reputation during filing and challenger reputation plus
  per-(run, challenger) bonus marker during challenge. Preserve existing entries;
  flush before measuring storage. Finalize has zero storage growth. First overturned
  win consumes the +5/run entitlement even if score cap clips the increase.
- Historical 100-event cost estimate: 0.13–0.22 NEAR (~0.15 pitch), primarily
  storage stake. Re-measure before presenting as current; refunds/release and
  adjudication gas must be stated accurately, not assumed implemented or included.

## Toolchain
- Keep repository dependency pins/lockfiles (near-sdk 5.29.1); do not upgrade
  dependencies merely to match a newer installed tool.
- Historical requested tools: Rust 1.97.1, cargo-near 0.21.1 (fallback 0.22.0).
  Available tools may be used when compatible; record actual versions and failures.
- Node 22.12+; Docker reproducibility remains a separate check, not an assumed result.
- wasm32-unknown-unknown target required.
- ACTUAL INSTALLED: Rust 1.99.1, cargo-near 0.23.0, near-cli-rs 0.30.1. Record actual versions in run log.

## Never-do (grep the repo before submitting)
- Never assert priority/novelty without current supporting research. Qualifying a
  claim as "on-NEAR" does not establish it. Describe the implemented mechanism as
  the optimistic challenge model, not the separate verification game.
- Never "formally verified" / "verified system" / "model-checked" / "TEE-verified" /
  "attestation verified" / "cryptographically attested".
- Never "reserve" / "solvency reserve" / BlameDAG payout numbers (140/55/70).
- Never port BlameDAG.tla — no TLA+ model in this build.
- Never reintroduce the reserve or BlameDAG payout math.
- Publishing, external posts, submission and any deployment (testnet or mainnet)
  require explicit builder authorization for that action. Routine local commits
  and pushes to the project repository are authorized; inspect branch/remote first,
  preserve others' work, and never force-push or rewrite shared history by default.
- Destructive or irreversible actions outside the approved task require confirmation.
- Never touch the `website` submission field (spam trap — leave EMPTY).
- Never work around a Codex cap with a second account.
- Never fake a testnet failure — injection only via a labelled test-only path or mock token.
- Never mock data in the UI.
- No token, marketplace, or reputation system beyond the §1 score.
- No depth >2, multiple primes, or Intents integration.

## Working rules
- Work whenever requested. No morning-only restriction or mandatory `/status`
  check. Report actual usage/tool limits if encountered; unavailable meters do not
  block useful work. Never evade provider limits.
- Keep changes focused and reviewable, but complete the approved work item without
  requiring another prompt per method, diff, compile fix or supporting change.
- Investigate and fix test/build failures within scope; rerun relevant checks.
  Stop dependent work for unresolved money-safety failures, missing necessary
  authorization, or a material design conflict. Report evidence and ask for the
  specific decision needed. Never bypass a failing check or label it passed.
- For factual claims use primary sources first, then synthesis, addendum, handover.
  For product decisions use the builder's latest confirmed instructions.
- Every claim maps to an artifact. A claim without an artifact is a liability.

## Role split
- Builder: material product/economic decisions, publication/deployment approval,
  video/submission and evidence that specifically requires human execution.
- Codex: implementation, supporting changes, research, critical review, verification,
  CLI/tests/toolchain debugging, honest docs and routine commits/pushes. Do not defer
  needed work merely because an old plan assigned it to another agent.
- Historical agent reports are inputs to verify, not independent proof by default.

## Contract methods (pin before code)
register / anchor / file_claim / challenge / adjudicate / finalize +
resolve_panel (stub, documented no-op) / resolve_panel_with_vrf (design-only, no code) +
views: verify_run, get_reputation, get_claim, get_run.

## Shared collaboration
- Read ../collab/STATUS.md first when it exists. In managed worktrees, resolve the
  shared directory from the original project checkout; current path is
  /home/alisa/Projects/ongoing/near/collab/STATUS.md. If unavailable, report that and
  use local evidence; do not make a missing relative path an approval gate.
- Keep STATUS.md under 40 lines: current step, open items, links. Log one short
  paragraph per work item in codex-log/; write detailed reports once and link them.
- Search sections first and read relevant ranges. Avoid full-file reads, full diffs
  or build transcripts in chat; report results and paths with failing excerpts only.
- Do not copy-paste prompts between agents; use the shared files.
- Codex: be a critical thinker, not an executor. Flag uncertainties as "UNVERIFIED."
