# Fixed grounding acceptance review v1

The native citation contract passes these five fixed cases: seven cited spans
resolve to their exact recorded source revisions, and the concurrent revision
change is marked stale. This establishes quotation integrity. It does not
establish that the answer's claims follow from those quotations.

The [actual input/output receipt](grounding-acceptance-v1.json) has SHA-256
`af54f708e65bc59c3acf56348e4fecf9909f354185f04888442cb99728d2b6ba`.
It records original source bytes, the question, actual outgoing provider request,
all native stream events, final response, resolved evidence checks, and the
separate manual source comparison below. Generation outputs were fixed scripted
loopback HTTP responses. Retrieval, HTTP SSE parsing, streaming, revision storage,
and evidence resolution used the real Rust implementation. No paid provider
request or real-provider quality evaluation took place.

The migration audit agent compared each claim with the original sources directly;
the judgment does not come from a model judge, the presence of `[E1]`, or the
runtime's list of `used_evidence`. This is a small adversarial contract audit,
not a sampled accuracy estimate or an independent human annotation study.

| Case | Quotation integrity | Source-based inference review |
| --- | --- | --- |
| Supported | `[E1]` resolves to a record saying the gate opens at 10:00 | Supported. The answer explicitly attributes the same time to that record. |
| Unsupported | `[E1]` resolves to a record about apple trees flowering in spring | Unsupported. The answer claims that the orchard is 500 years old; no source supplies an age. |
| Conflict acknowledged | Both quoted notices resolve, one giving 10:00 and one 12:00 | Supported as a statement of disagreement. Neither notice establishes which time is currently authoritative. |
| Conflict erased | Both notices resolve despite giving different times | Contradicted. The answer claims that both notices agree on 10:00. |
| Stale current claim | Revision one's 10:00 quotation still resolves; current revision two says 12:00; evidence is marked `stale=true` | Historically supported only. The answer's claim about the current time conflicts with the new revision. |

All five streams emitted progress and evidence before the provider delta, then
usage and `done`, with contiguous sequence numbers. Every final response declares
`citation_semantics="model_markers_only_not_entailment"`; unknown citation lists
are empty. The two conflicting-source cases each retain both actual source
quotations. Source text containing `原典𠮷` survives in the original evidence spans.

Three limits matter to consumers:

1. `done` means generation completed. Unsupported and contradicted scripted
   answers also receive `done`; the runtime has no automated entailment verifier.
2. `used_evidence` identifies source labels actually written in the answer. It
   does not certify support for individual statements, resolve conflicts, or prove
   that uncited statements are grounded.
3. The final stale flag preserves an honest historical citation after a concurrent
   source edit. The runtime does not regenerate the answer or rewrite a false
   present-tense claim automatically. A consumer must keep that status visible
   and must not describe such an answer as verified against current sources.

These are explicit limits of the present contract, not evidence that a real
provider passed or failed a quality benchmark. A future provider evaluation must
retain actual provider inputs/outputs and receive separate source-based review;
these scripted responses cannot be substituted for it.

Validation used `cargo test -p llmbase-core --test ask --test entities_model` in
the isolated migration worktree. All seven ask tests passed, including
`capture_fixed_grounding_acceptance_cases`; the capture path was supplied with
`LLMBASE_GROUNDING_AUDIT_PATH`. The corresponding log is
`.rust-migration-runs/grounding-audit.log`. The unrelated new entities assertion
initially expected a narrower error variant than the public source guard returns;
that assertion was corrected and its targeted rerun passed without a production
change (`.rust-migration-runs/entities-revocation-v2.log`).
