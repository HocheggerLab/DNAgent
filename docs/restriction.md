# Restriction sites: first biological operation

## Scope

This slice detects exact recognition sites and reports nominal cut boundaries.
It does **not** simulate a digest, produce fragment sequences, design cloning
strategies or predict experimental cleavage efficiency.

```bash
cargo build -p dnagent-cli --locked
target/debug/dnagent enzymes --output json
target/debug/dnagent sites construct.dna --enzymes EcoRI,BamHI,BsaI --output json
```

Selection is explicit, case-insensitive and deduplicated. Catalogue output has a
fixed order; sites are ordered by recognition start, enzyme name, then strand.
`enzymes` and `sites` were introduced in the 0.3.0 envelope/schema. Current
responses use 0.9.0; 0.8.0 added [Gibson candidates](gibson.md) after 0.7.0's
[ligation products](ligation.md) and 0.6.0's
[annotation projections](fragment-annotations.md)
after 0.5.0's [end compatibility](compatibility.md) and the separate
[`digest` command](digest.md) introduced in 0.4.0.

## Enzyme catalogues and provenance

DNAgent always has a **built-in set of 99 commercially available enzymes**
(`BUILTIN` in `crates/dnagent-domain/src/restriction.rs`): common cloning and
diagnostic enzymes, Type IIS enzymes for Golden Gate, degenerate sites (e.g. AccI
GTMKAC, ApoI RAATTY), interrupted sites (e.g. BglI GCCNNNNNGGC) and enzymes that cut
outside their site. The names, sites and cut offsets are factual definitions that were
hand-listed and checked against **Biopython 1.85** `Bio.Restriction` (its
REBASE-derived dictionary). No database file or implementation was copied.

A complete **REBASE** catalogue is optional and local. REBASE is
"Copyright (c) Dr. Richard J. Roberts … All rights reserved" and states no
redistribution licence, so DNAgent never commits or bundles it.
`scripts/manage_enzymes.py` downloads one pinned release (609, EMBOSS format),
verifies the pinned SHA-256 values and writes a manifest:

```bash
python3 scripts/manage_enzymes.py plan      # what would be installed, and where
python3 scripts/manage_enzymes.py install   # download, verify, publish
python3 scripts/manage_enzymes.py verify
dnagent enzyme-catalogue                    # which catalogue is active
```

Files go to `~/Library/Application Support/DNAgent/enzymes` on macOS
(`$XDG_DATA_HOME/dnagent/enzymes` elsewhere, or `DNAGENT_ENZYME_DIR`). When a verified
installation exists, the CLI and the desktop app use its commercially available
enzymes. Two-sided cutters and enzymes with unknown cuts are listed as `unsupported`
rather than approximated. An installation that fails verification falls back to the
built-in set with an `enzyme_catalogue_fallback` warning. `DNAGENT_ENZYMES=builtin`
forces the built-in set, which the validation scripts and desktop e2e tests do so
results don't depend on the machine. Please cite REBASE (Roberts et al., *Nucleic
Acids Res.* 2023; 51:D629–D630) for results that depend on it.

Offsets are base-boundary distances from the recognition-oriented sequence start;
negative values cut upstream of the site. Biopython uses `fst5` for the top offset and
`len(site) + fst3` for the bottom. EMBOSS cut numbers are converted with
`offset = c + 1` for `c < 0`, else `c` (EMBOSS has no position 0).

## Coordinate semantics

- Recognition locations use the existing zero-based region representation.
- All reported cut positions are **boundaries along the stored forward sequence**,
  including cuts on the complementary strand. The input sequence is not rotated
  or reverse-complemented in the artifact.
- For a forward site at `s`, cuts are `s + top_offset`, `s + bottom_offset`.
- For a reverse site of recognition length `m`, cuts are
  `s + m - bottom_offset`, `s + m - top_offset` respectively.
- A linear cut boundary lies in `0..=length`. Out-of-bounds cuts are `null`;
  the recognition site is still reported, with `cleavage_available: false` and
  a `restriction_cut_out_of_bounds` warning. Strict mode rejects that result.
- Circular cuts are normalised modulo molecule length. Origin-spanning
  recognition sites are retained as one circular arc, not split or duplicated.
- Palindromic sites appear once, canonically on the forward strand.
- `overhang_polarity` and `overhang_length` describe the enzyme's nominal
  geometry, not an experimentally demonstrated end or a fragment-end sequence.
  `cleavage_available` only says both cut coordinates exist on the substrate.

For example, reverse-oriented BsaI at position 5 cuts at forward-coordinate
boundaries 0 and 4. It must not be modelled by simply adding 7 and 11 to position 5.

## Deliberate limitations

- Input is assumed to represent double-stranded DNA.
- Any ambiguous base in the *input* causes an explicit error; we do not report an
  uncertain sequence as having no sites. Degenerate and interrupted enzyme *motifs*
  are matched by IUPAC code on both strands.
