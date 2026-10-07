---
name: gibson-cloning
description: Clone an insert (e.g. a cDNA ORF) into a vector by Gibson assembly with the DNAgent CLI - choose the vector opening and insert interval (from a DNAgent desktop handoff or explicit coordinates), design PCR primers with tails (fixed or Tm-optimised), and write the annotated product as GenBank or SnapGene .dna (carried features, the translated insert, primer sites, overlaps) plus a markdown design record, and present it in the DNAgent desktop app when one is running. Trigger on requests to Gibson-clone, subclone, put a cDNA/ORF/gene/tag into a plasmid or vector, or assemble a product from a handoff. Requires the dnagent skill for command details. Does not order oligos, choose reaction conditions or claim experimental success.
---

# Gibson cloning with DNAgent: insert into vector, annotated product

A recipe on top of the **`dnagent`** skill (load it for command contracts, handoff
rules and the design-record format). Every sequence, coordinate, primer and product
comes from the CLI; never compute primers or products yourself.

## 1. Pin down the design (ask, do not guess)

You need, explicitly:

| Decision | Examples |
|---|---|
| **Recipient vector and the site to cut** | "into pUC19 at HindIII/EcoRI"; the user usually already has a site in mind — take it and verify, do not re-derive one |
| Insert source and exact interval | the ORF selected in the handoff; `[60, 660)` of the cDNA; with or without stop codon, Kozak, tag |
| Vector opening | what the digest removes, e.g. the MCS `[232, 289)`; or a single cut that inserts without removing anything |
| Insert orientation in the product | forward (reads with the backbone) or reverse |
| Product topology | circular plasmid (usual) or linear |
| In-frame fusion? | N-/C-terminal tag or fusion partner: the junction must keep the reading frame |
| Primer method | Tm-optimised (recommended) with the user's constraints, or fixed annealing length |

### The vector is digested, not amplified

**Default: the recipient vector is cut with restriction enzymes and used as it is**
(`"preparation": "provided"`); only the insert is amplified (`"preparation": "pcr"`).
Both overlaps are then written into the insert's two primers and the backbone gets none.
That is the normal bench workflow: two oligos, no polymerase errors across the backbone,
no DpnI step.

Choose the site with `sites`/`digest`: it must be **unique in the whole plasmid** and lie
in the region being replaced. Two different enzymes give a directional opening and less
uncut background. If the user named an enzyme, use it and confirm it is unique and cuts
where expected; say so if it is not.

**If no suitable site exists**, say so and **propose amplifying the vector**
(`"preparation": "pcr"` for both cores), then ask before proceeding. Amplifying a
backbone is the exception: for when no site works, or a region must be deleted precisely.

Enzymes cut inside their recognition sequence, so the fragment you actually get may
differ from the interval you had in mind by a few bases. The backbone core must match the
real cut ends, because the insert's tails are copied from them.

From a desktop handoff (`~/DNAgent/handoff/context.json`, see the dnagent skill's
handoff reference): the insert is usually the **selected feature or selection** in the
cDNA tab, the vector is the other construct, and the opening is its selection (e.g. the
MCS). If the vector opening is not marked, ask. Work from the `snapshot` paths.

Inspect both sources (`inspect`, `features`) and confirm the insert: `translate
--feature <id>` for a CDS (start codon, no internal stop, stop present or deliberately
absent).

## 2. Express it as cores

A Gibson product is the **concatenation of oriented cores**, in plan order, with base 0
at the start of the first core. For insert-into-vector use two cores:

- **Backbone** (vector, `forward`, normally `"preparation": "provided"`): to replace
  `[a, b)` of a circular vector of length `L`, start at `b` with length `L - (b - a)`
  (it may run through the origin), where `[a, b)` is what the digest removes. For a
  single cut at `p` that removes nothing, start at `p` with length `L`.
- **Insert** (`"preparation": "pcr"`): `start`/`length` on the insert source;
  `orientation` as it should read in the product (`reverse` reverse-complements after
  extraction). With a provided vector this fragment carries **both** overlaps, so expect
  two tailed primers here and none on the backbone.

