# GUI test harness plan

Status: implemented 2026-09-28 (see `desktop/e2e/README.md` for the current contract). Branch: `gui-harness` (from `main`, fast-forwarded to
the former `feat/offline-primer-design`).

## Findings that shape the plan

- `main.ts` holds state in module variables (`current`, `selected`, `activeTab`,
  `revision`). Only the **active** tab is rendered; the hidden panel keeps stale DOM.
- `invoke` is imported directly in `main.ts`; the dialog plugin's `open` is too.
- The Map highlight is one `polyline` per source part. The Sequence highlight is
  `<mark>` bases (from `contains`) plus `.feature-track.selected` buttons clipped per
  60-base row (`rowSpans`).
- The CLI `location.parts` shape differs from the DTO: `linear {start,end}` vs
  `circular_arc {start,length}`, while the DTO `Segment` is always `{start,length}`.
- **No public fixture has overlapping features** (checked every `synthetic_*.dna`).
  See "New fixture" below.
- Warnings are one `<pre>` of newline-joined text, so the rendered count is not
  directly observable.

## main.ts refactoring: small, no restructuring

No significant restructuring is needed. Changes:

1. Replace `invoke`/`open` imports with `openDocument(path)` / `pickConstructPath()`
   from a new `src/ipc.ts`.
2. Add stable `data-testid` attributes, plus `data-feature-id` on list buttons and
   `data-part-index` on map highlight polylines and sequence track buttons.
   Feature-list names go in a child `<span data-testid="feature-name">` so the
   *displayed* text can be read. Each warning becomes its own
   `<span data-testid="warning-item">` line inside the same `<pre>`. Neither change
   is visible to users.
3. One guarded block at the end of `main.ts`:
   ```ts
   if (import.meta.env.MODE === 'e2e') void import('./testing/automation').then(m => m.install(() => ({ current, selected, activeTab })));
   ```
   Vite replaces `import.meta.env.MODE` statically, so production builds drop the
   branch and the dynamic chunk. The check is a `grep` of `dist/` for
   `__DNAGENT_TEST__`, `recordings` and fixture names, run as part of `npm run e2e`
   (a `pretest` style script) as well as by hand.

The read-only accessor exposes the model but cannot mutate it. Everything else is
read from the DOM.

## IPC seam and recordings

- `src/ipc.ts` is the only module that imports `@tauri-apps/api/core` and the dialog
  plugin. It exports `openDocument`, `pickConstructPath` and `pendingRequests()`, a
  counter used for the idle flag. In `e2e` mode, it forwards to
  `src/testing/stub-backend.ts`, which is dynamically imported and absent from
  production.
- The stub answers `open_document` from `desktop/e2e/fixtures/recordings.json`,
  keyed by repo-relative fixture path. It returns `Ok(Document)` or
  `Err(Diagnostic)` exactly as serialised by Rust. An unknown path rejects with
  `e2e_recording_missing` so it fails loudly and is never silently empty.
  `setDelay(path, ms)` is a one-shot delay for the stale-request scenario.
  `pickConstructPath` returns a path queued by the test, so the Browse button can be
  covered too.
- The new example `cargo run -p dnagent-desktop-api --example export_recordings >
  desktop/e2e/fixtures/recordings.json` calls the real
  `dnagent_desktop_api::open_document` for every public `fixtures/formats/snapgene/*.dna`
  (valid and invalid) in sorted order and prints pretty JSON. `serde_json` is added
  to the crate's dependencies from the workspace.
- A drift test, `e2e_recordings_are_current`, sits next to `generated_contract_is_current`
  and compares `recordings_json()` with `include_str!` of the committed file. Error
  messages containing absolute paths are checked for determinism first; any that
  aren't deterministic are excluded from the recordings and documented.

## Automation API (`window.__DNAGENT_TEST__`, e2e mode only)

Each command drives real user events:

| Command | Implementation |
| --- | --- |
| `open(path, {delayMs?})` | Sets `#path` value, then `form.requestSubmit()`. Calls the real `load` via the submit handler. |
| `browse(path)` | Queues the picker result, then clicks `#browse`. |
| `selectFeature(id)` | `.click()` on `[data-testid=feature-item][data-feature-id=id]`. Throws if absent. |
| `selectTab(name)` | `.click()` on `#tab-<name>`. |
| `clickSequenceBase(i)` | Requires the Sequence tab. Clicks the forward-strand `[data-position=i]`, bubbling to the real `#sequence` handler. Throws if absent. |
| `getState()` | JSON snapshot (below). |

Scenarios can also use Playwright's real `click: {testid, index}` (trusted events),
which go through the browser input pipeline rather than `element.click()`.

### State snapshot

