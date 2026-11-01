# OpenLaser Calibration Feature — Working Notes

**Status: complete (feature work) · Branch: `feat/calibration` · Base: upstream `88139a9` (0.1.0-alpha.4.1)**
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
3. UI wizard. **DONE.**

The remaining step before an upstream PR is the checklist at the bottom
of this file (rebase, `just check`, changelog — the changelog entry
itself is already written in `CHANGELOG.md` under Unreleased).

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
`calibration_session_view()` — now **typed as `CalibrationSession`**
(ts-rs exported under the `typescript` feature; see
`ui/src/api/bindings/Calibration*.ts`), no longer `serde_json::Value`.

Key mechanics, learned the hard way (each of these was a caught
bug — do not regress them):

- Variant recipes are minted into the library only long enough for the
  draft's `LayerChange::Recipe` choices to snapshot them, then removed;
  **`library_changed()` must be called after the removals** or the
  library view still shows the variants.
- Variant recipe names use the **layer name** (`Aluminum · Focus -2`),
  never the cell label — labels contain `mm/s` and `/` is forbidden
  by `clean_name` in `openlaser-library`.
- **One session at a time**: `start_calibration` refuses while one runs
  ("a calibration is already underway; cancel it first"). The UI
  catches that and refreshes.
- **A refused save keeps the session** — `save_calibration` no longer
  `take()`s the session before `add_recipe`; a bad name (say `a/b`)
  leaves every pick in place.
- **`library_changed()` must also be called after `save_calibration`'s
  `add_recipe`** — found during the UI work; the saved recipe otherwise
  never reaches the screens.
- The pattern draft's stock rectangle is grown **10 mm** past the
  pattern bounds, because draft preparation enforces a default 3 mm
  edge margin and refuses otherwise ("a part or machining path reaches
  the sheet edge margin").
- Draft preparation is **on-demand**: tests must call
  `openlaser_server::machine::prepare(&shared).await` and wait
  for the document, or the preview stays `None`. Follow
  `tests/correction.rs` for the pattern. (Over HTTP this is now the
  `edit()` helper's own spawned task — same as every draft edit — so
  `generate` answers with the draft and its revision.)
- `Geometry::default()` patterns are ~100–150 mm wide; they fit any bed.

### API surface (all verified live over HTTP)

```
GET  /api/calibration            the session view, or null
POST /api/calibration           cancel the session
POST /api/calibration/start     { "recipe": id }     refused while one runs
POST /api/calibration/generate  {}  answers with draft + revision + session
POST /api/calibration/pick     { "value": f64 }   apply a pick, advance
POST /api/calibration/save       { "name": optional }  a refused save keeps the session
```

These are wired in `api.rs` via `mod calibration;` +
`.merge(calibration::routes())`, and live in
`src/api/calibration.rs`. POST bodies are JSON. `generate` goes
through the same `edit()` helper as every draft edit, so its reply
carries the draft and its revision (the UI applies them at once) and
preparation and compilation run under the edit's own lifetime.

### `crates/openlaser-server/tests/parameter_calibration.rs`

5 tests over the simulator harness (`common/mod.rs`): the full
sweep-all-9-settings lifecycle (typed view assertions, per-layer
variants spanning the sweep, variants gone from the library, picks
landing in the saved recipe), pattern-draft preparation, the refusal
paths, one-session-at-a-time, and the refused-save-keeps-the-picks
behaviour.

### Increment 3: the UI wizard (done as designed, with two additions)

- `ui/src/lib/calibration.ts` (+ `calibration.test.ts`, vitest) —
  the screen-side logic: the focus attribute constant, progress,
  current-step lookup, and `cellLayout`, which places the pattern's
  cells on screen as they lie on the plate (Y up, at least 56 px
  apart) — the plate-matching pick buttons the notes asked for.
- `ui/src/components/CutCalibration.svelte` — the wizard, a modal
  over any tab (`ui.modal === 'calibration'`, rendered in
  `WorkspaceOverlays.svelte`, so it survives tab switches on both
  the desktop and phone layouts). Step chips show progress; each step
  loads its pattern with **Generate**, the cells are pick buttons
  laid out like the plate, and the wizard polls the session every 5 s
  so a second screen's picks stay current.
- The focus step runs in two phases, the "decide during the UI work"
  item from the follow-ups: **Cut rows** — a tap makes that row the
  only layer that cuts, via `LayerChange::Output`, so the operator
  re-focuses and runs once per row — then **Pick**, same as every
  other step. Per-row output toggling beat minting one recipe per row:
  no extra recipes, and one undo step per row already exists.
- `ui/src/routes/materials/Recipe.svelte` — the **Calibrate**
  action on the recipe card; it refuses while recipe edits are staged
  (the session snapshots the saved values).
- The session view is ts-rs typed (`CalibrationSession` et al.),
  regenerated into `ui/src/api/bindings/`, barrel re-exported.

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

## Remaining work

### Follow-ups (decided against, for the first PR — but agreed worth doing)

- Surface the **kerf gauge** and **window grid** as steps or
  companion actions; they exist in the crate
  (`kerf_comb`, `window_grid`) but the session currently only
  sweeps single parameters and `pick` matches one value. A kerf
  step wants a *measured* input (mm), not a cell pick.
- A canvas-tap pick that hit-tests the drawn cell (`api.pick` +
  `cell.center`) instead of the plate-mirrored buttons; nicer, but
  the buttons already read one-to-one against the plate.

### Upstream-PR checklist (increment 3 is done; this is what remains)

1. Rebase on upstream main; confirm the gates:
   `just check` (it runs everything; `just` is the sanctioned
   entry point and avoids the ts-rs churn — see below). Note: this
   machine's clippy is newer than CI's pinned 1.97.1 and already
   warns on upstream code (`chunks_exact` lints in
   `openlaser-protocol`); judge new warnings by diff, not by count.
2. The CHANGELOG entry is written (`CHANGELOG.md`, Unreleased);
   re-read it against the final diff.
3. The two audit findings that touch docs (README's
   simulator-fixture dead end) are **upstream's to fix**, not ours —
   do not fold them into this PR.
4. `ui/node_modules` was not installed on this machine when the
   wizard was written; run `npm ci` in `ui/` and then
   `npm test`, `npm run check`, `npm run build` before the PR.

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
