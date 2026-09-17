# CLI-first cloning roadmap

The GUI is a central product interface over the shared application layer, not the
place to develop biological algorithms. Initial Gibson support is implemented;
**Tauri desktop prototyping now proceeds alongside engine hardening**. Native Rust
runs the biology; WASM is deferred. See [desktop architecture](desktop-architecture.md).

## Completed implementation slices

- Checked sequence/coordinate types, read-only SnapGene import and fidelity warnings.
- Headless inspect/features/primers/sequence/map CLI and versioned JSON contracts.
- Six-enzyme restriction catalogue, site detection and nominal cut geometry.
- Complete digest simulation with both strand sequences and explicit fragment ends.
- Sequence-only end compatibility with reasons and proposed relative orientation.
- Source-linked annotation projections on both digest strands, clipping/split-part
  flags, JSON reports and explicitly labelled strand FASTA exports.
- Conservative GenBank selected-strand views with per-piece `misc_feature`
  annotations and provenance notes, without inferred joins or stale CDS qualifiers.
- Explicit restriction/ligation plans, orientation/end usage, linear/circular
  products, duplex phase/conservation checks and component annotation placements.
- First [Gibson PCR-tail design slice](gibson.md): explicit core intervals/order/
  orientation, fixed-length primer candidates, exact-site/overlap uniqueness,
  PCR-product junction verification and annotated-product component placements.
- [Primer optimisation](primer-optimisation.md): bounded annealing-length search,
  explicit nearest-neighbour Tm conditions and full-oligo sequence screens, checked
  against pinned Biopython and an exhaustive independent candidate search.
- [Existing-overlap assembly](existing-overlaps.md): explicit variable homology,
  linear/circular exact merges and both source associations in shared overlaps.
- Heterogeneous Gibson source intake: SnapGene, single-record FASTA, literal DNA,
  selected restriction-digest strands and ideal PCR products with explicit tails and
  retained primer candidates. Exact-overlap products export as sequence-only FASTA
  or conservative component-provenance GenBank.

## Next workstreams

1. **Gibson hardening and scope review** — full folding-energy assessment,
   broader real-template design coverage and a separate assembly-engine comparison.
   NN annealing Tm is available, but hairpin/dimer checks are sequence screens,
   not structure thermodynamics. Candidate oligos are not ordering-ready.
   Preserve the distinction between model predictions and experimentally validated
   designs; review this scope before GUI integration.
2. **Tauri desktop prototype** — native Rust services, explicit transport DTOs and
   generated TypeScript types; linked map/feature/sequence selection first, then
   validated assembly and junction views. Keep the egui viewer during evaluation.
3. **Architecture consolidation** — prepared-artifact provenance, shared diagnostics
   and composable coordinate mappings; discuss the scope in `architecture-review.md`.

## Follow-up coverage

Broader enzyme catalogue, explicit terminal/overlapping strand-product models,
structured CLI argument errors, stronger metadata round-trip support and richer
assembly evaluation. Ligation product export and feature reunion/fusion inference
remain explicit follow-ups. Gibson exact-overlap products now export sequence-only
FASTA and conservative component-provenance GenBank, but product-level biological
feature reconstruction remains deliberately absent. Expand these when needed by a
concrete workflow; do not silently approximate unsupported biology.
