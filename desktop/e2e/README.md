# Desktop GUI end-to-end harness

Declarative scenarios drive the real frontend (`src/main.ts`) in headless Chromium,
read a structured state snapshot, and compare it with the **dnagent CLI as ground
truth**. The GUI is never the source of truth: no expected coordinate, name, count
or warning is hand-written, and no backend response is hand-mocked.

## Run

```bash
# From desktop/ (first time: npm ci && npx playwright install chromium)
npm run e2e          # prod-bundle check, cargo build of the CLI, then all scenarios headless
npm run e2e:headed   # same scenarios in a visible browser, for humans
npm run e2e:check-prod  # only: prove the production bundle has no test code
npm test             # unit tests, including the harness's path/transform/validator tests
npm run review       # design-review gallery (see below); add local .dna paths to include them
```

Exit status is nonzero on any failure. Screenshots go to `e2e/artifacts/<scenario-id>/`
(`final.png`, `NN-<action>.png` for `screenshot: true` steps, and
`failure-step-NN.png`), and Playwright traces go to `e2e/artifacts/test-results/`. The
directory is gitignored. There are no visual baselines yet, so screenshots are for review.

Set `DNAGENT_BINARY` to use a different CLI binary. The default is
`../target/debug/dnagent`, which `npm run e2e` rebuilds.

## How it works

```text
scenario JSON ──validate──► runner (Node/Playwright) ──page.evaluate──► window.__DNAGENT_TEST__
     │                           │                                       │ dispatches real DOM events
     │                           │                                       ▼
     │                           │                              main.ts handlers ─► src/ipc.ts
     │                           │                                                  │ (e2e mode)
     │                           ▼                                                  ▼
     └── equals_cli ──► target/debug/dnagent … --output json      stub-backend ─► /__dnagent (Vite proxy)
                         (ground truth, run at test time)                          │
                                                                   e2e_server (real Rust desktop session)
```

- **Vite `e2e` mode** (`vite --mode e2e`, port 1421) is the only way to get the
  automation API and stub backend. Both sit behind `import.meta.env.MODE === 'e2e'`,
  which Vite resolves at build time. `npm run e2e:check-prod` fails if their markers
  appear in `dist/`, and checks that the same markers *are* present in an e2e build,
  so the check can't pass vacuously.
- **A live Rust test server** answers every desktop command: Playwright starts
  `cargo run -p dnagent-desktop-api --example e2e_server -- --port 1431` from the
  repository root, and Vite (e2e mode) proxies `/__dnagent` to it. It runs the same
  `Session` as the Tauri shell (open, preview, add/remove, undo/redo, save), with one
  session per page load, so edits and saves are real. There are no recorded responses.
  Relative fixture paths resolve from the repository root, and saves must go under
  `desktop/e2e/artifacts/`.
- **Expectations come from the CLI at test time.** The desktop API and CLI are
  separate projections of the same engine, so the scenarios also cross-check the
  desktop DTOs against the CLI contract. This follows the existing
  `scripts/check_*.py --binary` convention, and there are no committed expected snapshots.

## Automation API (`window.__DNAGENT_TEST__`)

Every command dispatches the same event a user would. None of them sets app state directly.

| Command | Effect |
| --- | --- |
| `open(path, {delayMs?})` | Fills the path field and submits the form. `delayMs` delays that one response. |
| `browse(path \| null)` | Queues the picker answer and clicks **Browse…**. |
| `selectFeature(id)` | Clicks that feature-list button. |
| `selectTab('map' \| 'sequence')` | Clicks the tab. |
| `clickSequenceBase(i)` | Clicks forward-strand base `i` (Sequence tab only). |
| `clickSite(enzyme)` | Clicks the enzyme's map label or first sequence site box. |
| `chooseEnzymes(names)` | Opens **Choose…**, ticks exactly `names`, confirms. |
| `toggleDetection(name)` / `selectDetection(name)` | Clicks a **Detected** row's tick box / label. |
| `getState()` | JSON snapshot (below). |

`getState()` fields (see `AppState` in `src/testing/automation.ts`):

