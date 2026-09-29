# DNAgent architecture review — decisions and alternatives

**Review baseline:** commit `243c0e4`, branch `feat/mixed-gibson-products`.
**Purpose:** discussion document, not an approved redesign or implementation plan.

**Subsequent decision (2026-09-17):** the GUI is a central desktop interface; use
Tauri with a TypeScript frontend and native Rust services, with WASM deferred.
The toolkit/defer-GUI discussion below is historical; see `desktop-architecture.md`
for the agreed direction and prototype.
**Evidence:** current source, workspace manifests, contributor guidance, roadmap and validation scripts. Statements about intended priorities come from repository documentation; trade-off assessments and recommendations below are this review's interpretation. No new experimental validation or independent algorithm audit was performed for this report.

## Executive assessment

DNAgent is currently best understood as a **local, deterministic cloning engine with a command-line interface**, rather than a small imitation of the SnapGene desktop application.

That is a good foundation for its stated purpose: an agent should be able to request a calculation, inspect the exact inputs and assumptions, and receive a reproducible result. The strongest decisions are the separation of biological calculations from interfaces, explicit circular/strand coordinates, conservative handling of unsupported biology, and substantial invariant/reference testing.

The main architectural weakness is emerging at the boundary between **a sequence and a prepared physical fragment**. The recent mixed-source implementation makes the KNIT workflow possible, but normalises digest and PCR products into `SequenceRecord` objects. That simplifies assembly while discarding structured preparation context. The output preserves associations to those derived records, not a complete chain back through their preparation steps to the original sources.

**Recommendation:** retain the Rust engine and existing layers. Before adding many more workflows, introduce a small, explicit artifact/preparation/provenance model and an application-level execution result carrying diagnostics on both success and failure. This is an incremental consolidation, not a rewrite.

## 1. Architecture as implemented

```text
CLI (clap)                         Desktop GUI (eframe/egui)
    |                                      |
    +----------- application layer --------+
                 |          |
                 |          +--> format adapters --> checked records
                 |
                 +------------> biological domain operations

Domain records --> shared map scene --> SVG / GUI painting
Domain reports --> JSON serialization / conservative format exports
```

This is a conceptual flow, not a strict dependency diagram: CLI and GUI also depend directly on domain, format and rendering types.

| Crate | Current responsibility | Assessment |
|---|---|---|
| `dnagent-domain` | Checked DNA/locations/features; restriction, digest, compatibility, ligation, Gibson and primer calculations | Appropriate centre of the program; independent of filesystem and UI, though coupled to Serde serialization |
| `dnagent-formats` | Import reports, warnings, retained source packets; GenBank and assembly exports | Useful boundary, but mixes generic contracts with SnapGene-specific metadata and concrete exporters |
| `dnagent-format-snapgene` | Read-only packet/XML interpretation | Sensible isolation of a complex migration format |
| `dnagent-app` | File loading, plan parsing, source resolution, operation entry points and views | Correct intended role, but FASTA parsing and PCR construction have also accumulated here |
| `dnagent-render` | Domain-to-map scene and SVG rendering | Reusable visual projection, separate from biology |
| `dnagent-cli` | Commands, output modes, warnings, strict policy and error envelopes | Agent-facing interface; still owns some orchestration that a GUI would otherwise need to repeat |
| `dnagent-gui` | Basic Map/Features/Sequence views and interaction state | Deliberately behind CLI capabilities; not a mature cloning workbench yet |

Seven crates are not inherently excessive: they enforce useful boundaries at compile time. Further crate splitting is not needed simply to make the diagram more elaborate.

## 2. Major decisions and credible alternatives

### 2.1 Rust library-first, rather than Python-first or GUI-first

**Current choice.** Rust workspace, checked types, forbidden unsafe code, minimal domain dependencies. Default CLI build is headless; GUI dependencies are optional for that package. Python is used for reference checks, not as the production engine.

**Benefits:** reusable calculations, explicit failure handling, good deployment as a local executable, and strong compile-time constraints. It also serves the project's Rust-learning aim.

**Costs:** scientific integrations and exploratory algorithms generally take more code than in Python. Maintaining custom scientific algorithms creates a validation obligation regardless of language.

**Alternatives:**

- **Python + Biopython/pydna:** faster experimentation and broader scientific functionality; more runtime/environment management and fewer compile-time guarantees. This would have been a credible initial choice, especially for a research-only tool.
- **Rust core + Python bindings:** preserves the engine while supporting notebooks. Adds API packaging and cross-language testing; not necessary just to call the existing CLI.
- **GUI-first monolith:** quickest route to interactive features, but risks embedding calculations in event handlers and making agent access secondary.

