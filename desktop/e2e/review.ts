// Design-review gallery: renders construct views in light and dark mode at several
// window sizes and writes one HTML contact sheet (with the previous run for
// before/after comparison). Local files are opened by the live Rust test server; only
// screenshots are written, into the gitignored e2e/artifacts/. Private files never enter Git.
//
//   npm run review                       # public fixtures only
//   npm run review -- ~/path/to/file.dna # plus local files
import { chromium, type Page } from '@playwright/test';
import { execFileSync, spawn } from 'node:child_process';
import { existsSync, mkdirSync, renameSync, rmSync, writeFileSync } from 'node:fs';
import { basename, relative, resolve } from 'node:path';
import type { AppState } from '../src/testing/automation.ts';

const DESKTOP = resolve(import.meta.dirname, '..');
const REPO = resolve(DESKTOP, '..');
const ARTIFACTS = resolve(DESKTOP, 'e2e/artifacts');
const OUT = resolve(ARTIFACTS, 'review/latest');
const PREVIOUS = resolve(ARTIFACTS, 'review/previous');
const PORT = 1423;
const DEFAULT_FIXTURES = [
  'fixtures/formats/snapgene/pUC19_M77789.dna',
  'fixtures/formats/snapgene/synthetic_circular.dna',
  'fixtures/formats/snapgene/synthetic_multipart_origin.dna',
  'fixtures/formats/snapgene/synthetic_linear.dna',
  'fixtures/formats/snapgene/synthetic_translation.dna',
];
const SIZES = [{ width: 1440, height: 900 }, { width: 1100, height: 720 }, { width: 1920, height: 1200 }];
const THEMES = ['light', 'dark'] as const;

interface Shot { file: string; caption: string; notes: string[] }

function inputs(): { key: string; label: string }[] {
  const local = process.argv.slice(2).map(path => resolve(process.cwd(), path.replace(/^~(?=\/)/, process.env.HOME ?? '~')));
  for (const path of local) if (!existsSync(path)) throw new Error(`no such file: ${path}`);
  return [...DEFAULT_FIXTURES.map(key => ({ key, label: basename(key) })), ...local.map(key => ({ key, label: `${basename(key)} (local)` }))];
}

/** Feature ids, names, total covered length and part count from the CLI (ground truth). */
function cliFeatures(path: string): { id: string; name: string; length: number; parts: number }[] {
  const envelope = JSON.parse(execFileSync(resolve(REPO, 'target/debug/dnagent'), ['features', path, '--output', 'json'], { cwd: REPO, encoding: 'utf8' }));
  return (envelope.result as { id: string; label: string; location: { parts: ({ kind: 'linear'; start: number; end: number } | { kind: 'circular_arc'; length: number })[] } }[])
    .map(f => ({ id: f.id, name: f.label, parts: f.location.parts.length,
      length: f.location.parts.reduce((sum, p) => sum + (p.kind === 'linear' ? p.end - p.start : p.length), 0) }));
}

const slug = (text: string) => text.replace(/\.[^.]+$/, '').replace(/[^A-Za-z0-9]+/g, '-').replace(/^-|-$/g, '');
const state = (page: Page) => page.evaluate(() => window.__DNAGENT_TEST__!.getState());
const idle = (page: Page) => page.waitForFunction(() => window.__DNAGENT_TEST__!.getState().idle);

function notesFor(snapshot: AppState): string[] {
  const notes: string[] = [];
  const map = snapshot.map;
  if (map) {
    notes.push(`${map.drawn_ids.length}/${snapshot.features.length} features drawn`);
    if (map.unlabelled_ids.length) notes.push(`${map.unlabelled_ids.length} labels not shown: ${map.unlabelled_names.join(', ')}`);
  }
  if (snapshot.selection.map?.parts.length) notes.push(`selection parts ${JSON.stringify(snapshot.selection.map.parts)}`);
  return notes;
}

async function capture(page: Page, name: string, caption: string, shots: Shot[]) {
  const file = `${name}.png`;
  await page.screenshot({ path: resolve(OUT, file) });
  shots.push({ file, caption, notes: notesFor(await state(page)) });
}

function gallery(sections: { label: string; shots: Shot[] }[]): string {
  const cell = (shot: Shot) => {
    const before = existsSync(resolve(PREVIOUS, shot.file)) ? `<a href="../previous/${shot.file}"><img src="../previous/${shot.file}" class="before" alt="previous"></a>` : '';
    return `<figure><div class="pair"><a href="${shot.file}"><img src="${shot.file}" alt=""></a>${before}</div>
      <figcaption><b>${shot.caption}</b>${shot.notes.map(n => `<br>${n}`).join('')}</figcaption></figure>`;
  };
  return `<!doctype html><meta charset="utf-8"><title>DNAgent map review</title>
<style>body{font:13px system-ui;margin:20px;background:#f4f5f7;color:#222}@media(prefers-color-scheme:dark){body{background:#161719;color:#ddd}}
section{margin-bottom:40px}.grid{display:grid;grid-template-columns:repeat(auto-fill,minmax(420px,1fr));gap:16px}
figure{margin:0}img{width:100%;border:1px solid #8886;border-radius:4px}.pair{display:grid;gap:4px}.before{opacity:.75}
figcaption{margin-top:4px;line-height:1.4}</style>
<h1>DNAgent map review — ${new Date().toISOString().slice(0, 16).replace('T', ' ')}</h1>
<p>Each tile shows this run on top and the previous run (dimmed) below when one exists. Click to open full size.
Coordinates are checked by <code>npm run e2e</code>; this page is for look and feel.</p>
${sections.map(s => `<section><h2>${s.label}</h2><div class="grid">${s.shots.map(cell).join('')}</div></section>`).join('')}`;
}

