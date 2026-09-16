# CLI-first cloning roadmap

The GUI is an adapter to the tested application layer, not the place to develop
biological algorithms. **Gibson cloning is required before GUI integration.**

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

## Next milestones, in order

1. **Gibson hardening and scope review** — full folding-energy assessment,
   real-template design coverage and a separate assembly-engine comparison.
   NN annealing Tm is available, but hairpin/dimer checks are sequence screens,
   not structure thermodynamics. Candidate oligos are not ordering-ready.
   Preserve the distinction between model predictions and experimentally validated
   designs; review this scope before GUI integration.
2. **GUI integration** — expose the validated shared application operations;
   resolve existing display/selection issues without reimplementing biology.

## Follow-up coverage

Broader enzyme catalogue, explicit terminal/overlapping strand-product models,
structured CLI argument errors, stronger metadata round-trip support and richer
assembly evaluation. Ligation product-feature reunion/fusion inference and direct
product FASTA/GenBank exports remain explicit follow-ups; source-linked component
placements are available now. Expand these when needed by a concrete workflow; do not
silently approximate unsupported biology.