**Recommendation:** keep Rust. Use established external scientific implementations as reference or optional specialist backends where justified; do not reimplement every thermodynamic model merely for language consistency.

### 2.2 Layered modular program, not services or plugins

**Current choice.** Ordinary in-process function calls across crates. No required server, database or dynamic plugin framework.

**Benefits:** simple operation, reproducibility, low deployment overhead, straightforward testing.

**Costs:** supported operations and formats require compiled changes. Application services are not yet sufficiently complete for full CLI/GUI parity.

**Alternatives:** HTTP/MCP service; plugin registry; a single crate with modules. Each can be useful, but none resolves the current biological/provenance modelling problem.

**Recommendation:** keep the modular local program. Add an MCP or HTTP adapter only when a concrete client needs it, wrapping the same application services. Traits are useful at actual replaceable boundaries; introducing a trait for every operation would add ceremony rather than flexibility.

### 2.3 Checked sequence and coordinate types

**Current choice.** Canonical uppercase IUPAC DNA; private checked record/location fields; zero-based half-open coordinates. Circular arcs use start plus length. Multipart feature order and strand are preserved.

**Benefits:** prevents common off-by-one, origin-crossing and feature-flattening errors. Ordered qualifier lists preserve repeated and valueless qualifiers.

**Costs:** format conversions need careful coordinate translation. Operation request/report types often still expose raw `usize` and `String`, so the checked-type discipline is stronger in foundational records than across the full API.

**Alternatives:** use raw strings and intervals everywhere; normalise circles into split linear intervals; use an established generic sequence model. These reduce some implementation effort but can lose biological identity or permit invalid states.

**Recommendation:** retain the current foundation. Gradually introduce checked operation-specific inputs, such as an exact-ACGT sequence view and validated selections, where they remove repeated checks. Do not attempt an enormous universal biological type system.

### 2.4 Separate reference sequences from physical duplex geometry

**Current choice.** `SequenceRecord` stores a reference sequence, topology and annotations. Digests and ligation have richer top/bottom strand sequences, staggered boundaries, overhangs and paired lengths. Exact-overlap Gibson works with oriented single-sequence views and assumes an ideal fully paired product.

**Benefits:** the digest model correctly represents situations where top-strand length, bottom-strand length and paired-core length differ. Gibson remains comparatively simple.

**Costs:** conversions between models become critical. A selected digest strand is not the complete physical substrate, and trimming a selected sequence is not itself a model of end processing in a reaction.

**Alternatives:**

1. One sequence model for everything: simpler but inadequate for sticky ends and strand-phase conservation.
2. Full duplex model for every operation: more explicit, but burdens normal sequence operations with physical detail they do not need.
3. **Layered artifact model:** reference sequence plus optional, operation-specific physical state and explicit conversions.

**Recommendation:** option 3. Keep specialised duplex calculations; make conversion to an ideal Gibson fragment a named preparation/projection step. Never imply that passing exact-overlap validation proves the declared physical preparation will work experimentally.

### 2.5 Explicit plans before automatic design

**Current choice.** The caller supplies source selections, order, orientation, topology and overlaps. Optimisation searches a bounded primer space; it does not discover an entire cloning strategy.

**Benefits:** deterministic, auditable and testable. Particularly appropriate when an agent proposes a plan and DNAgent checks it.

**Costs:** the user/agent must solve the higher-level design problem. Strict overlap-uniqueness and length policies reject some designs that may still be experimentally usable.

**Alternatives:** automatic overlap graph search; target-driven synthesis/PCR/digest planning; reaction simulation.

**Recommendation:** preserve an explicit execution engine. Future planners should generate candidate plans for that engine, not introduce a second execution path. Keep three different conclusions separate:

- the declared product is sequence-consistent;
- the design meets a chosen conservative policy;
- the experiment is predicted or demonstrated to work.

Those are not interchangeable.

### 2.6 File-based plans and self-contained reports

**Current choice.** Versioned JSON plan files, relative path resolution, embedded sequences in output, no persistent project store. CLI envelope schema and software package have separate versions. Domain report types are often serialized directly.

**Benefits:** inspectable files, good scripting, no server dependency, straightforward archival.

**Costs:** larger repetitive outputs; manually maintained schemas can drift; internal refactoring can accidentally become a public schema change. Plans depend on mutable external files unless archived with their inputs.

**Alternatives:** stable transport DTOs independent of domain types; schema generation; project bundles; database-backed projects.

