# CLI-first cloning roadmap

The GUI is an adapter to the tested application layer, not the place to develop
biological algorithms. **Gibson cloning is required before GUI integration.**

## Completed implementation slices

- Checked sequence/coordinate types, read-only SnapGene import and fidelity warnings.
- Headless inspect/features/primers/sequence/map CLI and versioned JSON contracts.
- Six-enzyme restriction catalogue, site detection and nominal cut geometry.
- Complete digest simulation with both strand sequences and explicit fragment ends.
- Sequence-only end compatibility with reasons and proposed relative orientation.

## Next milestones, in order

1. **Annotation-aware fragments and export** — preserve or split features at cuts,
   flag disrupted annotations, retain source provenance; FASTA/JSON followed by
   annotated GenBank export. Do not invent mappings for unsupported metadata.
2. **Restriction/ligation assembly simulation** — explicit fragment selection,
   order/orientation and end usage; construct products and verify junctions,
   topology and per-strand sequence conservation. Pairwise compatibility alone
   is not a completed assembly design.
3. **Gibson cloning** — overlap and primer design, fragment ordering/orientation,
   overlap uniqueness checks, junction verification and annotated assembled-product
   prediction. Keep Gibson homology rules separate from restriction cohesive-end
   rules. Preserve sequence/primer provenance and distinguish design predictions
   from experimental validation.
4. **GUI integration** — expose the validated shared application operations;
   resolve existing display/selection issues without reimplementing biology.

## Follow-up coverage

Broader enzyme catalogue, explicit terminal/overlapping strand-product models,
structured CLI argument errors, stronger metadata round-trip support and richer
assembly evaluation. Expand these when needed by a concrete workflow; do not
silently approximate unsupported biology.
