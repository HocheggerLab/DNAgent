# Roadmap

DNAgent is developed in small, validated slices. The CLI and the desktop app call the same
Rust application layer; neither reimplements biology. Native Rust runs all calculations;
WASM is deferred.

## Done

- Read-only SnapGene import; GenBank and FASTA read by content; lossless GenBank saving.
- Restriction sites, complete digests, end compatibility and explicit ligation plans.
- Gibson assembly: PCR-tail primer design (fixed or Tm-optimised), existing overlaps,
  mixed sources (SnapGene, FASTA, literal DNA, digest strands, PCR products) and
  annotated product GenBank.
- Offline amplification primer design with positive/negative template screening.
- Translation, six-frame ORFs and a private feature library with detection.
- Tauri desktop app: map, sequence, enzymes, feature editing with undo, tabs, agent
  handoff, isoform viewer for gene loci.

## Next

- Continuous integration running the `AGENTS.md` validation list.
- Gibson hardening: folding-energy assessment and broader real-template coverage.
  Candidate oligos are model predictions, not ordering-ready designs.
- A more direct agent–desktop channel than the file handoff.
- Consolidated provenance and diagnostics across operations.

Unsupported biology is reported, never silently approximated.
