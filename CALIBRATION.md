# OpenLaser Calibration Feature — Working Notes

**Status: in progress · Branch: `feat/calibration` · Base: upstream `88139a9` (0.1.0-alpha.4.1)**
**Last updated: 2026-11-01**

This file is for whoever picks this work up next — a human or another
agent. It records what this feature is, what is done, what is verified, and
what remains, in the order we would do it. It is working documentation, not a
design doc; the full design rationale lives in `~/laser/audit.md`
(audit of upstream, outside this repo) and the conversation that
produced this branch.

---

## What the feature is

A material profile builder ("calibration wizard") for OpenLaser, in the
spirit of 3D-printer slicer material calibration: cut one labelled test
pattern per tunable parameter, each cell varying **one** setting around the
recipe's anchor value; the operator inspects the plate, picks the best
cell, and the pick becomes the new anchor for the next parameter. At the
end the tuned recipe is saved to the library.

The vendor's cut parameters shipped with the machine are "cut
something, called it good" numbers; this workflow exists to dial them in
deliberately, one parameter at a time.

The plan was pitched to, and accepted by, the repo owner in three
increments:

1. `openlaser-calibration` crate — sweep math + test-pattern geometry. **DONE.**
2. Server session + API. **DONE.**
3. UI wizard. **NOT STARTED.**

---

## Commit map

| Commit | Contents |
|---|---|
| `ba2ed06` | Increment 1: the `openlaser-calibration` crate (types, sweep math, pattern generators, 19 unit tests, `examples/preview.rs` SVG renderer) |
| `b85addc` | Increment 2: `openlaser-server` session + `/api/calibration` routes + 3 integration tests |

