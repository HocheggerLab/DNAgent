# Local reference store (provisioning v1)

Use compressed public FASTA originals plus BLAST's native version-5 indexes, not a
SQL database for sequence searching. Reference data stays **outside Git**, shared
between projects. This installer provisions data; it does not yet connect BLAST
results to `primer-design` or claim genome-wide specificity.

## Default profile

`references/human-refseq-grch38.json` pins:

- Homo sapiens, taxid 9606; assembly GCF_000001405.40 / GRCh38.p14.
- Associated RefSeq annotation release GCF_000001405.40-RS_2025_08.
- Genome FASTA (~928 MB compressed) and associated RefSeq RNA FASTA (~129 MB).
- Assembly report and annotation README.
- Exact compressed-file MD5 values from NCBI's HTTPS checksum listing, retrieved
  2026-09-22. Downloaded file/index SHA-256 values are recorded at installation.

These URLs can change in place. A pinned-checksum mismatch fails rather than adopting
new data under the old release ID. Future profile updates must explicitly select new
content/release identity. Existing installed versions are never overwritten.

**Coverage caveat:** the full assembly includes primary chromosomes, alternate loci,
patches and non-nuclear sequence. Multiple hits on alternate representations must not
be naively counted as independent off-target loci. The assembly report is retained for
future interpretation. Transcript data is assembly-associated RefSeq RNA, not the much
larger all-species `refseq_rna` database. Predicted and curated records retain accession
identifiers; future evidence displays should distinguish them.

Source: NCBI RefSeq / Genome Reference Consortium. Original FASTA headers and provider
metadata are retained. The committed profile contains URLs and hashes, no private data.

## Install

Requires Python 3.11+ and NCBI BLAST+ (`makeblastdb`, `blastdbcmd`, `blastn`) on PATH.
On macOS the user can install BLAST+ with `brew install blast`. The agent does not
request administrator credentials or automatically install system software.

```bash
python3 scripts/manage_references.py plan
python3 scripts/manage_references.py install
python3 scripts/manage_references.py verify
python3 scripts/manage_references.py list
```

On macOS the default store is `~/Library/Application Support/DNAgent/references`.
Elsewhere it is `$XDG_DATA_HOME/dnagent/references`, falling back to
`~/.local/share/dnagent/references`. Set `DNAGENT_REFERENCE_DIR` or use `--root PATH`
for another volume. Installing within the source repository is refused.

Allow approximately 10 GB final storage and **at least 20 GiB free** during installation.
The latter is an enforced conservative preflight minimum, not a measured guarantee of
final index size. The installer checks all tools and space **before downloading**.
BLAST v5's LMDB can require a large virtual address space; that is not equivalent to
physical RAM or on-disk reference size.

## Layout and publication

```text
<root>/human-refseq-grch38/GCF_000001405.40-RS_2025_08/
  manifest.json
  source/       # original FASTA.gz, assembly report and annotation README
  blast/        # genome.* and transcripts.*; keep each database's files together
  logs/         # makeblastdb output and blastdbcmd -info
```

The manifest records the complete pinned profile, source URLs/checksum provenance,
file sizes and SHA-256s, tool versions/paths, actual build arguments, database prefixes,
uncompressed FASTA sizes/hashes, database information and creation time. Paths to
installed content are relative to the release directory. Machine-specific tool paths
occur only in local manifests, never committed configuration.

Files are downloaded and checksummed in a unique `.staging/` directory. Only the current
FASTA is decompressed; it is removed after successful indexing/inspection. Indexing
uses `-dbtype nucl -parse_seqids -blastdb_version 5 -taxid 9606`. Both source and index
hashes are verified before an atomic same-filesystem directory rename publishes the
release. There is no implicit `latest` alias or automatic update.

An exclusive `.install-lock` prevents concurrent installers. Failures leave staging
files and logs for diagnosis, never a ready release. Ordinary errors release the lock;
a killed process can leave a stale lock. Inspect before manually removing stale locks
or failed staging directories. This v1 has no automatic cleanup, resume or retry.

`verify` checks all manifest-listed file sizes and SHA-256s; it does not rerun a
biological search. `list` explicitly reports that integrity has not been checked.
`blastdbcmd -db <release>/blast/genome -entry <accession>` can retrieve sequence without
keeping the decompressed FASTA. The entire reference store is local: only fixed public
reference URLs are requested; no construct or primer queries are uploaded.

## Validation and next step

```bash
python3 scripts/test_manage_references.py
```

Offline tests exercise successful publication, corrupt-file detection, pinned-checksum
mismatch, failed builds, missing prerequisites/space, concurrent-install locks, stream
size limits and manifest traversal rejection. They mock BLAST and downloads; they are
not evidence that a real human index has been built successfully.

After BLAST+ is installed, run the real installer and record its actual disk footprint.
Then implement the optional local specificity adapter: short-primer queries, paired
hit/product interpretation, alternate-locus handling, and reference manifest identity
in the design evidence. Until then, `primer-design` remains supplied-template-only.