async function main() {
  const files = inputs();
  if (existsSync(OUT)) { rmSync(PREVIOUS, { recursive: true, force: true }); renameSync(OUT, PREVIOUS); }
  mkdirSync(OUT, { recursive: true });
  const SERVER_PORT = 1433;
  const server = spawn('cargo', ['run', '-q', '-p', 'dnagent-desktop-api', '--example', 'e2e_server', '--', '--port', String(SERVER_PORT)], { cwd: REPO, stdio: 'ignore' });
  const vite = spawn('npx', ['vite', '--mode', 'e2e', '--host', '127.0.0.1', '--port', String(PORT), '--strictPort'],
    { cwd: DESKTOP, stdio: 'ignore', env: { ...process.env, DNAGENT_E2E_PORT: String(SERVER_PORT) } });
  try {
    for (const url of [`http://127.0.0.1:${SERVER_PORT}/health`, `http://127.0.0.1:${PORT}`]) {
      for (let i = 0; i < 1200; i++) { try { await fetch(url); break; } catch { await new Promise(r => setTimeout(r, 250)); } }
    }
    const browser = await chromium.launch();
    const sections: { label: string; shots: Shot[] }[] = [];
    for (const { key, label } of files) {
      const shots: Shot[] = [];
      for (const theme of THEMES) {
        const page = await browser.newPage({ viewport: SIZES[0], colorScheme: theme });
        page.on('pageerror', error => console.error(`page error (${label}): ${error.message}`));
        await page.goto(`http://127.0.0.1:${PORT}/`);
        await page.waitForFunction(() => window.__DNAGENT_TEST__ !== undefined);
        await page.evaluate(path => window.__DNAGENT_TEST__!.open(path), key); await idle(page);
        const opened = await state(page);
        if (!opened.document) throw new Error(`could not open ${key}: ${opened.status}`);
        for (const size of SIZES) {
          await page.setViewportSize(size); await page.waitForTimeout(50); // one layout frame after resize
          await capture(page, `${slug(label)}-${theme}-${size.width}x${size.height}`, `${theme} · ${size.width}×${size.height}`, shots);
        }
        await page.setViewportSize(SIZES[0]);
        // Largest feature, and the first multipart feature if any (a display choice).
        const features = cliFeatures(key);
        const largest = [...features].sort((a, b) => b.length - a.length)[0];
        const multipart = features.find(f => f.parts > 1);
        for (const [what, pick] of [['largest', largest], ['multipart', multipart]] as const) {
          if (!pick) continue;
          await page.evaluate(id => window.__DNAGENT_TEST__!.selectFeature(id), pick.id);
          await capture(page, `${slug(label)}-${theme}-select-${what}`, `${theme} · selected ${what}: ${pick.name}`, shots);
        }
        // Sequence view: largest (translated) feature, then ORFs + six frames, then 3-letter.
        if (largest) await page.evaluate(id => window.__DNAGENT_TEST__!.selectFeature(id), largest.id);
        await page.getByTestId('tab-sequence').click();
        await capture(page, `${slug(label)}-${theme}-sequence`, `${theme} · sequence · ${largest?.name ?? 'no feature'}`, shots);
        await page.getByTestId('toggle-orfs').check();
        await page.getByTestId('toggle-frames').check();
        await capture(page, `${slug(label)}-${theme}-sequence-orfs-frames`, `${theme} · sequence · ORFs ≥ 75 codons + six frames`, shots);
        await page.getByTestId('toggle-frames').uncheck();
        await page.getByTestId('amino-acid-mode').selectOption('three');
        await capture(page, `${slug(label)}-${theme}-sequence-3letter`, `${theme} · sequence · 3-letter amino acids`, shots);
        await page.getByTestId('amino-acid-mode').selectOption('one');
        await page.getByTestId('tab-map').click();
        await capture(page, `${slug(label)}-${theme}-map-orfs`, `${theme} · map with ORFs ≥ 75 codons`, shots);
        await page.getByTestId('toggle-orfs').uncheck();
        await page.close();
      }
      sections.push({ label, shots });
    }
    await browser.close();
    writeFileSync(resolve(OUT, 'index.html'), gallery(sections));
    console.log(`review gallery: ${relative(process.cwd(), resolve(OUT, 'index.html'))}\n  open ${resolve(OUT, 'index.html')}`);
  } finally { vite.kill(); server.kill(); }
}

await main();
