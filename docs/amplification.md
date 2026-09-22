# Offline amplification / primer design, version 1

This is the first implementation slice motivated by the analytical-primer/qPCR run
reported on 2026-09-22. It is **not a complete qPCR assay designer**, Primer3 replacement,
or validation of the private primers from that run. It leaves the GUI map work intact.

## Run

```bash
cargo build -p dnagent-cli --locked
target/debug/dnagent primer-design fixtures/plans/synthetic-primer-design.json
```

JSON-only command, using the existing 0.9.0 envelope and a named object result.
Existing `features` array and circular-arc contracts are unchanged. `--strict` rejects
import warnings; successful/failing JSON preserves warnings gathered before failure.
No files are written and no remote services or external executables are called.

## Plan

See `schemas/primer-design-plan-1.schema.json` and the committed synthetic example.

- `schema_version`: 1.
- `inputs`: 1–8 local SnapGene/single-record FASTA paths, relative to the plan file.
  FASTA is linear; circular templates need an explicitly circular supported source.
- `feature_id`: optional exact source feature ID. Currently requires a single-part
  feature whose start/length exactly equal the declared window. No label ambiguity,
  implicit feature expansion, strand reversal or multipart flattening. Larger features
  require an explicit subwindow without `feature_id`.
- `request.reference_input`: **one-based** input index.
- `window_start`, `window_length`: zero-based start and length in the forward reference,
  length 36–500. Circular windows may wrap; linear windows must fit. Primer pairs are
  selected **within** this window, not necessarily flanking or covering the whole target.
- `junction_offset`: optional boundary in the unrolled window, not a whole-record index.
  At least one primer must span it with `junction_min_bases` on each side. This is a
  primer-over-junction constraint, not just a product-containing-junction constraint.
- `positive_inputs` / `negative_inputs`: partition every input exactly once; reference
  must be positive. One pair must produce exactly one exact heteroprimer product of the
  requested size on each positive, with no additional screened products; none on negatives.
  Products need not be byte-identical across positives. Each template interval is reported.
- `min_product_length`, `max_product_length`: allowed positive product lengths. Primer
  binding sites must be nonoverlapping; minimum product size is 36.
- `screen_max_product_length`: screen for alternative products up to this length, at
  least the design maximum, at most 5000. Products longer than the template are excluded.
- `max_mismatches`: 0–2 full-oligo Hamming mismatches; `exact_three_prime_bases`: 4 up to
  minimum primer length. Anchor mismatches are excluded, not merely penalised.
- `max_results`: 1–20 ranked pairs.
- `constraints`: existing explicit primer length/Tm/GC/structure/solution constraints.
  See `primer-optimisation.md` for numerical provenance and concentration units.

All templates must be unambiguous ACGT and at most 100,000 bases. No guessing for N or
other ambiguous symbols. Cross-field validation in Rust supplements JSON Schema.

## Computation and screening

1. Enumerate forward and reverse oligos of each allowed length at every legal window
   position. Assess Tm/GC, contiguous hairpin stems and self-dimer runs.
2. For each surviving oligo, search every supplied template in both orientations,
   including origin-spanning binding. Require an exact occurrence on every positive.
3. Enumerate pairs, requiring product size, pair Tm difference, junction (if supplied),
   and heterodimer constraints.
4. Enumerate inward-facing, nonoverlapping binding-site combinations on every template.
   Include products involving two copies of the same oligo and reversed F/R placement.
   Reject additional screened products on positives and any screened product on negatives.
5. Rank feasible pairs by summed absolute deviations from target Tm, then total primer
   length, forward reference start, reverse reference start and forward primer length.
   Return top N, plus counts of examined and feasible pairs. No stochastic search.

The search has explicit hard budgets: 200 million base comparisons, 200 sites per
oligo per template, one million candidate-pair combinations and 20 million product-site
combinations. Exceeding any budget returns an error, never a silently truncated search
or partially screened candidates. Narrow the window or length range instead.

### Thermodynamic model

`dnagent-nn-v1` reuses the existing Rust Allawi/SantaLucia 1997 nearest-neighbour model,
SantaLucia 1998 entropy salt correction and von Ahsen 2001 sodium-equivalent Mg/dNTP
approximation, with primer in excess. Results preserve all solution concentrations.
Self-complementary oligos unsupported by this model are excluded. This is **not
Primer3**, and structure screens are contiguous Watson–Crick runs, not folding free
energies, bulges or temperature-dependent secondary-structure predictions. No PCR
annealing temperature is inferred from the reported Tm.

### Evidence and identity

Application intake imports the same byte snapshot it hashes. Each source record has
file SHA-256, exact uppercase forward-sequence SHA-256, name, path, length, topology,
input index and source-associated warnings. The result also has a SHA-256 of the exact
plan bytes, crate version, model identifier and effective request. Sequence hashes are
not rotation-invariant; origin and orientation remain explicit.

Each primer has its 5′→3′ sequence and forward-reference start/length/orientation.
Every selected pair carries its binding sites (primer 1 = forward candidate, 2 = reverse
candidate) and product template intervals. `template_sequence_forward` is the source
interval, not an inferred mismatch-corrected molecular product. Accepted positive
products have exact matches; their forward-reference sequences may differ in orientation
between sources. Features/translations are not inferred for these intervals.

## Architecture

- `dnagent-domain::amplification`: typed validation, design, site and product logic.
- `dnagent-app::amplification`: versioned plan intake, immutable byte snapshots, hashes,
  feature checks, source-associated warnings and shared service.
- CLI: thin JSON adapter. Future GUI can call the same service; it must not reimplement
  design or specificity calculations in TypeScript.

A small `import_path_bytes` application entry point allows hashed callers to import
exactly the bytes they read. Existing `open_path` behaviour and callers are retained.
Existing Gibson structure screens are reused internally; no duplication of their model.

## Explicitly deferred

- Automatic shared/unique block discovery across whole constructs: candidates here
  originate from one **explicit reference window** and are screened against all sources.
- Primer3 integration / energetic secondary-structure evaluation.
- Seed-only searches, indels, mismatches within the declared exact 3′ anchor, and
  genome/transcriptome specificity. No products is a statement about this bounded model,
  not proof of biological specificity.
- Local BLAST: optional future adapter requiring declared database identity/version and
  parameters. Remote searches must remain opt-in; private sequence must never be sent
  automatically.
- GUI primer placement, amplicon inspection and agent-to-live-GUI handoff.
- Assay-type inference, DNA-versus-RNA specificity, experimental validity or oligo ordering.

## Validation

```bash
cargo test -p dnagent-domain amplification
uv run scripts/check_amplification.py --binary target/debug/dnagent
```

Rust tests exercise rotation/reverse positives, negative identity rejection, junction
coverage, orientation-aware mismatch anchoring, circular sites/products, self-primer
products and invalid input/budgets. Independent Python checks use public synthetic
SnapGene fixtures, enumerate sites/products separately and compare NN Tms to pinned
Biopython 1.85. They validate file/sequence/plan hashes, deterministic output, circular
windows, schemas, refusal cases, strict mode and warning retention after later failures.
No private construct or previously reported real primer pair is claimed revalidated.