```jsonc
{
  "idle": true,                    // ipc.pendingRequests() === 0
  "status": "Read-only · …",       // #status text
  "active_tab": "map",             // model; also checked against aria-selected
  "document": null | { "name": "…", "length": 15, "topology": "linear",
                       "title": "<#title text>" },
  "features": [ { "id": "feature-0001", "name": "<displayed span>",
                  "strand_text": "reverse", "selected": false } ],   // DOM order
  "selection": {
    "feature_id": null | "feature-0001",          // model
    "map": null | { "parts": [ {"start":10,"length":4}, … ] },   // DOM, rendered view only
    "sequence": null | {
      "parts": [ … ],               // rebuilt from selected track spans grouped by data-part-index
      "highlighted_positions": [ … ] // sorted data-position of forward-strand <mark>s
    }
  },
  "warnings": { "count_shown": 6, "items": 6, "codes": ["…"], "open": false } | null,
  "primers": { "count": 1 }
}
```

- Only the visible view reports a selection; the hidden view reports `null`, so
  stale DOM can't be read by mistake.
- Map `parts` come from `data-part-start/length` attributes written in the same
  loop that draws each highlight polyline. This catches dropped, reordered or
  shifted parts, but not geometry errors inside `point()` (screenshots cover that;
  noted as a gap).
- Sequence `parts` are rebuilt from the DOM: for each `data-part-index`, the
  covered reference positions (`rowStart + span column`) are collected and
  converted back into one circular interval `{start,length}`. Discontinuous
  coverage is reported as an error value. This is coordinate bookkeeping in test-only
  code over rendered output, not a biological calculation in the app, and it catches
  `rowSpans` clipping bugs and dropped wrap pieces.

## Expectation resolution: run the CLI at test time

Playwright `globalSetup` runs `target/debug/dnagent <command> <fixture> --output json`
(override with `DNAGENT_BINARY`) once per distinct (command, fixture) pair and
caches the results. If the binary is missing, it fails with the `cargo build`
command to run.

Why this approach rather than precomputing:

- The CLI is the independently validated contract (schema checks, private corpus).
  The recordings come from `dnagent-desktop-api`, so comparing the GUI with the
  CLI also cross-checks the desktop DTO projection against the engine. If the
  expectations were precomputed from the same example, the test would compare
  the desktop API with itself.
- It adds no second committed snapshot that could drift. It matches the existing
  `scripts/check_*.py --binary` pattern.

The JSON path language is a small, documented subset: `a.b`, `[n]`, `[*]`,
`[?(@.key=='value')]`. It's implemented in `e2e/lib/jsonpath.ts` with unit tests.
A path that matches nothing is an error, never an empty pass. Filters that must
select exactly one element use `one:` (e.g. `one: "result[?(@.label=='x')]"`).

Transforms are pure format adapters, applied only in test code:

- `parts`: CLI `location` → `[{start,length}]` in source order. For `linear`,
  `length = end - start`; `circular_arc` is copied. No flattening.
- `positions`: parts → sorted set of covered bases (mod length) for comparison with
  `highlighted_positions`. Order-free by design, because source order is checked
  via `parts`.
- `count`, `codes` (`[*].code`).

## Scenario format (JSON, validated with Ajv)

JSON is used rather than YAML: no YAML parser dependency or ambiguity, and it matches
the repo's JSON Schema practice. The schema is `desktop/e2e/scenario.schema.json`.

```json
{
  "id": "multipart-origin",
  "description": "…",
  "fixture": "fixtures/formats/snapgene/synthetic_multipart_origin.dna",
  "steps": [
    {"open": {}},
    {"select_feature": {"label": "multipart wrap"}},
    {"expect": [{"state": "selection.map.parts",
                 "equals_cli": {"command": "features", "one": "result[?(@.label=='multipart wrap')].location", "transform": "parts"}}],
     "screenshot": true},
    {"select_tab": "sequence"},
    {"expect": [ … "selection.sequence.parts" …, … "highlighted_positions" with "positions" … ]}
  ]
}
```

Step vocabulary: `open {fixture?, delay_ms?, wait?}`, `browse {fixture?}`,
`select_feature {id | label}`, `select_tab`, `click_sequence_base`,
`click {testid, index}`, `wait_idle`, `expect [...]`, plus `screenshot: true` on any
step. Labels are resolved to ids through CLI `features` and must match exactly one.

Assertion forms: `equals` (literal), `equals_cli`, and `equals_state` (compare two
state paths, e.g. map parts == sequence parts). **The validator rejects literal
`equals` on biological paths** (`document.length|topology|name`,
`features*`, `selection.*.parts`, `highlighted_positions`, `warnings.count*/codes`),
so coordinates cannot be hand-written. Literals are allowed for UI state such as the tab,
`idle`, panel `open` and `feature_id: null`. Unknown keys fail. A scenario with zero
`expect` steps fails.

