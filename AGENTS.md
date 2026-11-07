# AGENTS.md — Who Broke the Swarm (reframe, 2026-11-07)

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
## Verified nuances (from independent verification, 2026-11-07)
- The DeepMind paper describes TWO mechanisms: (a) optimistic challenge model (which we
  implement) and (b) game-theoretic verification game (Schelling point/TrueBit). We
  implement (a) only. README must say "optimistic challenge model," not "verification game."
- The paper says "post-hoc error discovery—even outside the dispute window—triggers a
  retroactive update to reputation." We do NOT implement post-hoc updates outside the
  dispute window (deliberate simplification, design principle 4). README must acknowledge this.
- Directory has 8 projects submitted (not 217). Novelty still holds (0 blame/attribution
  entries). Plan's track competition numbers are unreliable; re-pull before submission.
- Prior art details for Writ, FlashFirm, CartelBreaker are UNVERIFIED. Spot-check repos
  before citing specific numbers (3 levels/16 agents, 2-of-3 evaluators).

## Pick
