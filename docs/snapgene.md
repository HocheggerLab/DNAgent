# SnapGene `.dna`: reading and writing

DNAgent reads SnapGene files and writes them, so a construct can go out to a lab that works
in SnapGene without an import step. Source files are never modified in place: writing always
goes to a path you choose.

## Two ways a file is written

**Verbatim.** A record that DNAgent read from `.dna` and has not edited since is written back
**byte for byte** — the same cookie version fields, the same sequence flag bits, the same
packet order, and the annotation packets as their original XML. Nothing SnapGene stores is
lost by passing a file through DNAgent, including the packets DNAgent does not interpret.

Before copying those packets, DNAgent re-reads them and compares them with the record. If
they no longer agree, it does not copy them: a file whose annotation packets contradict its
own model would be worse than no file.

**Generated.** Anything else — an edited record, a Gibson product, a construct read from
GenBank or FASTA — has its `Features` and `Primers` packets built from DNAgent's model. Such
a file holds exactly what DNAgent models and is reported as generated. It does **not** carry:

- the uninterpreted packets of a source file (`0x03`, `0x08`, `0x0d`, `0x0e`, `0x11`, `0x23`,
  `0x1c`, notes `0x06`), because their meaning is unknown and they may describe annotations
  the record no longer has; each is named in a warning;
- SnapGene's display and detection attributes (`translationMW`, `maxRunOn`, `detectionMode`,
  `allowSegmentOverlaps`, …), which DNAgent does not model.

Sequence flag bits other than topology (methylation and similar settings) are carried over
when the record came from a SnapGene file that set them, and reported.

## Writing one

```bash
dnagent convert construct.gb --out construct.dna      # the extension chooses the format
dnagent annotate construct.dna --out annotated.dna --add --range 100..160 --label probe
dnagent gibson-optimise plan.json --out product.dna   # hand the product to a SnapGene lab
```

In the desktop app, **Save as…** offers SnapGene alongside GenBank; the status line says
which format was written and repeats any warning.

## What is checked

- Every valid public fixture, and (opt-in, via `DNAGENT_PRIVATE_DNA`) real SnapGene files,
  are read and written back **byte for byte**.
- Taken the long way round — `.dna` → DNAgent GenBank → `.dna`, which forces regeneration —
  **Biopython 1.85**, which never sees DNAgent's own reader, finds the same sequence,
  topology and features in the rebuilt file as DNAgent models in the original
  (`scripts/check_snapgene_write.py`).
- A record whose annotations no longer match its source packets is refused the verbatim
  path, and the desktop save reports the regeneration.

Opening a generated file in SnapGene itself is the acceptance test these cannot replace.
