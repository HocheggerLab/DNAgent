# CLI envelope version policy

**Current emitted version: 0.8.0.** Every current JSON command emits that version.
The CLI does not negotiate older versions and does not accept JSON envelopes as
input. Envelope schemas describe outputs, not an import API. Source constructs
are read from SnapGene `.dna` files. The new `ligate` command accepts a distinct
[version-1 plan input schema](ligation-plan-1.schema.json); those plans are not
CLI output envelopes. `gibson` accepts a separate
[version-1 PCR-tail plan](gibson-plan-1.schema.json).

| Schema | Status | Purpose |
| --- | --- | --- |
| 0.3.0 | Retired; not emitted by current CLI | Historical restriction-site/annotation responses |
| 0.4.0 | Retired; not emitted by current CLI | Historical complete-digest responses |
| 0.5.0 | Retired; not emitted by current CLI | Historical restriction-end compatibility responses |
| 0.6.0 | Retired; not emitted by current CLI | Historical fragment annotation/export responses |
| 0.7.0 | Retired; not emitted by current CLI | Historical restriction/ligation responses |
| 0.8.0 | Current | Adds PCR-tail Gibson candidates and predicted products |

Retired schemas remain unchanged so archived responses can still be validated
against their declared version. Their presence does not imply current runtime
support. Consumers should inspect `schema_version`, select the matching schema,
and explicitly reject unsupported versions rather than assume compatibility.

0.4.0 added `digest`/`digest_failed`; 0.5.0 added
`compatible-ends`/`compatibility_failed`; 0.6.0 added `fragments`/`annotation_failed`;
0.7.0 added `ligate`/`ligation_failed`; 0.8.0 adds `gibson`/`gibson_failed`.
Existing command result shapes were
preserved across those additions, but envelope version labels changed globally.
New labels require deliberate consumer support even when a particular command's
payload is unchanged.

These schemas validate structure and basic value constraints. Coordinate bounds,
sequence conservation, end compatibility and other relational/biological rules
remain domain checks. CLI argument-parsing errors still use Clap diagnostics
rather than JSON envelopes.
