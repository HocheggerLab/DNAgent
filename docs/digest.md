# Complete restriction digest simulation

```bash
cargo build -p dnagent-cli --locked
target/debug/dnagent digest construct.dna --enzymes EcoRI,BamHI --output json
# Public demonstration (not a lab construct):
target/debug/dnagent digest fixtures/formats/snapgene/synthetic_restriction_linear.dna \
  --enzymes EcoRI,BamHI,EcoRV,KpnI,BsaI,BsmBI --output json
```

The `digest` command calls the typed application layer and the filesystem-free
`dnagent_domain::digest::simulate_digest` operation. It uses the same six-enzyme
catalogue and recognition/cut rules as [`sites`](restriction.md).

**This is a complete, sequence-only cleavage model, not experimental validation.**
Methylation, star activity, incomplete digestion, reaction conditions, flanking
DNA requirements and gel migration are not modelled. Imported files are not
modified. Fragment features, primers and retained metadata are not propagated;
this limitation is included in every result's assumptions. This operation returns
fragment descriptions, not newly annotated `SequenceRecord` objects.

## Contract (envelope 0.4.0)

Digest simulation was introduced in `schemas/cli-envelope-0.4.0.schema.json`.
Current envelopes use 0.7.0, adding [ligation products](ligation.md) after
0.6.0's [annotation projections](fragment-annotations.md)
after 0.5.0's [end compatibility](compatibility.md). The digest payload shape is
unchanged. Historical schemas remain available for archived responses only.

A successful digest contains:

- `input_length`, `input_topology`, selected `enzymes` and model `assumptions`.
- `cuts`: unique physical top/bottom boundary pairs, grouped with their enzymes.
- `fragments`: deterministic IDs and input-forward-coordinate order, **not** size
  order. Each has `topology`, `top`, `bottom`, `paired_length`, `left_end`, `right_end`.
- Each strand has `source_start`, `length`, `sequence_5to3`. Both sequences are
  written **5′→3′ on their own strand**. `source_start` is the lower boundary of
  the source interval on the **stored forward coordinate axis**, including for
  the bottom strand; it is not the bottom oligo's 5′ end. A circular source
  interval can wrap and is unambiguously described by start plus length.
- Ends report `polarity` (`blunt`, `five_prime`, `three_prime`),
  `protruding_strand` (`forward` = top, `reverse` = bottom, or null), the exposed
  `overhang_sequence` written 5′→3′, enzymes and `original_terminus`.

There is deliberately no single ambiguous fragment `length`: the two strand
lengths can differ. `paired_length` counts the duplex core, excluding exposed
bases. Text output labels top bases, bottom bases and paired bases separately.
It is not a gel-band prediction.

Uncut circles retain circular topology and null ends. Uncut linear molecules
retain their assumed-blunt original ends. A circle with one unique cut becomes
one linear fragment; linear molecules with `k` accepted cuts produce `k + 1`
fragments, circles with `k > 0` produce `k`. Selection order and duplicate enzyme
names do not change the result. Identical cut pairs are merged, including
convergent Type IIS sites that produce the same physical cuts.

## Strand geometry

For adjacent cut pairs `(t₁, b₁)` and `(t₂, b₂)`, a fragment comprises the top
source interval `[t₁, t₂)` and the reverse complement of the bottom source
interval `[b₁, b₂)`. The paired core is
`[max(t₁, b₁), min(t₂, b₂))`. Circular calculations use unwrapped boundaries;
only reported coordinates are reduced modulo input length. This avoids confusing
an origin-crossing 4-base overhang with an almost-full-molecule overhang.

At a cut with `bottom > top`, the left fragment exposes a bottom-strand 5′
overhang and the right fragment exposes the complementary top-strand 5′ overhang.
For `bottom < top`, those are top- and bottom-strand 3′ overhangs respectively.
Blunt ends have no protruding strand and an empty overhang sequence.

For example, EcoRI cleavage of `AAAGAATTCTTT` gives:

| Product | Top, 5′→3′ | Bottom, 5′→3′ | Paired bases |
| --- | --- | --- | ---: |
| Left | AAAG | AATTCTTT | 4 |
| Right | AATTCTTT | AAAG | 4 |

The top strands sum to 12 bases, and the bottom strands independently sum to 12.
The exposed ends are complementary `AATT` 5′ overhangs. Top-only slicing would
miss the asymmetric strand lengths and the actual duplex core.

## Conservative rejection policy

Errors return `digest_failed` with no partial fragment result:

- Ambiguous sequence, unknown enzyme, empty selection or unsupported short circle.
- Any selected site with an unavailable cut. Unlike `sites`, digest cannot merely
  warn and omit it while calling the remaining products a complete digest.
- Any cut at a linear terminus on either strand. Single-stranded terminal
  products are outside this initial model.
- Overlapping **or touching** cleavage regions that leave no positive paired
  core between adjacent cuts. This includes the closing interval of a circle.
  These cases need a richer strand-product model, not invented duplex fragments.

Import warnings remain in the envelope. `--strict` still rejects them before
analysis with `import_warnings`. No fragment files are written by this command.

## Validation and provenance

```bash
cargo test --workspace --locked
uv run scripts/check_cli_schema.py --binary target/debug/dnagent
uv run scripts/check_digest.py --binary target/debug/dnagent
# Optional private regression corpus; never place it in the repository:
# append --manifest "$PRIVATE_CORPUS/manifest.json"
```

Hand-checked Rust examples cover blunt/5′/3′ ends, non-palindromic Type IIS
oligos, reverse recognition, circular origins, no/single/multiple cuts,
coincident cut merging and rejected geometries.

The independent checker uses **Biopython 1.85**, `Bio.Restriction.catalyse`, on
both the input strand and its reverse complement for single-enzyme products.
Multi-enzyme reference partitions use the union of Biopython's public `search`
positions. All circle rotations are exercised, including mixed-polarity digests.
The comparison matches the domain's documented uppercase intake convention;
source files are unchanged. Additional assertions check each strand's base
conservation and source-coordinate reconstruction, paired core plus exposed
bases, protruding oligo position and complementary adjacent ends. JSON responses
are schema-validated, including unsupported cases.

Synthetic substrates are deterministic originals, generated inside temporary
directories; committed examples reuse the public synthetic fixture generator.
Optional private EcoRI/BamHI digests are hash-checked and output no sequences.
No reference source code or enzyme database was copied.

The separate [end-compatibility operation](compatibility.md) now evaluates these
ends. The separate [fragments operation](fragment-annotations.md) now projects
source annotations onto both strands and exports JSON/FASTA plus conservative
[GenBank strand views](genbank-export.md). [Ligation products](ligation.md) now include explicit component placements.
Next is Gibson cloning before GUI integration; see [the roadmap](roadmap.md).
Terminal/overlapping strand-product support remains a follow-up limitation.