Everything passes the project's gates as of those commits:
`cargo fmt --check`, `clippy -D warnings` (pedantic), `cargo doc
-D warnings`, `gate-layers`, and the full workspace test suite
(871 tests passing, up from upstream's 849 — 19 crate + 3 server).

---

## What exists (map of the code)

### `crates/openlaser-calibration/` (pure, synchronous; passes gate-layers)

- `src/parameter.rs` — `Parameter`, an enum of the 9 tunable settings:
  `Focus, Pressure, Power, Speed, Frequency, Height, PierceDelay,
  PiercePower, PierceFrequency`. Each knows its M-Laser attribute key
  (`CutSpeed`, `DrillDelay0`, `OpenLaserManualFocus`, …), display
  domain, decimals, label format (`Sp 46.7 mm/s`), and an
  `anchor()` reader that validates a recipe's current value. **Note:**
  the attribute-key mapping is the contract the server relies on; change
  both together.
- `src/sweep.rs` — `Sweep { parameter, anchor, step, below, above }` with
  per-parameter `recommended()` spans, domain clamping in `values()`,
  and `refined(choice)` which halves the span around a pick (the
  "home in" behaviour).
- `src/pattern.rs` — generators: `line_ladder` (focus/speed), 
  `square_grid` (pressure/power/frequency/height), `pierce_grid`,
  `kerf_comb`, `window_grid` (speed × power), and the dispatcher
  `pattern_for()`. Each cell gets its **own layer** + a **text label
  cut on that layer** (via `openlaser_svg::Text`). `into_place()`
  shifts finished patterns into the first quadrant (centred labels can
  overflow past the first cell). `svg()` renders a pattern for the
  library part.
- `examples/preview.rs` — writes all 11 patterns as SVG files:
  `cargo run -p openlaser-calibration --example preview -- /tmp/x`
- `tests/` — `sweeps.rs` (9), `patterns.rs` (10).

### `crates/openlaser-server/src/calibration.rs` (the session)

`Session { working, remaining, picks, step, revision }` plus
`Coordinator::{start, generate, pick, save, cancel}_calibration` and
`calibration_session_view()` (a `serde_json::Value` for the UI —
**not ts-rs typed yet**; see Remaining work).

Key mechanics, learned the hard way (each of these was a caught
bug — do not regress them):

- Variant recipes are minted into the library only long enough for the
  draft's `LayerChange::Recipe` choices to snapshot them, then removed;
  **`library_changed()` must be called after the removals** or the
  library view still shows the variants.
- Variant recipe names use the **layer name** (`Aluminum · Focus -2`),
  never the cell label — labels contain `mm/s` and `/` is forbidden
  by `clean_name` in `openlaser-library`.
- The pattern draft's stock rectangle is grown **10 mm** past the
  pattern bounds, because draft preparation enforces a default 3 mm
  edge margin and refuses otherwise ("a part or machining path reaches
  the sheet edge margin").
- Draft preparation is **on-demand**: tests must call
  `openlaser_server::machine::prepare(&shared).await` and wait
  for the document, or the preview stays `None`. Follow
  `tests/correction.rs` for the pattern.
- `Geometry::default()` patterns are ~100–150 mm wide; they fit any bed.

### API surface (all verified live over HTTP)

```
GET  /api/calibration            the session view, or null
POST /api/calibration           cancel the session
POST /api/calibration/start     { "recipe": id }
POST /api/calibration/generate  {}           mint the current pattern draft
POST /api/calibration/pick     { "value": f64 }   apply a pick, advance
POST /api/calibration/save       { "name": optional }
```

These are wired in `api.rs` via `mod calibration;` +
`.merge(calibration::routes())`, and live in
`src/api/calibration.rs`. POST bodies are JSON; see the handlers.

### `crates/openlaser-server/tests/parameter_calibration.rs`

3 tests over the simulator harness (`common/mod.rs`):
full sweep-all-9-settings lifecycle (checks per-layer variants span the
sweep, variants gone from the library, picks land in the saved
recipe), pattern-draft preparation, and the refusal paths.

---

## Verified behaviour (live, against the real binary + simulator)

Session start on the bundled "Aluminum 1mm N2" recipe detects all 9
tunable parameters. `generate` for focus produces a draft named
`Calibration Aluminum OpenLaserManualFocus` with 7 layers
(`Focus -5` … `Focus 1`), each holding a variant recipe whose
`OpenLaserManualFocus` attribute spans −5…+1. Picking cells moves
through the parameters in order; `save` wrote
"Aluminum 1mm N2 audit-tuned" with focus −2, pressure 8, power 80,
original speed, and a `calibrated` tag — confirmed on disk and
present after a server restart.

---

## Remaining work, in the order we would do it

### Increment 3: the UI wizard (the main gap)

A "Calibrate" flow on the Materials page, desktop and phone layouts,
against the endpoints above. Suggested shape:

- A Calibrate action on a recipe card → `POST /start`.
- A step view: current parameter name, the pattern opened as the
  working draft (the normal drawing view already shows it — no new
  canvas work needed), the cells listed from the session view's
  `cells` (label + value), and a **pick** control per cell
  (`POST /pick {value}`) — a tap on the drawn cell that matches
  `cell.center` would be nicer; hit-testing exists in the shape
  editor.
- After a pick: the view refreshes (`current` advanced); a Generate
  button for the next step; progress shown from `picks` +
  `steps`.
- At the end: name field + Save (`POST /save`), then Cancel or
  navigate away.
- UI conventions live in `ui/DESIGN.md` (44 px targets, held
  confirmations for destructive actions). Svelte components live in
  `ui/src/components/`, routes in `ui/src/routes/Materials.svelte`.
  Types for the session view should move to ts-rs exports
  (see how `DraftView` etc. do it in `crates/openlaser-server`, the
  `typescript` feature) rather than staying `serde_json::Value`.

**UI copy that matters:** the focus step must tell the operator to
re-focus the head between ladder rows — a focus sweep cuts one row
per pass; the other steps cut the whole pattern in one run.

### Follow-ups (decided against, for the first PR — but agreed worth doing)

- Surface the **kerf gauge** and **window grid** as steps or
  companion actions; they exist in the crate
  (`kerf_comb`, `window_grid`) but the session currently only
  sweeps single parameters and `pick` matches one value. A kerf
  step wants a *measured* input (mm), not a cell pick.
- ts-rs typing for the session view (above).
- A focus sweep's variant recipes all carry the same
  `OpenLaserManualFocus` delta, which is correct (each row is cut
  under its own layer recipe), but the wizard could also toggle
  layer output one row at a time instead — decide during the UI
  work.
- CHANGELOG entry when the feature lands (the project keeps a strict,
  plain-language changelog).

### Upstream-PR checklist (when Increment 3 is done)

1. Rebase on upstream main; confirm the gates:
   `just check` (it runs everything; `just` is the sanctioned
   entry point and avoids the ts-rs churn — see below).
2. Update CHANGELOG.md in the project's plain-language style.
3. The two audit findings that touch docs (README's
   simulator-fixture dead end) are **upstream's to fix**, not ours —
   do not fold them into this PR.

---

## Operational notes for whoever runs this repo

- `cargo test --all-features` regenerates `ui/src/api/bindings/*.ts`
  with whitespace churn; the project's own answer is `just
  ui-bindings-check` (exports to a temp dir). If you see 160+ dirty
  bindings files after a test run: `git restore ui/src/api/bindings/`.
  Both commits here were made after restoring them.
- The `desktop` feature needs WebKitGTK dev packages
  (`libwebkit2gtk-4.1-dev`) on Linux; test without it via
  `--exclude openlaser`, or install the package.
- The full workspace suite takes ~7–10 minutes; the targeted ones
  are seconds: `cargo test -p openlaser-calibration`,
  `cargo test -p openlaser-server --test parameter_calibration`.
- Simulator smoke test recipe (what these notes were verified
  with):
  ```sh
  cargo build -p openlaser
  ./target/debug/openlaser --simulate --no-browser --data /tmp/ol --listen 127.0.0.1:8080
  # then: import fixtures/xml/harness-backup.xml via
  # POST /api/machine/files?name=… , restart the server,
  # take control via POST /api/access/control with header
  # x-openlaser-client: <id>, then /api/calibration/…
  ```
  (The harness-fixture import + restart dance is the upstream
  README bug from the audit; the real 1390backup.xml works without
  it.)