- Very short circular molecules are rejected if shorter than a selected enzyme's
  recognition/cleavage span. Multi-turn recognition or cleavage is not modelled.
- Methylation, star activity, accessibility, buffer/temperature dependence and
  required flanking DNA are **not** modelled. In-bounds cuts at linear ends are
  geometric positions, not proof that an enzyme can cleave that substrate.
- Imported metadata warnings remain visible. `--strict` rejects both import
  warnings and out-of-bounds cleavage warnings before emitting a result.
- Unsupported enzymes/ambiguous input/short circles return JSON error code
  `restriction_scan_failed`. CLI syntax errors still use Clap diagnostics.
- Two-sided cutters (e.g. BaeI, CspCI) are not modelled yet; REBASE lists them as
  `unsupported`.

## Validation

```bash
cargo test --workspace --locked
uv run scripts/check_cli_schema.py --binary target/debug/dnagent
uv run scripts/check_restriction.py --binary target/debug/dnagent
uv run scripts/check_enzymes.py --binary target/debug/dnagent
# Optional: append --manifest "$PRIVATE_CORPUS/manifest.json" to check_restriction.py
```

`check_enzymes.py` compares every built-in enzyme (and, if installed, every REBASE
enzyme Biopython also knows with the same site) with Biopython 1.85: overhang length and
polarity, and top-strand cuts on a synthetic sequence carrying every site in both
orientations, linear and circular. DNAgent's cuts must equal an independent brute-force
scan exactly. Differences from Biopython's `search` are allowed only where they are
explained and confirmed by brute force: Biopython omits a nominal cut exactly at a linear
end, and it misses a degenerate site's reverse match at a start that also matches forward
(regex alternation, e.g. MspJI CNNR at CAAG). For enzymes that cut upstream of their site,
Biopython's `ovhg` is one too large (TspRI leaves a 9-nt 3′ overhang per REBASE,
Biopython says 10), so only their cut positions are compared.

Rust tests use hand-checked cut geometry, both Type IIS orientations, circular
origin wrapping, missing flanks, selection errors and ambiguous input. Three
synthetic `.dna` fixtures are generated by the existing public fixture script.

`check_restriction.py` verifies all built-in catalogue definitions and
forward-coordinate top-strand cut positions across both orientations, linear
substrates and every rotation of synthetic circles (566 synthetic scans). Ample
linear flanks avoid conflating geometric boundaries with library-specific
terminal-cut conventions. Both-strand cut positions are covered by the Rust
expectations; Biopython's public `search` comparison checks top-strand cuts only.
Every comparison response is also JSON-Schema validated. Optional private scans
check source hashes before processing and print no sequences.

## Digest simulation

The separate [`digest` command](digest.md) now partitions supported substrates at
validated cut pairs, reports both strand sequences and ends, and checks against
independent reference products. `sites` remains a recognition/geometry projection:
it never silently runs a digest or assumes that reported cuts work experimentally.

## Desktop display

The desktop app shows sites for an **enzyme set** chosen in the toolbar (remembered
between sessions):

| Set | Enzymes shown |
| --- | --- |
| Unique 6+ cutters (default) | exactly one recognition site, and a site of at least 6 specified bases (N not counted; BglI GCCNNNNNGGC counts 6) |
| Unique & dual 6+ cutters | one or two sites, 6+ specified bases |
| All unique cutters | exactly one site, any length |
| Common cloning enzymes | a fixed list of 32 everyday enzymes (`COMMON_ENZYMES` in `desktop/src/enzymes.ts`) that have a site |
| Chosen enzymes | ticked in **Choose…** (filterable, with site counts), with a site |

Counts, sites and fragments come from the engine (`enzyme_counts`, `find_sites` and
`digest` desktop commands over the active catalogue). The frontend only picks names.
An enzyme whose site cannot be scanned on a very short circle counts zero sites.

- **Map:** a tick at each top-strand cut, labelled "EcoRI, ApoI (396)" with every enzyme
  cutting there (cut positions are zero-based boundaries, as in `dnagent sites`). Site
  labels share placement with feature labels but rank below them. Labels that don't fit
  are counted in the map notice (the ticks are still drawn; hover the notice for their
  positions).
- **Sequence:** a *sites* row with one box per recognition site (stacked when they
  overlap, split across the origin) and red cut marks before the base after each cut,
  on both strands. A cut at a linear molecule's end has no base after it and is not marked.
- **Click** a site label or box to select its recognition sequence. The selection works
  like a dragged range: translate it, or add a feature with **New feature…**.
- **Digest** (below the feature list) digests with the enzymes shown, lists fragments
  longest first with their ends (enzyme, blunt or 5′/3′ overhang, or the molecule end),
  and selects a fragment when you click it. The list stays, labelled with its enzymes,
  until you digest again.

The e2e scenarios `enzymes-unique-sites`, `enzymes-digest-linear` and
`enzymes-origin-site` check all of this against `dnagent enzymes`, `sites` and `digest`.
