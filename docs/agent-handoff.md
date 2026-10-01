# Agent handoff and the DNAgent workspace

DNAgent does not run an agent or a shell. You run the agent (e.g. Claude in a terminal
beside the app), and the app shares context with it through a **workspace folder**
(default `~/DNAgent`, changeable with **Workspace…**).

## GUI → agent: Hand off to agent (⇧⌘C)

For every open tab, DNAgent writes into `<workspace>/handoff/`:

- `<name>.gb`: a DNAgent GenBank snapshot of what is open, **including unsaved edits**.
  The original file is not touched.
- `context.json`: which constructs are open and what you are pointing at.

It also shows a short prompt and copies it to the clipboard, for example:

```text
DNAgent handoff: 2 open constructs (snapshots include unsaved edits) — context: ~/DNAgent/handoff/context.json
Selections mark the regions of interest. Use the dnagent CLI and write results as GenBank into ~/DNAgent (they will open in DNAgent automatically).
```

Each handoff replaces the previous one's files in `handoff/`.

### context.json (format `dnagent-handoff`, version 1)

```jsonc
{
  "format": "dnagent-handoff", "version": 1, "created": "2026-09-29T18:04:05Z",
  "workspace": "/Users/me/DNAgent",
  "coordinates": "zero-based, half-open; selections with end < start wrap through the origin",
  "instructions": ["…"],
  "constructs": [{
    "document_id": 1, "name": "pEXAMPLE", "active": true,
    "snapshot": "/Users/me/DNAgent/handoff/pEXAMPLE.gb",   // read this with the dnagent CLI
    "source": "/Users/me/constructs/pEXAMPLE.dna", "unsaved_changes": true,
    "length": 9175, "topology": "circular",
    "selection": {"start": 1250, "end": 6188, "length": 4938, "wraps_origin": false},  // or null
    "selected_feature": { …same shape as `dnagent features` rows… },                  // or null
    "added_feature_ids": ["feature-0021"],
    "features": [ …exactly the `dnagent features --output json` result for the snapshot… ]
  }]
}
```

Keys appear in alphabetical order in the file. Feature rows and locations use the CLI's
JSON contract.

### Guidance for agents

- Work from the **snapshots**, not the `source` paths: sources may lack unsaved edits.
- Use the `dnagent` CLI (`features`, `translate`, `orfs`, `gibson`, `annotate`,
  `convert`, …). Put plan files and results in the workspace, not in `handoff/`.
- Write sequence results as **GenBank** (`--output genbank`, `annotate --out`,
  `convert --out`), so DNAgent can open them with all metadata.
- Selections and selected features are what the user means by "this", "these parts" or
  "the region".

## Agent → GUI: the watched workspace

DNAgent checks the workspace every two seconds (sequence files up to one folder deep,
ignoring hidden folders and `handoff/`), along with every file open in a tab.

- **New file** (e.g. `product.gb`): a notice offers **Open** (it opens in a new tab) or
  **Dismiss**.
- **Changed file that is not open:** the same notice, as "Changed".
- **Changed file open in a clean tab:** the tab reloads, keeping the selection if the
  feature still exists.
- **Changed file open in a tab with unsaved edits:** a notice offers **Reload (discard my
  edits)** or **Keep mine**. Nothing is thrown away without asking.

The app's own saves don't trigger notices.

## Tabs

Several constructs can be open at once. Each tab has its own selection, undo history
and unsaved state (● on the tab). ⌘1–⌘9 switch tabs. The × closes a tab, asking first if
it has unsaved edits. Opening a file that is already open switches to its tab.

## Security

The app still exposes no shell or process execution to its web view. It writes only to
the chosen workspace (handoff snapshots) and to paths you save to, and it only reads the
workspace to notice changes.

## Tests

The desktop e2e scenarios `tabs-isolation`, `agent-handoff-product` and
`workspace-reload` play the agent with real `dnagent` CLI commands in a test workspace.
They check the snapshots and `context.json` against the GUI and the CLI, and check the
notices and reloads. A Rust test checks every desktop command is served by the e2e server.
