## v0.2.0 (2026-10-07)

### Feat

- build and ship for Windows
- **desktop**: open a gene locus on Isoforms, ranked by expression
- write SnapGene .dna from the model, not just copy it back
- write SnapGene .dna back byte for byte when it was only read
- say how each Gibson fragment is prepared; digest the recipient vector
- agents point back in the desktop app (open, select, notify, present)
- live agent connection over MCP (read tools)
- isoform viewer for degron-db gene loci; SVG export; large loci up to 3 Mb
- write the Gibson product as annotated GenBank; agent handoff scenario
- similarity grouping of same-named point and codon variants in the feature library; fix e2e waits
- variant families in the feature library; detection folds shorter variants into their longer match
- **desktop**: delete any feature with Delete/Backspace after an in-app warning; re-add from the library
- **desktop**: Detect features from the feature library, tick and add as one undoable edit; CLI-checked e2e
- SQLite feature library built from sequence collections, detect-features, Biopython/brute-force checked
- **desktop**: restriction sites on map and sequence, enzyme sets and chooser, digest panel; CLI-checked e2e scenarios
- built-in enzyme set, optional pinned REBASE catalogue, degenerate/interrupted/outside-cutting sites, Biopython-checked
- **desktop**: document tabs, agent handoff snapshots with context.json, watched ~/DNAgent workspace
- **desktop**: stateful edit session with undo/redo/save, live Rust e2e server, editing UI
- lossless GenBank read/write with DNAagent extensions, convert and annotate commands, Biopython oracle
- **desktop**: sequence view with CDS amino acids, six frames, ORF tracks, map ORFs and drag-selected range translation
- **desktop-api**: expose CDS translations, six-frame translations and ORFs
- **cli**: add translate and orfs commands with schema, synthetic fixture and Biopython oracle
- **domain**: add pinned NCBI genetic codes, IUPAC-aware translation and ORF finding
- **desktop**: redesign map with scaling canvas, lanes, block arrows, labels, selection band and themes
- **desktop**: add gitignored design-review gallery for public and local constructs
- **desktop**: route IPC through one seam and add e2e-only automation API
- provision pinned local human reference databases for BLAST
- add bounded offline amplification primer design and provenance
- add linked map and duplex sequence tabs with feature tracks
- label desktop maps and expose strand arrows and native file picker
- prototype Tauri desktop with generated Rust TypeScript contracts
- support mixed-source Gibson products
- optimise Gibson primers and assemble existing overlaps
- design explicit PCR-tail Gibson candidates
- simulate explicit restriction and ligation products
- project digest annotations and export strand FASTA and GenBank views
- validate restriction-end compatibility and address review findings
- simulate complete restriction digests with explicit strand ends
- harden headless CLI and add restriction site analysis
- scaffold DNAagent milestone 1

### Fix

- allow the Windows serve stub to be async without awaiting
- forward each agent message instead of splicing the stream
- keep the source file's sequence casing when generating .dna
- draw amino acids over their codons in the Sequence view
- read files by content (GenBank saved as .dna), name non-sequence uploads, ApE qualifier continuations, keep ORIGIN placeholders as N