A single `e2e/scenarios.spec.ts` validates every file first (one malformed file fails
the run with the file, JSON pointer and message), then creates one test per scenario.
It takes a full-page screenshot at the end, plus one for each `screenshot: true` step,
saved to `desktop/e2e/artifacts/<id>/…png` (gitignored). Waiting is
`page.waitForFunction(() => __DNAGENT_TEST__.getState().idle)`, never sleeps.

## Playwright

`@playwright/test` and `ajv` are dev dependencies, with Chromium only and headless.
`webServer` runs `vite --mode e2e --port 1421 --strictPort`. Scripts:
`npm run e2e`, `npm run e2e:headed`, and `npm run e2e:check-prod`, which builds and
greps `dist/`. Installing needs network access for npm and the Chromium download
(`npx playwright install chromium`).

## Scenarios

1. **open-linear**: `synthetic_linear.dna`. Checks feature names and order against
   `features[*].label`, the count, and length and topology against `inspect`.
2. **list-selection-links-views**: `synthetic_linear.dna` (a reverse two-part
   feature). Selects it from the list. Map parts must equal the CLI parts. Switch to
   Sequence: sequence parts must equal the CLI parts, positions must equal the
   CLI positions, and the selected feature must still be the same one. Switch back to Map and check again.
3. **multipart-origin**: `synthetic_multipart_origin.dna`. Both parts (wrap
   `[10,+4)` then `[4,6)`) must appear in source order in both views, and the positions
   must include `0,1,10,11`.
4. **sequence-click-cycles-overlaps**: needs a **new fixture** (below). Clicking one
   base repeatedly must visit each covering feature id in list order and then wrap
   around. The expected cycle order comes from CLI `features` filtered to the
   features whose CLI parts cover the base, computed in test code with the
   `positions` transform.
5. **warnings-panel**: `synthetic_partial.dna` (6 warnings). The shown count, item
   count and codes must match CLI `features.warnings`. The panel starts closed; a
   real `click` on the summary opens it. A second part opens
   `synthetic_unannotated.dna` and checks that `warnings` equals the CLI (empty,
   so `null`/0).
6. **stale-request**: open `synthetic_multipart_origin.dna` with `delay_ms: 400,
   wait: false`, then `synthetic_linear.dna`, select a feature, then `wait_idle`
   (waits for A to finish). The document must match the CLI `inspect` for B,
   `feature_id` must be unchanged, and the parts must be B's.
7. (If cheap) **browse-picker**: Browse returns a queued path. The document is loaded
   and `#path` is filled in.

## New fixture (needs your approval)

`synthetic_overlaps.dna`, added to `generate_cases.py`: a linear 30-base
synthetic sequence with three hand-authored annotations covering one shared base:
a forward single-part feature, a reverse feature, and a two-part feature whose
second part covers the base. There are no lab sequences. Provenance: a new row in the
fixtures README (hand-authored, MIT, generated by `generate_cases.py`). I will check
that existing glob-based checks (`check_cli_schema.py` globs only `*.json`) and
fixture tests are unaffected, and add an adapter assertion in `fixture_cases.rs`
in the existing style. Existing fixture bytes must not change: I'll confirm that
the regenerated files are byte-identical.

## Failure injection (after the scenarios pass, then reverted)

Planned bugs: (a) an off-by-one in `contains` (sequence marks); (b) dropping
`parts[1]` in map highlighting; (c) `rowSpans` losing the wrap piece; (d) not
discarding stale responses (`request !== revision` removed); (e) cycling that
doesn't advance; (f) showing `warnings.length - 1` in the summary; (g) a feature
list that drops the last item. The report will list which scenario caught each
one, and I'll strengthen the assertions for any that slip through.

## Out of scope / known gaps (to be documented)

Chromium versus WKWebView/WebKitGTK; the native shell and real `invoke`
serialisation; the native dialog; map geometry correctness beyond per-part data;
visual baselines.

## Files touched

New: `desktop/src/ipc.ts`, `desktop/src/testing/{automation,stub-backend}.ts`,
`desktop/e2e/**`, `desktop/playwright.config.ts`,
`crates/dnagent-desktop-api/examples/export_recordings.rs`, and this plan.
Edited: `desktop/src/main.ts` and `desktop/src/sequence-view.ts` (seam, test ids,
part index), `desktop/package.json`, `tsconfig.json`, `.gitignore`, desktop-api
`lib.rs` and `Cargo.toml`, `generate_cases.py` plus the fixture README (if
approved), `AGENTS.md`, `desktop/README.md`, `desktop/e2e/README.md`, and the
docs note in `desktop-architecture.md`.
