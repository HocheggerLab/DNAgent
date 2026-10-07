# Refusals, provenance and validation boundaries

## Recovery without changing the question

| Signal | Meaning / next action |
|---|---|
| Nonzero status, stderr usage, no JSON | CLI parsing failure. Read subcommand help; do not read `result` or infer a biological refusal. |
| `command_failed` | Source I/O or other runtime failure. Check path, a supported sequence file (SnapGene `.dna`, GenBank or FASTA; content decides), access and file integrity. Non-sequence uploads are named in the message. Preserve earlier warnings. |
| `import_warnings` | Strict policy rejected warnings. Explain them; do not silently remove `--strict`. Review/authorise non-strict analysis separately. |
| `restriction_scan_failed` | Check alphabet, requested catalogue enzymes and source validity. Do not substitute an enzyme. |
| `digest_failed` | Inspect `sites` for unavailable/terminal/overlapping cuts. No partial digest exists. Changing enzyme sets changes the plan. |
| `compatibility_failed` | Check complete digests, supported endpoints and the 128-endpoint cap. A specified ligation plan can avoid the exploratory matrix; it cannot repair ends. |
| `annotation_failed` | Projection/export failed; retain original inputs and JSON diagnostics. Do not emit a partially trusted annotated file. |
| `ligation_failed` | Check plan schema, actual fragment IDs, input-copy rules, all selected orientations, internal junctions and closure. Do not silently linearise. |
| `gibson_failed` | Distinguish bad plan/interval, exact-site or overlap ambiguity, supplied-homology mismatch, or no feasible constrained primer pair. Keep the requested constraints; ask before changing them. |

Correct syntactic mistakes only when intent is unambiguous. A refusal caused by
biology/model scope is not permission to invent a different construct. If sources
are missing, never replace them with example fixtures and call the answer real.

## Evidence ladder

1. **Import/structure checks:** sequence, topology and feature fidelity; warnings
   retained. Current JSON schemas validate types/basic bounds, not all biological
   relationships. Historical envelope schemas are retired, not negotiable modes.
2. **Model prediction:** deterministic cleavage, compatibility, assembly or primer
   selection under stated assumptions. Pairwise ends alone do not prove assembly.
3. **Independent software comparison:** pinned Biopython sequence/parsing/Tm and
   separately implemented coordinate, primer-search and assembly checks. Agreement
   is evidence about implementation, not proof the selected physical model fits
   the experiment.
4. **Experimental validation:** not provided by this app or skill. Do not infer
   yield, sequence-confirmed clones, functional genes or primer ordering readiness.

The checkpoint has public synthetic coverage across operations, and an authorised
private circular-plasmid corpus for inspection/restriction/ligation. Do **not**
extend that claim to real-template Gibson optimisation or existing-overlap assembly.
Native SnapGene product comparison, broader linear/format variants, separate
assembly-engine comparison and full folding thermodynamics remain gaps.

## Keep reproducible evidence without unnecessary work

Normal use does not require rebuilding or running the entire test suite per query.
Retain executable identity, schema, source hashes, plans, command/status/stdout/stderr
and selected result fields. Cache only against unchanged sources, settings and
binary identity. Do not cache away warnings or overwrite past runs.

For app development/contract changes, use the checkout's `AGENTS.md`. Its scripts
include `check_cli_schema.py`, `check_restriction.py`, `check_digest.py`,
`check_compatibility.py`, `check_fragment_annotations.py`, `check_ligation.py`,
`check_gibson.py` and `check_gibson_extensions.py`. Run the optional private-corpus
checker only with existing authorisation; inputs/manifests/reports stay outside Git.
Never upload constructs to a web service for convenience.

The skill's `scripts/check_skill.py` checks frontmatter/link/command coverage and
executes documented command families on **public synthetic fixtures**, including
export variants and strict/error handling. It is a smoke test of documentation and
CLI availability, not independent biology validation or an agent evaluation.

## Next evaluation: held-out agent tasks (not performed by the smoke check)

Evaluate whether an agent using this skill can choose the correct tool and explain
its result, rather than merely reproduce a successful sample plan. Include:

- Warning-bearing inspection; unknown/multipart/reverse/origin-spanning features.
- Half-open extraction versus requested circular arcs; historical primer metadata.
- A supported site scan and an unsupported enzyme; an unavailable/terminal cut.
- Sticky-end compatibility with reversed orientation and correct physical oligos.
- Annotation clipping and explicit top/bottom GenBank export without CDS invention.
- A ligation with unused fragments, circular phase and a required second copy.
- Fixed PCR-tail mode versus supplied-overlap mode; prevent endogenous deduplication.
- Optimisation requiring explicit buffer units; infeasible constraints without
  silent relaxation; exact specificity versus experimentally reliable specificity.
- Existing-overlap closure with both source associations preserved.
- Runtime versus CLI syntax errors; warning retention on a later missing source.
- Unsupported gRNA design, faithful round-trip editing or product gene repair:
  the correct answer is an explicit boundary and appropriate next step.

Use held-out synthetic tasks first, predefine expected choices/refusals, compare
products/coordinates with independent references, and check source hashes before
and after. Score plan/command correctness, provenance, warning handling, refusal
quality, output interpretation and unnecessary tool calls. Add authorised real
constructs separately; keep model and experimental evaluation distinct.
