# Restriction analysis and explicit ligation

## Sequence of operations

1. Inspect each distinct source and warnings first.
2. Query the small enzyme catalogue, then only the requested recognition sites.
3. Run a complete digest (`digest`) or annotated digest (`fragments`) for chosen
   enzyme sets. Inspect actual fragment IDs, both strands, ends and clipping.
4. Use `compatible-ends` for an exploratory pairwise comparison if necessary.
5. For a known order, write an explicit `ligate` plan and inspect every junction,
   both product strands, circular phase, source placements and unused fragments.

```bash
"$DNA" enzymes --output json
"$DNA" sites source.dna --enzymes EcoRI,BamHI --output json
"$DNA" digest source.dna --enzymes EcoRI,BamHI --output json
"$DNA" compatible-ends source.dna --enzymes EcoRI,BamHI --output json
"$DNA" compatible-ends vector.dna --enzymes BsaI --other insert.dna --other-enzymes BsmBI --output json
```

The second-file flags are paired: `--other` needs `--other-enzymes` and vice versa.

## Catalogue and conservative cleavage

The catalogue is a built-in set of **99 commercial enzymes** (degenerate, interrupted
and outside-cutting sites included), or a locally installed, pinned REBASE release
(`python3 scripts/manage_enzymes.py install` in the app checkout; never committed).
`"$DNA" enzyme-catalogue` reports which is active; `DNAGENT_ENZYMES=builtin` forces the
built-in set. Use the live catalogue rather than inferring support for other enzymes. Selection
is explicit, case-insensitive and deduplicated; results use catalogue ordering.

`sites` reports recognition and nominal strand-cut positions, including reverse
Type IIS sites and circular seams. A recognition site is **not** proof of two
available cuts on a linear source. Inspect cut availability/warnings; strict mode
rejects these warnings too.

Complete digestion rejects ambiguous sequence, out-of-bounds cuts, terminal
single-stranded products and overlapping cuts/nonpositive paired cores. It does
not silently skip an inconvenient site or return partial products. Use `sites` to
explain why cleavage is unavailable, not to hand-construct a partial digest.

Uncut linear molecules are modelled as blunt-ended; uncut circles have no free
ends. That blunt-end convention is a model assumption, not knowledge of physical
sample preparation. Methylation, star activity and experimental cleavage yield
are not assessed.

## Ends and orientation

Each physical strand/overhang is written **5′→3′**. Compatible sticky ends have
complementary exposed strands, not identical oligo strings. Respect polarity,
overhang length and sequence; blunt compatibility is not a yield prediction.

Compatibility reasons distinguish blunt matches, complementary overhangs,
polarity mismatch, length mismatch and sequence mismatch. Proposed orientation
and before/after placement are relative to the compared endpoints; same-side
ends can require reversal. Follow the reported placement rather than applying a
second guessed reverse complement.

The exploratory matrix is capped at **128 endpoints / 8,128 unordered pairs**.
For a specified assembly use the explicit ligation plan rather than building a
larger matrix yourself. Pairwise compatibility alone does not validate an assembly.

## Version-1 ligation plan

Shape only: the example IDs are **not verified for your files**. Obtain IDs from
current digest reports; do not infer `fragment-0001` means the desired backbone.

```json
{
  "schema_version": 1,
  "inputs": [
    {"path": "vector.dna", "enzymes": ["EcoRI", "BamHI"]},
    {"path": "insert.dna", "enzymes": ["EcoRI", "BamHI"]}
  ],
  "fragments": [
    {"input": 1, "fragment_id": "fragment-0002", "orientation": "forward"},
    {"input": 2, "fragment_id": "fragment-0001", "orientation": "reverse"}
  ],
  "topology": "circular"
}
```

```bash
"$DNA" ligate plan.json --output json
```

- Source paths are relative to the plan. Each input is a distinct **digest instance**,
  numbered from one. The fragment array is explicit product order and orientation.
- A particular `(input, fragment_id)` can occur only once. For another molecule
  copy declare another input instance (same source path is allowed) and select
  from that instance. Do not bypass the rule by renaming fragment IDs.
- Uncut circles are not ligatable, even as pass-through products. A one-fragment
  linear selection is a supported zero-junction pass-through.
- Circular topology adds last-to-first closure; do not silently switch to linear
  if closure fails. All complete digests must succeed, even for unused sources.
- Bounds: 16 inputs, 128 components, 10,000,000 bases per product strand.

## Read the predicted product correctly

Ligation concatenates physical strand material; it **does not trim sticky-end
sequence as if it were duplicated Gibson homology**. Reversal swaps source top
and bottom 5′→3′ strands and ends, not a guessed RC of the physical overhang oligo.

The product bottom is stored in its own 5′→3′ direction, built in reverse component
order. For a circle, `bottom_forward_start` records its forward-axis phase. Do not
rotate it to force direct equality with `RC(top)`, or report a phase offset as a
sequence error. Linear staggers can give negative forward starts.

Component `top` and `bottom` spans are on those respective product strand axes.
Use `source_strand` to choose the original projected strand, then add the span
start to local annotation coordinates. Junction compatibility already accounts
for selected orientation: its relative `forward` does not undo a reversed component.

Keep `unused_fragments`, full input digest reports and clipping flags in the
handoff. No annotation reunion, gene fusion, coding repair or updated translations
are inferred. No trimming, polishing, fill-in, phosphorylation or nick sealing
chemistry is simulated; predicted sealing assumes compatible junctions.

Canonical app references: `docs/restriction.md`, `docs/digest.md`,
`docs/compatibility.md`, `docs/ligation.md`, `schemas/ligation-plan-1.schema.json`.
