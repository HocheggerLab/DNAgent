# Desktop architecture decision: Tauri + native Rust

Status: agreed direction; initial read-only prototype, not a finished workbench.

The GUI will be a central part of DNAagent. CLI-first validated the engine; it must
not become an indefinite prerequisite that prevents desktop development.

## Decision

- Use Tauri 2 for the desktop shell and a TypeScript web frontend.
- Execute biology natively in Rust. WASM/browser deployment is deferred.
- Retain the CLI and existing egui viewer during evaluation; do not migrate by deletion.
- CLI and desktop call the shared application/domain layers, not each other's commands.
- Rust owns biological transformations, validation, provenance and durable project operations.
- TypeScript owns presentation and transient selection/interaction state.
- Separate transport DTOs from internal domain structs. Generate TypeScript types from
  Rust, validate requests in Rust, and test the generated contract for drift.
- Future asynchronous analysis APIs need job identity, document revisions, cancellation,
  and structured diagnostics. The prototype only uses a request counter to discard
  stale imports; it does not yet cancel background work.

This updates the earlier GUI deferral in the roadmap. Architecture consolidation and
a bounded GUI prototype now proceed together. See `architecture-review.md` for the
trade-offs; its egui recommendation is superseded by this agreed decision.

## Prototype boundary

`dnagent-desktop-api` is a workspace library providing explicit DTOs and a read-only
open operation over `dnagent-app`. `ts-rs` generates `desktop/src/bindings.ts`; a Rust
test detects drift. The Tauri shell invokes this service on a blocking worker thread.
The web frontend provides Map and Sequence tabs with shared feature selection.
The Sequence tab displays the forward reference (5′→3′) and its coordinate-aligned
complement (3′→5′), computed by an IUPAC-aware domain method in Rust. It renders
source feature parts as tracks on fixed 60-base lines. Display clipping preserves
multipart gaps and origin crossings; strand arrowheads appear only at real source-part
ends, not at artificial line breaks. Imported primer names, sequences and descriptions
are available in an explicitly unplaced list: the current importer does not retain
binding coordinates. No primer positions, translations or enzyme sites are inferred.
Ordered multipart/circular intervals retain their original meaning; UI highlighting
is a display projection, not a second biological calculation.

The small frontend uses vanilla TypeScript/Vite to evaluate interaction before
choosing React/Svelte or a state framework. Rust DTO generation does not yet generate
command names/arguments; `open_document` is a small manually maintained command seam.

The native shell has its own workspace/lockfile to keep WebView dependencies out of
the core workspace validation/build. The DTO crate remains in the core workspace.
This is prototype packaging, not a claim that separate release processes are needed.

## Security and limitations

No shell, filesystem or network plugin commands are exposed to JavaScript. The
native dialog plugin grants only `dialog:allow-open` to the main window. One custom
command reads a user-entered or picker-selected local file path through the existing importer. It is not a file
sandbox; compromised trusted frontend code could invoke that command on other paths.
Do not load remote frontend content. A CSP restricts production content and connections.

Read-only, one document, no project persistence, editing, undo, assembly UI, jobs API
or WASM. A native file picker supplements path entry. A 100,000-base viewer limit bounds DOM rendering after import;
it does not impose an input-file byte limit. Rendering is intentionally unvirtualised.
Feature lanes are illustrative, not collision-free layouts. Map labels use vertically
spaced leaders, with full names available on hover; arrowheads indicate each source
part's strand direction (unknown strands have no arrows). Imported feature-level hex
colours are retained, with stable fallback colours; per-segment colours are not exposed.
Sequence clicks cycle overlapping features on either strand; the coordinate ruler
marks every ten reference bases. Map label text is dark independently of imported
feature colours for readability. Import warnings remain available in a collapsed
details panel. Tab controls support arrow keys, Home and End. Reverse
features highlight their reference positions without reverse-complementing the view.

## Next acceptance milestone

Manually test the native window with public linear, reverse/multipart and origin-spanning
fixtures. Display-helper tests cover origin crossings, arrow traversal, colour validation
and label spacing, plus row clipping and real source-end preservation. Rust tests
cover all IUPAC complementary symbols and the generated transport contract.
End-to-end selection, tabs, picker and stale-request tests remain to add.
Then add a validated assembly/junction view. Harden file limits and command
contracts before treating this as a supported release.