**Recommendation:** keep files and JSON, but introduce stable application response types at evolving boundaries. Consider generated structural schemas while retaining hand-written semantic validation and rejection tests. A portable bundle containing a plan, source hashes, inputs and results is more immediately useful than a database.

## 3. The most important design issue: provenance through preparation

The KNIT implementation exposed this clearly:

```text
original plasmid -> digest -> selected strand -> selected interval -> assembly
original plasmid -> PCR interval + tails -> PCR product -> assembly
synthetic sequence -------------------------------------> assembly
```

Today the assembly mostly sees the prepared sequences. `GibsonSource` is an untagged enum; its variants are inferred from fields. Digest/PCR source loading returns `ImportReport`, replaces its record with a derived record, and warns about lost context. PCR candidates are stored in `ImportedPrimer`, despite being generated rather than imported.

This is a pragmatic first slice, but not the final abstraction:

- Preparation history is not a first-class object in the assembly result.
- Digest end geometry and original annotations are absent from that projected record.
- PCR tail bases are not structurally distinguished from template-derived bases in the resulting `SequenceRecord`.
- A fragment ID such as `fragment-0002` is meaningful in its particular digest, not a globally stable biological identity.
- Source names/descriptions are not substitutes for machine-readable provenance.

### Alternatives

**A. Keep flat records and improve names/warnings.** Cheapest, but provenance becomes increasingly dependent on accompanying plans and prose.

**B. Introduce prepared artifacts with provenance.** A manageable middle ground: an artifact has identity, sequence/topology, preparation type, parent references, coordinate mappings and diagnostics. Assembly consumes artifacts without needing to reimplement preparation.

**C. Full workflow graph/event-sourced project system.** Supports branching designs, replay and eventual undo, but introduces persistence, migrations and graph execution before those needs are established.

**Recommendation: B now, designed so C remains possible.** Store parent references and transformations without building a workflow scheduler or database. Keep product and preparation reports distinct but connected.

An illustrative future plan might explicitly distinguish `kind: file`, `kind: digest_fragment`, `kind: pcr_product` and `kind: literal`. A versioned tagged representation would improve diagnostics and schema clarity; preserve the existing version through a compatibility reader rather than silently changing it.

### Hashes require a policy decision

A sequence checksum alone is not complete identity. Specify separately:

- original file-byte hash;
- canonical sequence hash;
- topology and annotation identity;
- preparation parameters and parent identities;
- software/schema versions.

For circles, rotated strings have different ordinary hashes despite representing the same circular sequence. The KNIT comparison demonstrated this. Start with an exact stored-sequence hash; add separately labelled rotation-invariant identity only if needed. Do not silently canonicalise origin or reverse orientation and lose the coordinate frame used by annotations.

## 4. Diagnostics should be application data, not CLI bookkeeping

**Current strengths:** warnings are visible, JSON errors are structured, and strict mode rejects warned imports. Unknown SnapGene content can remain preserved outside the domain rather than being silently interpreted.

**Current limitations:**

- `ImportWarning` now also represents intentional assembly projections.
- Strict mode treats all warnings alike, including expected FASTA limitations and potential data loss.
- The CLI accumulates warnings and chooses error codes; other adapters must reproduce policy.
- A compound loader can import successfully and fail later before returning its report. Its accumulated import warnings then cannot reach the CLI through the current `Result<ImportReport, AppError>` boundary.
- Retained raw packets describe the original file, not newly derived biology; their ownership must remain explicit.

**Recommendation:** an application outcome containing structured diagnostics on success **and failure**, including stage, source/artifact ID, category and severity. Preserve the current strict behaviour as a policy, but eventually allow explicit policies such as “reject information loss” versus “reject every advisory”. Move PCR sequence construction into a pure domain function and FASTA parsing into a format module; let application services orchestrate both.

This is more valuable than merely dividing the CLI's main file into smaller files.

## 5. Product annotations: conservative export is correct, but incomplete

The current distinction between source annotation coverage and biological validity is excellent. A retained CDS interval does not automatically remain in frame or functional. Exporting component `misc_feature` records avoids pretending otherwise.

However, repeated transformations need composable coordinate mappings: original source → prepared fragment → assembled product. Shared Gibson overlap bases can legitimately have more than one source association. Reverse orientation and circular closure must remain explicit.

**Recommendation:** introduce common mapping primitives before building more independent annotation projection code. Preserve specialised reaction models, but share the coordinate/provenance machinery. Add coding analysis later as a separate, reference-aware validation step; do not make annotation transfer silently infer gene function.

## 6. Testing and scientific assurance

The repository combines Rust unit/integration tests, public synthetic fixtures, live JSON-schema checks, conservation invariants, pinned Biopython comparisons, exhaustive bounded checks and optional private-corpus validation. This is a major architectural strength.