Inputs are 1-based in the plan (`"input": 1`); coordinates are zero-based on each
source's forward axis. Plan paths resolve relative to the plan file, so use absolute
paths for handoff snapshots.

```json
{
  "schema_version": 2,
  "inputs": [{"path": "/Users/me/DNAgent/handoff/pUC19.gb"}, {"path": "/Users/me/DNAgent/handoff/my_cdna.gb"}],
  "cores": [
    {"input": 1, "start": 289, "length": 2629, "orientation": "forward", "preparation": "provided"},
    {"input": 2, "start": 60, "length": 600, "orientation": "forward", "preparation": "pcr"}
  ],
  "topology": "circular",
  "overlap_length": 25,
  "constraints": {
    "min_length": 18, "max_length": 32, "min_tm_c": 58, "max_tm_c": 66, "target_tm_c": 62,
    "max_pair_tm_difference_c": 3, "min_gc_fraction": 0.3, "max_gc_fraction": 0.7,
    "max_hairpin_stem": 6, "max_dimer_run": 8, "max_three_prime_run": 4,
    "solution": {"sodium_mm": 50, "potassium_mm": 0, "tris_mm": 0, "magnesium_mm": 1.5, "dntp_mm": 0.2, "primer_nm": 250}
  }
}
```

For fixed-length primers replace `constraints` with `"annealing_length": 22` and run
`gibson` instead of `gibson-optimise`. The constraint values above are illustrative;
use the user's polymerase/buffer conditions when given, and say which you used.

## 3. Run and write the product

Keep the plan, report and any intermediate sequence in the **workspace run directory**
(`~/DNAgent/runs/`), never in `/tmp`: a design the user may come back to next week must
survive a reboot. Then:

```bash
"$DNA" gibson-optimise ~/DNAgent/runs/pUC19-myORF.json --out ~/DNAgent/pUC19-myORF.gb --name "pUC19-myORF" > ~/DNAgent/runs/pUC19-myORF.report.json
# or: "$DNA" gibson PLAN --out PRODUCT.gb --output json
```

Check exit status and `ok`. On `gibson_failed` (e.g. an overlap is not unique, no
primer pair meets the constraints) explain the reason and ask before changing cores,
overlap length or constraints. On success the report has `product_genbank` and the
desktop app offers the new file as a tab.

## 4. Annotate the insert (always)

A product assembled from a plain FASTA, a literal sequence or a digest fragment carries
**no feature for the insert** — those sources have no annotations to carry, so the map
shows the backbone and nothing where the new gene is. Add it, and let the engine find the
reading frame rather than counting bases yourself:

```bash
# Ask the engine where the insert's ORF is, rather than deriving it by hand.
"$DNA" orfs ~/DNAgent/pUC19-myORF.gb --min-codons 100 --output json
```

Pick the ORF that lies inside the insert's product span (`component.product_start` and the
core length in the report), then annotate it. For a coding insert use `--translate`, which
writes a CDS with `codon_start`, `transl_table` and a computed `/translation`:

```bash
"$DNA" annotate ~/DNAgent/pUC19-myORF.gb --add --range 6342..7842 \
  --label "MYGENE" --translate --color "#ff6b35" --out ~/DNAgent/pUC19-myORF.dna
```

- `--range` is the ORF **including its stop codon**, so the CDS reads as one open frame.
- A non-coding insert gets `--kind` instead of `--translate` (e.g. `misc_feature`,
  `promoter`), with no translation.
- Write the final file as `.dna` when the user works in SnapGene (the extension chooses
  the format); keep the `.gb` as well, since it is the lossless working copy.
- Check the written `/translation` against the source protein when there is one. If they
  differ, the cores or the frame are wrong — stop and say so.

## 5. Verify the product (always)

```bash
"$DNA" inspect ~/DNAgent/pUC19-myORF.gb --output json        # length = sum of cores; topology
"$DNA" features ~/DNAgent/pUC19-myORF.gb --output json       # carried features, Gibson F/R, overlaps
"$DNA" translate ~/DNAgent/pUC19-myORF.gb --feature <cds-id> --output json
```