- `idle`: no IPC request is pending. Wait on this instead of sleeping.
- `status`, `path_input`, `active_tab` (`'inconsistent'` if the model, `aria-selected`
  and the visible panel disagree), `visible_panels`.
- `document`: `{name, length, topology, title}`, **parsed from the rendered title**;
  `null` before a load.
- `features`: the displayed list in DOM order, as `{id, name, selected, text}`.
- `selection.feature_id` comes from the model. `selection.map` and `selection.sequence`
  are only reported for the **visible** view; the hidden view is `null`.
  - `map.parts`: one `{start,length}` per selection-band part, in draw order.
    `map.active_labels`: the visible label text.
  - `sequence.parts`: rebuilt from the selected track spans, grouped by source-part
    index, into one circular interval each, in source order. Discontinuous coverage
    becomes `{error, positions}`. `highlighted_positions` and
    `complement_highlighted_positions` are the sorted `<mark>` positions on each strand.
- `map` (Map tab only, else `null`): canvas `width`/`height`/`radius`; `fills_panel` (the map
  fills its panel) and `layout_current` (the drawing was computed for the current canvas
  size); `drawn_ids` (features with at least one drawn part, drawing order = source order);
  `labels` (`{id, mode: inline|outside}`); `unlabelled_ids`/`_names`/`_count` (drawn but no
  label); `badged_ids` (list items badged "not labelled on map"); `notice_count` (number
  in the notice, 0 when hidden); `accounted_ids` (features with exactly one label, or none
  plus a badge); `overlapping_labels`, `labels_outside_viewport`, `labels_under_notice`
  (bounding-box checks on the rendered labels).
- `options`: toolbar state as displayed, `{amino_acids: one|three, show_frames, show_orfs, orf_min_codons}`.
- `selection.kind` (`feature|orf|range|null`), `selection.orf_id`, `selection.range`
  (`{start, end}`, half-open), `selection.range_translation` (`{strand, protein}` as displayed).
- `selection.sequence` also carries:
  - `translations`: one entry per CDS amino-acid row, in source order, with
    `{feature_id, protein, codon_count, middles, warned}`. `protein` is the displayed
    letters in codon order (3-letter names joined by spaces in 3-letter mode). `middles`
    is each letter's reference column. `warned` is the ⚠ marker.
  - `frames`: `{frame: "+1"…"-3", protein, middles}`.
  - `orf_regions`: `{strand, start, length}` rebuilt from the ORF tracks.
- `map.orf_regions` (from the drawn arcs) and `map.undrawn_orfs` (the count in the notice).
- `layout.sequence_columns` and `layout.sequence_fits_width` (Sequence tab only).
- `tabs`: `[{document_id, name, path, dirty, active, shown_dirty}]` and `tab_count` (tabs in the bar).
- `workspace`: `{path, shown, notices: [{kind: new|changed|conflict, path}], handoff: {open, prompt, context_path}}`.
- `edit`: `{revision, can_undo, can_redo, dirty, added_feature_ids, saved_path, shown:
  {dirty, undo, redo, new_feature, delete_feature}}`. `shown` is what the controls
  display; the rest is the session's state.
- `dialog`: `{open, summary, protein, warnings (codes), error, inputs: {label, kind,
  strand, translate}}`, i.e. the New-feature dialog's engine preview and its inputs.
- `features[*].strand` (as displayed) and `features[*].added` (the "added" badge).
- `theme`: `{preference: system|light|dark, resolved}` — `resolved` is the theme actually
  applied to the page. `layout.feature_list_collapsed`.
