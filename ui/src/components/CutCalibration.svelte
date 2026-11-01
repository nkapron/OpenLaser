<script lang="ts">
  // The material profile builder's wizard: one recipe setting swept at a
  // time, the pattern cut as the working job, the best cell picked from
  // the plate. The server owns the session; this dialog reads its view and
  // posts its actions.
  //
  // The focus step is a physical setup rather than a program word, so it
  // runs in two phases: "Cut rows" makes a tapped row the only layer
  // that cuts — the operator re-focuses and runs once per row — and
  // "Pick" reads the finished plate like every other step. Nothing here
  // moves the machine, so no hold is needed (DESIGN.md rule 1); running
  // the pattern is the Run screen's own held Start.
  import { api } from '../api/client';
  import type { CalibrationSession, RecipeView } from '../api';
  import { server } from '../stores/server.svelte';
  import { ui } from '../stores/ui.svelte';
  import { osk } from '../lib/osk.svelte';
  import { plural } from '../lib/format';
  import { quantity } from '../lib/units.svelte';
  import { withBusy } from '../lib/busy';
  import { cellLayout, currentStep, isFocus, progress, screenY, stepLabel } from '../lib/calibration';
  import Modal from './Modal.svelte';

  let { recipe, onclose }: { recipe: RecipeView; onclose: () => void } = $props();
  let session = $state<CalibrationSession | null>(null);
  let ready = $state(false);
  let busy = $state(false);
  /** The focus step's phase: cutting one row per pass, or reading the plate. */
  let phase = $state<'cut' | 'pick'>('cut');

  const current = $derived(session ? currentStep(session) : null);
  const focus = $derived(isFocus(session?.current ?? null));
  const counts = $derived(session ? progress(session) : { done: 0, total: 0 });
  const layout = $derived(cellLayout(session?.cells ?? []));
  const draft = $derived(server.doc?.draft ?? null);
  const cutLayers = $derived(new Set((draft?.layers.filter((l) => l.output).map((l) => l.name)) ?? []));
  // A new pattern begins back at the row-cutting phase; set where it starts.

  /** What a step's plate is read for, in the operator's words. */
  const HINTS: Record<string, string> = {
    OpenLaserManualFocus: 'Focus is set by hand, so this ladder cuts one row per pass: make a row the only one that cuts, re-focus the head, run, and repeat.',
    CutAirPressure: 'Turn each square over: too little pressure hangs dross, too much blows the edge away.',
    CutPower: 'Watch the squares’ edges and corners: too little power leaves dross, too much burns the corner.',
    CutSpeed: 'Read the lines bottom to top: too slow burns, too fast does not cut through.',
    CutFreq: 'Watch the edges: the frequency sets the pulse marks and the heat each pulse puts in.',
    CutHeight: 'The nozzle height follows the sheet: too close burns the cut, too far loses it.',
    DrillDelay0: 'Read the spatter around each dot: too short a delay never gets through, too long blows a crater.',
    DrillPower0: 'Read the spatter around each dot: too little power never gets through, too much blows a crater.',
    DrillFreq0: 'Read the spatter around each dot: the pierce frequency sets how its pulses stack up.',
  };

  async function refresh(): Promise<void> {
    try {
      session = (await api.calibrationSession()).session;
    } catch { /* the poll retries; the actions report their own failures */ }
    finally { ready = true; }
  }
  $effect(() => { void refresh(); });
  // A second screen may hold the picks; keep this one current while open.
  $effect(() => {
    const timer = setInterval(() => { void refresh(); }, 5000);
    return () => { clearInterval(timer); };
  });

  async function act(action: () => Promise<void>): Promise<void> {
    if (busy) return;
    await withBusy((b) => (busy = b), action);
  }
  function start(): Promise<void> {
    return act(async () => { session = (await api.startCalibration(recipe.id)).session; });
  }
  function generate(): Promise<void> {
    return act(async () => { session = (await api.generateCalibration()).session; phase = 'cut'; });
  }
  function pick(value: number): Promise<void> {
    return act(async () => { session = (await api.pickCalibration(value)).session; phase = 'cut'; });
  }
  /** Makes `layer` the only pattern layer that cuts; one undo step per row. */
  function cutOnly(layer: string): Promise<void> {
    const rows = session?.cells ?? [];
    return act(async () => {
      for (const row of rows) await api.changeLayers({ kind: 'output', layer: row.layer, on: row.layer === layer });
    });
  }
  function cutEveryRow(): Promise<void> {
    const rows = session?.cells ?? [];
    return act(async () => {
      for (const row of rows) await api.changeLayers({ kind: 'output', layer: row.layer, on: true });
    });
  }
  function save(name?: string): Promise<void> {
    const tuned = session?.recipe.name ?? recipe.name;
    return act(async () => {
      const saved = await api.saveCalibration(name);
      ui.selectedRecipe = saved.id;
      ui.say(`${tuned} calibration saved to the library.`);
      onclose();
    });
  }
  function nameIt(): void {
    const fallback = `${session?.recipe.name ?? recipe.name} calibrated`;
    osk.text('Calibrated recipe name', fallback, (value) => {
      if (value.trim()) void save(value.trim());
    });
  }
  async function discard(): Promise<void> {
    const tuned = counts.done;
    const confirmed = await ui.confirm({
      title: 'Discard this calibration?',
      body: tuned
        ? `${plural(tuned, 'step')} tuned so far are thrown away; ${session?.recipe.name ?? recipe.name} keeps its values.`
        : 'Nothing is saved; the recipe keeps its values.',
      confirm: 'Discard calibration',
      danger: true,
    });
    if (!confirmed) return;
    await act(async () => { await api.cancelCalibration(); onclose(); });
  }
