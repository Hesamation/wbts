# Third-party code and data

The data escrow's sparse Merkle proof format and Rust verification algorithm are
adapted from [InferPay](https://github.com/taejeekim/InferPay), by Taejee Kim,
at commit `c7f773f144ff01092348185ed657dfe729fcec44` (MIT).

- `data-escrow/compat/merkle.ts` preserves the upstream JavaScript algorithm, with
  its compile-time branded string type inlined for standalone execution.
- `data-escrow/compat/coverage.json` contains upstream public constructed policy
  samples. They are test inputs, not actual insurance terms or private datasets.
- `data-escrow/src/proof.rs` is a bounded Rust port for string-valued `fact` entries
  and vacancy proofs. It does not implement upstream figures, regex rules, answer
  quality evaluation or the rest of InferPay.
- The full upstream [MIT notice](data-escrow/compat/INFERPAY-LICENSE) is included.

The agent workflow and escrow are a separate implementation. There is no claim of
an upstream partnership, endorsement, production integration or shared submission.
