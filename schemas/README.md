# CLI envelope version policy

**Current emitted version: 0.5.0.** Every current JSON command emits that version.
The CLI does not negotiate older versions and does not accept JSON envelopes as
input: file-reading commands take SnapGene `.dna` files. The schemas describe
outputs, not an import API.

| Schema | Status | Purpose |
| --- | --- | --- |
| 0.3.0 | Retired; not emitted by current CLI | Historical restriction-site/annotation responses |
| 0.4.0 | Retired; not emitted by current CLI | Historical complete-digest responses |
| 0.5.0 | Current | Adds restriction-end compatibility |

Retired schemas remain unchanged so archived responses can still be validated
against their declared version. Their presence does not imply current runtime
support. Consumers should inspect `schema_version`, select the matching schema,
and explicitly reject unsupported versions rather than assume compatibility.

0.4.0 added `digest`/`digest_failed`; 0.5.0 added
`compatible-ends`/`compatibility_failed`. Existing command result shapes were
preserved across those additions, but envelope version labels changed globally.
New labels require deliberate consumer support even when a particular command's
payload is unchanged.

These schemas validate structure and basic value constraints. Coordinate bounds,
sequence conservation, end compatibility and other relational/biological rules
remain domain checks. CLI argument-parsing errors still use Clap diagnostics
rather than JSON envelopes.