Important limits remain:

- Reference checks are not experimental evidence.
- Custom assembly reference scripts are not the same as comparison with a separately developed assembly engine.
- The KNIT acceptance run compares with a previous derived design, not a sequence-verified physical plasmid.
- A large number of cases does not guarantee coverage of preparation/provenance transitions.

**Next testing priorities:** reverse/origin-spanning PCR preparation, warning retention on failures after import, strict rejection before output, malformed public report exports, multi-stage mapping composition, schema/runtime agreement and circular rotation equivalence. Property-based tests are a natural complement to current fixed and exhaustive cases.

Performance is not yet the main architectural problem. Owned strings and cloned records are acceptable at plasmid scale. Profile before introducing ropes, persistent sequence structures or shared-memory caches. Resource limits should eventually cover input loading and intermediate allocations as well as final products.

## 7. GUI direction and the risk of indefinite deferral

The CLI-first decision has prevented UI-driven biology. Keep it. But “Gibson complete before GUI” can expand indefinitely: folding-energy models, GenBank fidelity and automatic planning are all substantial projects.

Define a bounded GUI milestone instead: **display an already validated assembly, its component mappings, warnings and junctions without calculating a different answer**.

The current Rust-native egui approach avoids a web stack. Alternatives include a web UI with a Rust backend, Tauri, or a notebook interface. Those may offer richer interaction or easier sharing, but also add language, packaging and synchronization boundaries. Nothing in this review establishes a need to change toolkit.

## 8. Recommended direction

### Keep

- Rust, library-first and local/headless by default.
- Checked sequence/location types and explicit duplex geometry.
- Explicit deterministic plans as the execution contract.
- Conservative scientific claims and independent reference testing.
- Separate UI interaction state and reusable rendering.

### Consolidate before broad feature expansion

1. Define prepared artifacts and parent-linked provenance; add exact sequence/file hashes with documented semantics.
2. Introduce shared application execution outcomes and diagnostic policy.
3. Move PCR calculations and FASTA parsing to their appropriate layers.
4. Add composable source-to-product mappings and distinguish generated primers from imported metadata.
5. Stabilise plan/report boundaries, with explicit migration/version policy.

Do this incrementally, retaining current commands and using the mixed KNIT workflow as a regression case. No microservices, plugin framework, database or whole-program rewrite is justified now.

## 9. Questions for our discussion

1. **Primary product:** a trustworthy agent-facing cloning engine, or a full interactive SnapGene replacement? My recommendation is engine first, with a deliberately bounded viewer/workbench.
2. **Meaning of a product:** just a predicted sequence, or an auditable prepared artifact with its full derivation? I recommend the latter, while modelling only supported physical properties.
3. **Default strictness:** should expected format limitations block automation, or should callers select a diagnostic policy?
4. **Design scope:** should DNAgent validate supplied plans, automatically propose plans, or both through separate layers?
5. **Annotation ambition:** component provenance only, faithful coordinate transfer, or biological reconstruction? These are three different milestones.
6. **Scientific dependencies:** which calculations should remain ours, and which should use established external tools?
7. **GUI gate:** what finite set of engine capabilities is enough to begin useful interaction?

**Bottom line:** the foundation is sound. The next important decision is not another feature or language—it is whether prepared fragments and their derivation become explicit core concepts, rather than remaining sequences accompanied by warnings.

## Source guide

- `Cargo.toml`, `crates/*/Cargo.toml`: workspace, dependency and headless-build boundaries.
- `AGENTS.md`, `docs/roadmap.md`: intended architecture, sequencing and validation policy.
- `crates/dnagent-domain/src/lib.rs`: checked records, locations and annotation representation.
- `crates/dnagent-domain/src/{digest,ligation}.rs`: duplex/end geometry and conservation.
- `crates/dnagent-domain/src/{gibson,existing_overlaps,primer_optimisation}.rs`: explicit assembly and bounded design contracts.
- `crates/dnagent-domain/src/fragment_annotations.rs`: source-associated annotation projections.
- `crates/dnagent-app/src/{lib,gibson,gibson_extensions}.rs`: intake, preparation and application boundaries.
- `crates/dnagent-formats/src/{lib,assembly,genbank}.rs`: fidelity metadata and conservative exports.
- `crates/dnagent-cli/src/main.rs`: command orchestration, strictness and envelopes.
- `crates/dnagent-{render,gui}/src/lib.rs`: map projection and UI state.
- `schemas/`, `scripts/check_*.py`, Rust test modules: public contracts and assurance infrastructure.
