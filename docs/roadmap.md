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

## Next milestones, in order

1. **Gibson cloning** — overlap and primer design, fragment ordering/orientation,
   overlap uniqueness checks, junction verification and annotated assembled-product
   prediction. Keep Gibson homology rules separate from restriction cohesive-end
   rules. Preserve sequence/primer provenance and distinguish design predictions
   from experimental validation.
2. **GUI integration** — expose the validated shared application operations;
   resolve existing display/selection issues without reimplementing biology.

## Follow-up coverage

Broader enzyme catalogue, explicit terminal/overlapping strand-product models,
structured CLI argument errors, stronger metadata round-trip support and richer
assembly evaluation. Ligation product-feature reunion/fusion inference and direct
product FASTA/GenBank exports remain explicit follow-ups; source-linked component
placements are available now. Expand these when needed by a concrete workflow; do not
silently approximate unsupported biology.