</script>

<Modal title="Calibrate {recipe.name}" onclose={onclose}>
  {#if !ready}
    <p class="muted">Loading…</p>
  {:else if !session}
    <div class="intro">
      <p class="muted">Tune {recipe.name} · {recipe.thickness_mm > 0 ? quantity(recipe.thickness_mm, 'mm') : 'no thickness set'} {recipe.gas} one setting at a time on scrap: each step cuts a small labelled test pattern, you pick the best cell from the plate, and the pick becomes the next step's starting point.</p>
      <button class="btn btn-primary lg block" disabled={busy} onclick={start}>Start calibration</button>
    </div>
  {:else}
    <div class="wizard">
      <header class="wizard-head">
        <span>{session.recipe.name} · {session.recipe.thickness_mm > 0 ? quantity(session.recipe.thickness_mm, 'mm') : 'no thickness set'} {session.recipe.gas}</span>
        <span>{counts.done} of {counts.total} tuned</span>
      </header>
      <ol class="steps" aria-label="Calibration steps">
        {#each session.steps as step (step.parameter)}
          <li class:done={step.picked !== null} class:on={step.parameter === session.current}>{stepLabel(step)}</li>
        {/each}
      </ol>

      {#if session.complete}
        <h3>Every step is tuned</h3>
        <dl class="picks">
          {#each session.steps as step (step.parameter)}
            <div><dt>{step.name}</dt><dd>{step.picked ?? '—'}</dd></div>
          {/each}
        </dl>
        <div class="row-actions">
          <button class="btn btn-ghost" disabled={busy} onclick={discard}>Discard</button>
          <button class="btn btn-ghost" disabled={busy} onclick={nameIt}>Name it…</button>
          <button class="btn btn-primary" disabled={busy} onclick={() => save()}>Save “{session.recipe.name} calibrated”</button>
        </div>
      {:else if current}
        <div class="step-card">
          <span class="eyebrow">Step {counts.done + 1} of {counts.total}</span>
          <h3>{current.name}</h3>
          <p class="muted">{HINTS[current.parameter] ?? 'Cut the pattern, then pick the best cell.'}</p>
          {#if session.cells.length === 0}
            <button class="btn btn-primary lg block" disabled={busy} onclick={generate}>Generate the {current.name.toLowerCase()} pattern</button>
          {:else}
            {#if focus}
              <div class="phase" role="group" aria-label="Focus phase">
                <button class:on={phase === 'cut'} aria-pressed={phase === 'cut'} onclick={() => (phase = 'cut')}>Cut rows</button>
                <button class:on={phase === 'pick'} aria-pressed={phase === 'pick'} onclick={() => (phase = 'pick')}>Pick</button>
              </div>
            {/if}
            <p class="muted">{#if focus && phase === 'cut'}Tap a row to make it the only one that cuts, then run it from the Run screen with the head re-focused.{:else}The pattern is the working job. {#if focus}Cut every row first, then pick the cleanest one.{:else}Cut it from the Run screen, then pick the best cell as it lies on the plate.{/if}{/if}</p>
            <div class="plate" style="width:{layout.width}px; height:{layout.height}px" role="group" aria-label="Pattern cells">
              {#each layout.placed as { cell, x, y } (cell.label)}
                {@const picking = !focus || phase === 'pick'}
                <button
                  class:cut={focus && phase === 'cut' && cutLayers.has(cell.layer)}
                  style="left:{x - 28}px; top:{screenY(layout, y) - 28}px"
                  disabled={busy || cell.value === null}
                  aria-label={(picking ? 'Pick ' : 'Cut only ') + cell.label}
                  aria-pressed={picking ? undefined : cutLayers.has(cell.layer)}
                  onclick={() => (picking ? cell.value !== null && pick(cell.value) : cutOnly(cell.layer))}
                >{cell.label}</button>
              {/each}
            </div>
            <div class="row-actions">
              {#if focus && phase === 'cut'}
                <button class="btn btn-ghost" disabled={busy} onclick={cutEveryRow}>Cut every row</button>
              {/if}
              <button class="btn btn-ghost" onclick={() => (ui.tab = 'run')}>Go to Run <i class="ic ic-arrow-right"></i></button>
              <button class="btn btn-ghost recipe-delete" disabled={busy} onclick={discard}>Discard</button>
            </div>
          {/if}
        </div>
      {/if}
    </div>
  {/if}
</Modal>

<style>
  .intro { display: flex; flex-direction: column; gap: 12px; }
  .intro p { margin: 0; }
  .wizard { display: flex; flex-direction: column; gap: 14px; min-width: min(560px, 92vw); }
  .wizard-head { display: flex; justify-content: space-between; gap: 12px; color: var(--ink-3); font-size: var(--t-sm); }
  .steps { display: flex; flex-wrap: wrap; gap: 8px; margin: 0; padding: 0; list-style: none; counter-reset: step; }
  .steps li { display: flex; align-items: center; min-height: 44px; padding: 6px 12px; border: 1px solid var(--line); border-radius: 10px; background: var(--panel-2); color: var(--ink-3); font-size: var(--t-sm); font-weight: 600; }
  .steps li::before { content: counter(step) ". "; counter-increment: step; }
  .steps li.on { color: var(--ink); border-color: var(--accent); background: color-mix(in srgb, var(--accent) 8%, var(--panel)); }
  .steps li.done { color: var(--ok); }
  .eyebrow { text-transform: uppercase; letter-spacing: .12em; font-size: var(--t-sm); color: var(--accent); font-weight: 700; }
  .step-card { display: flex; flex-direction: column; gap: 12px; }
  .step-card h3 { margin: 0; font-size: var(--t-lg); }
  .step-card p { margin: 0; }
  .phase { display: flex; gap: 0; border: 1px solid var(--line); border-radius: 10px; overflow: hidden; }
  .phase button { flex: 1; min-height: 44px; border: 0; background: var(--panel-2); color: var(--ink-3); cursor: pointer; }
  .phase button.on { color: var(--ink); background: var(--panel); box-shadow: inset 0 -3px 0 var(--accent); }
  .plate { position: relative; margin: 4px auto; border: 2px solid var(--line); border-radius: 12px; background: var(--panel-2); }
  .plate button {
    position: absolute; width: 56px; height: 56px; display: grid; place-items: center; padding: 2px;
    border: 1px solid var(--line); border-radius: 8px; background: var(--panel); color: var(--ink);
    font-size: var(--t-sm); cursor: pointer;
  }
  .plate button.cut { border-color: var(--ok); background: color-mix(in srgb, var(--ok) 12%, var(--panel)); }
  .row-actions { display: flex; justify-content: flex-end; gap: 8px; flex-wrap: wrap; }
  .picks { margin: 0 0 8px; }
  .picks div { display: flex; justify-content: space-between; gap: 15px; padding: 12px 0; border-bottom: 1px solid var(--line); font-size: var(--t-sm); }
  .picks dt { color: var(--ink-3); }
  .picks dd { margin: 0; }
  @media (max-width: 720px) { .wizard { min-width: 0; } }
</style>