- `features[*].unlabelled`: the item carries the "not labelled on map" badge.
- `warnings`: `{present, count_shown, items, codes, open}`; all counts are 0 when no panel is rendered.
- `primers`: `{count, summary}`.
- `enzymes`: `set` (model) and `set_shown` (the toolbar control); `catalogue` `{source,
  count, names}`; `shown`, the enzymes the app chose to show (null while engine results
  are pending); `map` (Map tab only): `ticks` (cut positions), `labels` (`{cut, names}`
  **parsed from the label text**), `unlabelled` (count in the notice) and `accounted`
  (every tick has exactly one label, or is counted); `sequence` (Sequence tab only):
  `sites` (`{enzyme, start, length}` rebuilt from the drawn boxes' columns) and `cuts`
  (`{top, bottom}` positions carrying a cut mark); `digest`: `{summary, fragments:
  [{start, length}] by start, enzymes}`. Site labels count in the map's overlap,
  viewport and notice checks, and `map.notice_count` counts features only.
- `confirm`: `{open, title, message, ok}` of the in-app confirmation dialog (delete a
  feature, close a tab with unsaved changes). Answer it with `click` on `confirm-ok` /
  `confirm-cancel`, or `press: Escape`.
- `detect` (null while the Detected panel is closed): `available`, `summary`,
  `add_label`, `rows` (from the rows' data and badges), `checked` (`{library_id,
  start}` of ticked rows), `new_names`, `unticked_new_names`, and on the Map tab `map` /
  `map_checked` (the drawn proposal arcs).

All coordinates are zero-based and half-open, matching the JSON contract.

## Scenario format

Files live in `e2e/scenarios/<id>.json`, and `id` must match the file name. They are
validated against `e2e/scenario.schema.json` plus semantic rules in
`e2e/lib/scenario.ts`. An invalid file fails as its own test with JSON-pointer
messages; it is never skipped.

```json
{
  "$schema": "../scenario.schema.json",
  "id": "my-scenario",
  "description": "What behaviour this protects",
  "fixture": "fixtures/formats/snapgene/synthetic_linear.dna",
  "steps": [
    {"open": {}},
    {"select_feature": {"label": "reverse multipart"}, "screenshot": true},
    {"expect": [
      {"state": "selection.map.parts",
       "equals_cli": {"command": "features", "one": "result[?(@.label=='reverse multipart')].location", "transform": "parts"},
       "message": "optional context shown on failure"}
    ]}
  ]
}
```

### Steps

Each step has exactly one action, plus optional `screenshot: true` and `note`.

| Step | Meaning |
| --- | --- |
| `open: {fixture?, delay_ms?, wait?, expect_error?}` | Open the scenario fixture, or another one. By default it waits for idle and fails if the status says "Open failed" (inverted by `expect_error`). `wait: false` returns immediately. |
| `browse: {fixture?}` | Open through the Browse… button with a queued picker result. |
| `select_feature: {id} \| {label}` | A label is resolved to an id through CLI `features` and must match exactly one feature. Waits until that list item is rendered. |
| `select_tab: "map" \| "sequence"` | |
| `click_sequence_base: n` | |
| `click: {testid, index?}` | A real Playwright pointer click on the nth `data-testid` element. |
| `wait_idle: true` | Wait until no request is pending. |
| `set_viewport: {width, height}` | Resize the browser window, then wait two animation frames for re-layout. |
| `set_color_scheme: "light" \| "dark"` | Emulate the OS appearance (`prefers-color-scheme`). |
| `select_option: {testid, value}` | Choose an option in a real `<select>`, e.g. `theme-select`. |
| `reload: true` | Reload the page: local preferences survive, the open document does not. |
| `select_orf: {id}` | Click that ORF's track (Sequence) or arc (Map); ORFs must be shown. |
| `drag_bases: {from, to}` | Real mouse drag across forward-strand bases; the range is half-open and includes both ends. |
| `fill: {testid, value}` | Type into an input (e.g. `feature-label`); waits for the debounced engine preview. |
| `press: "Meta+z"` | Keyboard shortcut (Playwright key syntax). |
| `save_as: {path}` | Queue the save dialog's answer (under `desktop/e2e/artifacts/`) and click **Save as…**; later `equals_cli.saved` and `open.saved` use that file. |
| `remember: {state, as, single?}` | Store a state value (e.g. the dialog's preview protein) for a later `equals_memory`. |

| `set_workspace: path` | Choose the workspace through **Workspace…** (a folder under `desktop/e2e/artifacts/`, emptied first). |
| `poll_workspace: true` | Run the app's workspace poll now (the function its timer runs). |
| `run_cli: {args}` | Play the agent: run the real `dnagent` CLI from the repository root; it must succeed. |
| `click_site: {enzyme}` | Click the enzyme's site label (Map) or first site box (Sequence). |
| `choose_enzymes: {names}` | Open **Choose…**, tick exactly these enzymes and confirm. |
| `toggle_detection: {name}` | Click the tick box of the first **Detected** row with this name. |
| `select_detection: {name}` | Click the first **Detected** row with this name (selects its span). |

`select_feature` also takes `extend: true` (shift-click), `click` takes
`modifiers: ["Shift"]`, and `open` takes `saved: true` or `file: <path under
desktop/e2e/artifacts>`. CLI defaults and label lookups follow the **active tab's** file
when it is repo-relative. Only committed `fixtures/` results are cached; files that change
during a scenario are re-run every time.
| `expect: [...]` | Assertions against one `getState()` snapshot; every failure in the step is reported. |

Selection, tab and base-click steps don't wait for idle: they issue no request and
render synchronously. That way, a stale request can still be pending at the next step.

Useful `data-testid`s: `feature-item`, `feature-name`, `tab-map`, `tab-sequence`,
`map-feature`, `map-selection-part`, `sequence-track`, `warnings-summary`,
`warnings-panel`, `warning-item`, `primer-item`, `path-input`, `open-form`, `browse`,
`map-label` (pills and on-arc labels), `map-hidden-notice`, `feature-unlabelled`,
`toggle-features`, `theme-select`, `enzyme-set`, `enzyme-choose`, `map-site-tick`,
`map-site-label`, `site-track`, `digest-run`, `digest-fragment`. Click map pills rather than `map-feature` arcs:
Playwright clicks an element's bounding-box centre, which for an arc lies off the shape.

### Map invariants

`map-layout-accounting` and `map-hidden-labels-notice` pin the display contract for
realistic plasmids (public pUC19, `fixtures/formats/snapgene/pUC19_M77789.dna`) at
several window sizes: every feature drawn (`drawn_ids` = CLI ids), every label placed
or reported (`accounted_ids` = CLI ids, `badged_ids` = `unlabelled_ids`, `notice_count`
= `unlabelled_count`), no overlapping, clipped or notice-covered labels, and a layout
that fills and matches the canvas. Literal zeros and booleans are allowed there because
they are geometry checks, not biology. Keep these invariants when redesigning.

### Assertions

`{state: <path>, ...}` with exactly one of:

- `equals_cli: {command, args?, path | one, transform?, index?, fixture?}`: runs
  `dnagent <command> <fixture> <args…> --output json`. `fixture` defaults to the most
  recently opened one. `command` is `inspect`, `features`, `primers`, `translate`,
  `orfs`, `sites`, `digest`, `detect-features` or `enzymes` (no input file). `args` are extra CLI
  arguments, e.g. `["--feature", "feature-0001"]` or `["--min-codons", "30"]`; an
  argument `{"memory": name}` inserts a remembered value (arrays joined with commas),
  e.g. `["--enzymes", {"memory": "shown"}]` for the enzymes the GUI chose to show.
  The CLI, `run_cli` and the e2e server all run with `DNAGENT_ENZYMES=builtin`, so a
  locally installed REBASE does not change results, and with
  `DNAGENT_FEATURE_DB=desktop/e2e/artifacts/feature-library.sqlite`, a feature library
  that global setup builds from the public fixtures (never your own library).
- `equals_memory: <name>`: a value stored by an earlier `remember` step.
- `equals_cli.saved: true` runs the CLI on the file written by the last `save_as`, and
  `equals_cli.file` on any file under `desktop/e2e/artifacts` (snapshots, agent products).
- `equals_json_file: {file, path | one}`: a value from a JSON file the app wrote (e.g.
  `handoff/context.json`).
- `single: true` on an assertion makes a filtered state path match exactly one value
  and compares it unwrapped, e.g. `selection.sequence.frames[?(@.frame=='+1')].protein`.
- `equals_state: <path>`: another path in the same snapshot.
- `equals: <literal>`: **rejected on biological paths** (`document.*`, `features*`,
  `selection.map*`, `selection.sequence*`, `warnings` counts and codes, `primers.count`,
  `enzymes.shown`, `enzymes.catalogue.names|count`, `enzymes.map.ticks|labels`,
  `enzymes.sequence*`, `enzymes.digest.fragments|enzymes`, `detect.rows|checked|new_names|unticked_new_names|map*`).
  `null` is allowed for `document`, `selection.feature_id`, `selection.map` and
  `selection.sequence`. Literals are otherwise for UI state: `idle`, `active_tab`,
  `warnings.open`, `warnings.present`, `path_input`.

Path subset (`e2e/lib/jsonpath.ts`): `a.b`, `[n]`, `[*]`, `[?(@.key=='text')]`,
`[?(@.key==3)]`. `path` returns an array when it contains a wildcard or filter;
otherwise it returns the single value. `one` must match exactly one value. A
path that matches nothing fails the assertion. A wildcard over an empty array gives `[]`.

Transforms (`e2e/lib/transforms.ts`) are format adapters in test code only:

| Transform | Input → output |
| --- | --- |
| `parts` | CLI `location` → `[{start,length}]` in source order (`linear`: `end - start`; `circular_arc` copied). Never flattened or merged. |
| `positions` | CLI `location` → sorted covered bases (circular parts wrap). Molecule length comes from `inspect`. Order-free; source order is checked by `parts`. |
| `count` | array → length |
| `codes` | warnings array → `[code…]` |
| `codon_middles` | codon list (or list of lists) → middle reference base of each codon |
| `orf_regions` | CLI ORFs → `[{strand, start, length}]` sorted like the GUI state |
| `orf_parts` / `orf_positions` | one CLI ORF → `[{start, length}]` / sorted covered bases |
| `lengths` | array of arrays or strings → their lengths |
| `{name: "forward_span", from, to, as?}` | the documented shift-click rule over CLI features (by label): forward from `from`'s first part start to the furthest part end of both, wrapping on circles; `as: "parts"` gives `[{start, length}]`. |
| `{name: "ids_covering", base}` | features array → ids covering `base`, in source order. Use `index` to pick the expected cycle position. |
| `{name: "enzyme_set", set}` | `sites` result over every catalogue enzyme → the display set's enzymes (`unique6`, `unique_dual6`, `unique_any`; rule in `docs/restriction.md`), alphabetical |
| `site_enzymes` | `sites` result → enzymes with at least one site, alphabetical |
| `site_ticks` / `site_labels` | `sites` result → distinct top-strand cuts / `[{cut, names}]` per cut |
| `site_regions` | `sites` result → `[{enzyme, start, length}]` recognition regions |
| `site_cuts` | `sites` result → `{top, bottom}` cut boundaries that have a base after them |
| `recognition_range` | one site → the half-open selection range of its recognition sequence (wrapping on circles) |
| `fragment_parts` | `digest` fragments → top-strand `[{start, length}]` by start |
| `detection_rows` | `detect-features` matches → panel rows `{library_id, name, start, length, strand, annotated, contained}` (contained: inside a longer match, circular-aware) |
| `default_detections` | matches → `[{library_id, start}]` of new, un-nested matches (the default ticks) |
| `detection_spans_new` | matches → `[{start, length}]` of new matches, by start then longer first (the map arcs) |
| `{name: "detection_range", label}` | matches → selection range of the first match with that name |
| `{name: "fragment_range", rank}` | `digest` fragments → selection range of the fragment at `rank` in list order (longest first) |

## Adding a scenario

1. Pick a **public synthetic** fixture. Never use private lab constructs. A new fixture
   goes in `fixtures/formats/snapgene/generate_cases.py`, gets a provenance row in that
   README. The live server reads it directly; nothing needs regenerating.
2. Write `e2e/scenarios/<id>.json`. Get every biological value through `equals_cli`.
   Add `message`s that explain intent.
3. Run `npm test` (schema and semantic validation) and `npm run e2e`.
4. Check that the scenario can fail: temporarily break the behaviour it protects and
   confirm that the failure message is readable, then revert.

## Failure injection (2026-09-28)

These bugs were introduced one at a time and then reverted:

| Injected bug | Caught by |
| --- | --- |
| `contains()` off-by-one | list-selection-links-views, multipart-origin |
| Map highlight drops parts after the first | list-selection-links-views, multipart-origin, stale-request |
| Map sorts parts by start | multipart-origin |
| `rowSpans` loses the origin-wrap piece | multipart-origin |
| Stale responses not discarded | stale-request |
| Sequence click never advances the cycle | sequence-click-cycles-overlaps |
| Warning summary shows `length - 1` | warnings-panel |
| Feature list drops the last feature | all seven scenarios |
| Tab switch doesn't re-render | list-selection-links-views, multipart-origin, sequence-click-cycles-overlaps |
| Tab switch clears the selection | list-selection-links-views, multipart-origin |
| List shows kind instead of label | open-linear, stale-request, warnings-panel |
| Title length off by one | open-linear, stale-request (initially missed; `document` is now parsed from the title) |
| Complement highlights shifted by one | list-selection-links-views, multipart-origin |

Second round, after the map redesign (2026-09-28):

| Injected bug | Caught by |
| --- | --- |
| Features under 10 bp not drawn | map-layout-accounting, map-hidden-labels-notice, multipart-origin, browse-picker |
| Unplaced labels dropped without a report | map-layout-accounting, map-hidden-labels-notice |
| Label spreading disabled (overlaps) | map-layout-accounting, map-hidden-labels-notice |
| Labels not clamped to the canvas | map-layout-accounting |
| Selected label not prioritised | map-layout-accounting |
| No re-layout on resize | map-layout-accounting, map-hidden-labels-notice (via `layout_current`, added for this) |
| Theme choice not persisted | theme-follows-os-and-switch |
| OS appearance changes ignored | theme-follows-os-and-switch (initially missed; `theme.resolved` now reads the applied theme) |
| Notice count off by one | map-layout-accounting, map-hidden-labels-notice |
| Selection band parts sorted by start | multipart-origin |
| Notice collapses the list instead of opening it | map-hidden-labels-notice |

The notice-covering-a-label bug was found in a screenshot, not by a scenario; the
`labels_under_notice` invariant was added and shown to fail before the fix.

Third round, after translation and ORFs (2026-09-29; recordings were still in use then).
Engine bugs changed the CLI and the recordings together, so the GUI suite (which only checks that the GUI agrees with the
CLI) cannot see them. Engine correctness is the job of the domain unit tests and the
Biopython oracle (`scripts/check_translation.py`).

| Injected bug | Caught by |
| --- | --- |
| Reverse join: each part reversed but kept in source order | domain tests, CLI tests, Biopython oracle |
| Ambiguous codon takes its first expansion instead of X | domain tests, CLI tests, oracle |
| Shortest instead of longest ORF per stop | domain tests, oracle |
| No initiator M for alternative starts | domain tests, CLI tests, oracle |
| No origin-wrapping ORFs | domain tests, CLI tests, oracle |
| Amino acid placed under the first codon base | cds-translation-rows |
| Reverse frame letters shifted one base | six-frame-translation |
| ORF minimum off by one | orfs-sequence-and-map |
| Range translated in the wrong frame | range-drag-translation |
| Reverse range read from forward frames | range-drag-translation |
| Imported-translation mismatch marker dropped | cds-translation-rows |
| Map ORFs silently skipped | orfs-sequence-and-map |

Fourth round, after editing and GenBank (2026-09-29). With the live server, the server
is rebuilt from source on every run while the CLI ground truth stays as built. Session
and GenBank bugs are therefore visible to the scenarios as well as to the unit tests.

| Injected bug | Caught by |
| --- | --- |
| GenBank writer drops `/dnagent_location` | formats unit tests, shift-click-translated-feature, wrap-span-feature |
| GenBank writer drops unplaced primers | formats unit tests, save-reopen-roundtrip |
| GenBank writer drops source warnings | formats unit tests, save-reopen-roundtrip |
| Save does not clear the unsaved state | desktop-api unit tests, shift-click-translated-feature |
| Undo truncates history (no redo) | desktop-api unit tests, undo-redo-delete |
| Shift-click ignores the forward/wrap rule | wrap-span-feature |
| Shift-click ignores the anchor's own extent | shift-click-contained (added for this) |
| Dialog ignores the chosen strand | wrap-span-feature |
| Dialog ignores Translate | shift-click-translated-feature |
| Delete offered for imported features (rule retired 2026-09-30: any feature can now be deleted, after a warning) | undo-redo-delete |
| ⇧⌘Z undoes instead of redoing | undo-redo-delete |
| Unsaved indicator never shown | shift-click-translated-feature |

Fifth round, tabs and agent handoff (2026-09-29):

| Injected bug | Caught by |
| --- | --- |
| Switching tabs does not restore the selection | tabs-isolation |
| Undo history leaks between tabs | tabs-isolation (and six others) |
| Handoff snapshots omit unsaved edits | desktop-api unit tests, agent-handoff-product |
| The handoff folder is watched | desktop-api unit tests, agent-handoff-product |
| A dirty tab reloads over unsaved edits | workspace-reload |
| A clean tab never reloads | workspace-reload |
| Reopening an open file adds a duplicate tab | tabs-isolation (after replacing a tautological check with `tab_count`) |
| Handoff sends the active tab's selection for every tab | agent-handoff-product |
| The test server lacks a command | desktop-api dispatch test (added after this happened for real), agent-handoff-product, workspace-reload |

Sixth round, restriction enzymes (2026-09-29):

| Injected bug | Caught by |
| --- | --- |
| Sequence cut marks one base late | enzymes-unique-sites, enzymes-digest-linear, enzymes-origin-site |
| A map label names only one of the enzymes cutting there | enzymes-unique-sites, enzymes-digest-linear |
| Clicking a site selects one base too few | enzymes-unique-sites, enzymes-origin-site |
| The digest list drops a fragment | enzymes-digest-linear, enzymes-origin-site |
| "6+ cutter" counts N positions | unit test `specificity ignores N` only: no fixture has a unique N-containing enzyme with fewer than 6 specified bases |

The first run also caught a real layout problem: pUC19's crowded polylinker hid 12 of 25
site labels at 1440×900. Site labels now use a tighter 17 px slot, which cuts that to 8.
The scenario checks accounting at that size and the full label list at 1920×1200.

Seventh round, detect features (2026-09-30):

| Injected bug | Caught by |
| --- | --- |
| Nested proposals ticked by default | detect-features-add-undo |
| The panel is not refreshed after an edit | detect-features-add-undo |
| Annotated proposals not marked | detect-features-add-undo, detect-features-annotated |
| Added features one base short | detect-features-add-undo, after adding a check: at first the GUI and the CLI on the saved file agreed with each other about the wrong features. The scenario now remembers the unticked proposals and requires exactly those to stay new. |

Eighth round, delete with a warning (2026-09-30): Cancel still deletes; deletes
without asking; Escape confirms. All caught by undo-redo-delete and
delete-and-readd-from-library.

Harness lesson: a first pass showed `orfs-sequence-and-map` "catching" unrelated GUI bugs.
The injection script had restored the engine source but not rebuilt the CLI, so the
ground truth itself was stale. `npm run e2e` always rebuilds the CLI for this reason.

## Design review gallery

`npm run review [-- local.dna …]` renders every public review fixture and any local files
in light and dark mode at 1440×900, 1100×720 and 1920×1200, plus the largest and first
multipart feature selected, and writes `e2e/artifacts/review/latest/index.html`. The
previous run moves to `review/previous/` and appears dimmed under each tile. Each tile
notes drawn features, hidden labels and selection parts from `getState()`. It starts its
own test server (port 1433), so local files open directly by absolute path; only
screenshots are written. Scenarios cannot reference local files (fixture paths must be
under `fixtures/formats/`). The gallery is for human judgement; correctness stays with
`npm run e2e`.

## Limits

- **Chromium, not the production webview.** Production runs in WKWebView (macOS) or
  WebKitGTK (Linux). The harness tests app logic, event wiring and layout in
  Chromium; it doesn't test the webview engine, native `invoke` serialisation, the
  Tauri shell or the native dialog. Real-shell testing (tauri-driver on Linux) is out
  of scope.
- Map `parts` come from per-part data on the selection bands, so dropped,
  reordered or shifted parts are caught. Errors in the arc/line geometry are
  not caught; only the screenshots show those.
- No visual regression baselines, keyboard-navigation scenarios or open-failure
  scenarios yet.