- The insert CDS must translate as before (same protein, no new warnings). For a fusion,
  translate the fused span with `translate --range START..END` and require no internal
  stop across the junction and the intended frame.
- Read every `gibson_feature_clipped` warning: features cut by the opening (an MCS or a
  feature spanning it) are expected; a cut in something that should be intact (a CDS,
  promoter or origin) means the cores are wrong - stop and ask.
- Optionally `detect-features` on the product to spot library parts.

## 6. Write the design record

**Always write a design record** beside the product, as
`<product-name>.md` in the workspace. The chat scrolls away; the file is what the user
takes to the bench, and what they read again in six months. Every number in it comes from
the JSON report or a CLI check — never retyped from memory.

Required sections:

```markdown
# <product> - Gibson design record

Generated by DNAgent. Primers are **candidates** and the product a **prediction**:
sequence-verify any clone. No oligos have been ordered.

## Product
File, length, topology, backbone (with its lab database number if it has one), insert.

## Vector preparation
Enzyme(s) and recognition sequence, every cut position, which fragment is kept and
which discarded (with lengths), the end geometry. If the vector was amplified instead,
say so and why no site was suitable.

## Insert (<n> bp)
What it is and where the sequence came from (accession, isoform, database). How it is
obtained: synthesised, amplified from a clone, a digest fragment. The full sequence in a
fenced block, wrapped at 60 - this is what gets ordered or checked against a synthesis
quote.

## Primers
A table: direction, annealing Tm, tail length, annealing length, full 5'->3' sequence.
State which tails are the Gibson overlaps, and the buffer conditions assumed.

## Overlaps
Junction, product position, sequence.

## Checks run
The arithmetic (product length = sum of cores), the ORF and its translation, any
reconstituted restriction sites, and any independent reading of the file.

## Not checked
Reaction conditions, folding energies, experimental success.
```

## 7. Show it in the app, then report

If the `mcp__dnagent__*` tools are available (see the dnagent skill's
[Live desktop session](../dnagent/references/live-session.md)), **finish by putting the
product on the user's screen** rather than leaving the file to be noticed:

```text
present(path: "~/DNAgent/pUC19-myORF.dna",
        summary: "<what was assembled, what to check>",
        highlights: [{label: "MYGENE CDS", ...}, {label: "5' junction", ...}, {label: "3' junction", ...}])
```

Highlight, in this order: the annotated insert, the two junctions (the overlap spans from
the report), and the vector's cut site if it is still visible in the product. Coordinates
come from the JSON report, never from arithmetic you did yourself.

**If the insert sequence came from an isoform decision** (a cDNA cloned from scratch, with
the isoform chosen from degron-db or another database), `open_file` the evidence *first* —
the `GENE.locus.json` locus bundle, which opens an Isoforms tab — and `present` the product
after it. The user then has the justification and the construct side by side, and the
product is the active tab. Tell them to click **Isoforms** on the locus tab; it lands on Map.

The GUI is optional. `status` failing, or the tools not existing, changes nothing about the
work: the written file and the design record are the deliverable.

Then, in the chat, give the same thing in brief: product path, length and topology; the
cores with their preparation; the primer table; all warnings and assumptions; and the
checks you ran. Say that primers are **candidates** and the product a **prediction**:
sequence-verify clones; do not order oligos unless asked.

## Worked example (public fixtures, software practice only)

With the app checkout at `$DNAGENT_REPO`:

```bash
"$DNA" gibson-optimise "$DNAGENT_REPO/fixtures/plans/synthetic-cdna-into-puc19-optimised.json" --out /tmp/pUC19-ORF.gb
```

Clones the 600 bp CDS of a synthetic cDNA into pUC19 in place of its polylinker:
3,229 bp circular product; the ORF, lac and pBR322 parts carried; M13mp19 (spanning the
polylinker) left out with a warning; four primers and two overlaps annotated. That plan
amplifies both fragments; `synthetic-cdna-into-puc19-digested.json` is the same clone the
default way — vector provided, two primers.
`scripts/check_skill.py --repo CHECKOUT --binary EXECUTABLE` runs this smoke check.
