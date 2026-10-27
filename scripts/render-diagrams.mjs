#!/usr/bin/env node
// Render the hand-laid-out diagrams in docs/diagrams/*.mjs to light and dark
// SVGs in the app's style: colours from apps/desktop/src/tokens.css and the
// Bricolage Grotesque UI font. Each source fixes every box and arrow position,
// so the layout never shifts; this script only draws it.
// Usage: node scripts/render-diagrams.mjs [name ...]
import { readFileSync, readdirSync, writeFileSync } from 'node:fs';
import { basename, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

const root = fileURLToPath(new URL('..', import.meta.url));
export const SOURCES = join(root, 'docs/diagrams');
export const OUTPUT = join(root, 'docs/assets/diagrams');
export const TOKENS = join(root, 'apps/desktop/src/tokens.css');
// Bricolage Grotesque (apps/desktop/src/assets, SIL OFL 1.1) cut to Latin plus
// the punctuation and arrows the diagrams use, weights 400 to 600:
//   uvx --from 'fonttools[woff]==4.60.1' fonttools varLib.instancer \
//     bricolage-grotesque.ttf wght=400:600 wdth=100 opsz=14 -o instance.ttf
//   uvx --from 'fonttools[woff]==4.60.1' pyftsubset instance.ttf --flavor=woff2 \
//     --layout-features=kern,liga --output-file=font.woff2 \
//     --unicodes=U+0020-007E,U+00A0,U+00B7,U+00D7,U+2013,U+2014,U+2019,U+201C,U+201D,U+2026,U+2190-2194,U+2264,U+2265
export const FONT = join(SOURCES, 'font.woff2');

const FONT_FAMILY =
  "'Vesper Diagram', 'Bricolage Grotesque', -apple-system, 'Segoe UI', sans-serif";
const TITLE_SIZE = 14;
const DETAIL_SIZE = 12;
const LINE = 16;
const CORNER = 8;

// Merges every block for a selector, in file order, as the cascade does.
export function readTokens(css) {
  const blocks = (selector) => {
    const tokens = {};
    for (let start = css.indexOf(selector); start >= 0; start = css.indexOf(selector, start + 1)) {
      const body = css.slice(css.indexOf('{', start) + 1, css.indexOf('}', start));
      for (const [, name, value] of body.matchAll(/--color-([a-z-]+):\s*([^;]+);/g)) {
        tokens[name] = value.trim();
      }
    }
    if (!Object.keys(tokens).length) throw new Error(`Missing ${selector} in tokens.css`);
    return tokens;
  };
  const dark = blocks(':root {');
  return { dark, light: { ...dark, ...blocks(":root[data-theme='light'] {") } };
}

const mix = (color, percent, base) => `color-mix(in srgb, ${color} ${percent}%, ${base})`;

// One colour per kind of thing, so a reader can tell Vesper's own code from
// native runtimes, data on disk and things outside the app.
export function palette(t) {
  return {
    bg: t.bg,
    frame: t.line,
    group: mix(t.ink, 3, t.bg),
    ink: t.ink,
    muted: t.muted,
    edge: t['control-edge'],
    app: { fill: mix(t.accent, 10, t.bg), stroke: t.accent, label: t.accent },
    runtime: { fill: mix(t.warning, 10, t.bg), stroke: t.warning, label: t.warning },
    data: { fill: t.raised, stroke: t['control-edge'], label: t.muted },
    outside: { fill: 'none', stroke: t['control-edge'], label: t.muted },
  };
}

export const LEGEND = {
  app: 'Vesper',
  runtime: 'Native runtime',
  data: 'Data on this Mac',
  outside: 'Outside Vesper',
};

const esc = (text) =>
  String(text).replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;');
const n = (value) => Number(value.toFixed(1));

function textBlock(cx, cy, title, detail = [], shift = 0) {
  const lines = Array.isArray(detail) ? detail : [detail];
  const height = (title ? LINE + 2 : 0) + lines.length * LINE;
  let y = cy + shift - height / 2 + 12;
  const out = [];
  if (title) {
    out.push(`<text class="title" x="${n(cx)}" y="${n(y)}">${esc(title)}</text>`);
    y += LINE + 2;
  }
  for (const line of lines) {
    out.push(`<text class="detail" x="${n(cx)}" y="${n(y)}">${esc(line)}</text>`);
    y += LINE;
  }
  return out.join('');
}

function drawNode(node) {
  const { x, y, w, h, role = 'app', title, detail } = node;
  const cls = `node ${role}`;
  if (role === 'data') {
    const ry = 6;
    const body = `M${x},${y + ry} a${w / 2},${ry} 0 0 1 ${w},0 v${h - 2 * ry} a${w / 2},${ry} 0 0 1 ${-w},0 z`;
    const rim = `M${x},${y + ry} a${w / 2},${ry} 0 0 0 ${w},0`;
    return `<g class="${cls}"><path d="${body}"/><path class="rim" d="${rim}"/>${textBlock(x + w / 2, y + h / 2, title, detail, ry / 2)}</g>`;
  }
  const radius = role === 'outside' ? h / 2 : 10;
  return `<g class="${cls}"><rect x="${x}" y="${y}" width="${w}" height="${h}" rx="${n(radius)}"/>${textBlock(x + w / 2, y + h / 2, title, detail)}</g>`;
}

function drawGroup(group) {
  const { x, y, w, h, role = 'app', label } = group;
  return `<g class="group ${role}"><rect x="${x}" y="${y}" width="${w}" height="${h}" rx="16"/><text class="group-label" x="${x + 16}" y="${y + 22}">${esc(label)}</text></g>`;
}

// 'id', 'id:b', 'id:r@0.25' (a fraction of the side) or 'id:t@440' (a canvas
// coordinate along the side) → a point on that side of a node or group.
function anchor(ref, boxes, toward) {
  const [id, spec = ''] = ref.split(':');
  const box = boxes.get(id);
  if (!box) throw new Error(`Unknown diagram element ${id}`);
  let [side, at] = spec.split('@');
  const t = at === undefined ? 0.5 : Number(at);
  const along = (start, size) => (t > 1 ? t : start + size * t);
  if (!side) {
    const dx = toward.x + toward.w / 2 - (box.x + box.w / 2);
    const dy = toward.y + toward.h / 2 - (box.y + box.h / 2);
    side = Math.abs(dx) > Math.abs(dy) ? (dx > 0 ? 'r' : 'l') : dy > 0 ? 'b' : 't';
  }
  const point = {
    t: [along(box.x, box.w), box.y],
    b: [along(box.x, box.w), box.y + box.h],
    l: [box.x, along(box.y, box.h)],
    r: [box.x + box.w, along(box.y, box.h)],
  }[side];
  return { point, vertical: side === 't' || side === 'b' };
}

function route(start, end, via) {
  if (via) return [start.point, ...via, end.point];
  const [[x1, y1], [x2, y2]] = [start.point, end.point];
  if (x1 === x2 || y1 === y2) return [start.point, end.point];
  if (start.vertical && end.vertical) {
    const my = (y1 + y2) / 2;
    return [start.point, [x1, my], [x2, my], end.point];
  }
  if (!start.vertical && !end.vertical) {
    const mx = (x1 + x2) / 2;
    return [start.point, [mx, y1], [mx, y2], end.point];
  }
  return [start.point, start.vertical ? [x1, y2] : [x2, y1], end.point];
}

// A polyline with rounded corners.
function pathFor(points) {
  let d = `M${n(points[0][0])},${n(points[0][1])}`;
  for (let i = 1; i < points.length - 1; i += 1) {
    const [px, py] = points[i - 1];
    const [cx, cy] = points[i];
    const [nx, ny] = points[i + 1];
    const into = Math.min(CORNER, Math.hypot(cx - px, cy - py) / 2);
    const out = Math.min(CORNER, Math.hypot(nx - cx, ny - cy) / 2);
    const [ux, uy] = [Math.sign(cx - px), Math.sign(cy - py)];
    const [vx, vy] = [Math.sign(nx - cx), Math.sign(ny - cy)];
    d += ` L${n(cx - ux * into)},${n(cy - uy * into)} Q${n(cx)},${n(cy)} ${n(cx + vx * out)},${n(cy + vy * out)}`;
  }
  const [lx, ly] = points.at(-1);
  return `${d} L${n(lx)},${n(ly)}`;
}

// Labels sit beside the longest segment unless the source places them.
function edgeLabel(points, label) {
  if (!label) return '';
  const spec = typeof label === 'string' ? { text: label } : label;
  let { at, anchor: align } = spec;
  if (!at) {
    const length = ([a, b]) => Math.hypot(b[0] - a[0], b[1] - a[1]);
    let best = [points[0], points[1]];
    for (let i = 1; i < points.length - 1; i += 1) {
      if (length([points[i], points[i + 1]]) > length(best)) best = [points[i], points[i + 1]];
    }
    const [[ax, ay], [bx, by]] = best;
    if (ax === bx) {
      at = [ax + 8, (ay + by) / 2 + 4];
      align ??= 'start';
    } else {
      at = [(ax + bx) / 2, ay - 8];
    }
  }
  return `<text class="edge-label" x="${n(at[0])}" y="${n(at[1])}" text-anchor="${align ?? 'middle'}">${esc(spec.text)}</text>`;
}

function drawEdge(edge, boxes) {
  const [fromId] = edge.from.split(':');
  const [toId] = edge.to.split(':');
  const start = anchor(edge.from, boxes, boxes.get(toId));
  const end = anchor(edge.to, boxes, boxes.get(fromId));
  const points = route(start, end, edge.via);
  const markers = `${edge.both ? ' marker-start="url(#arrow-start)"' : ''} marker-end="url(#arrow)"`;
  return `<path class="edge${edge.dashed ? ' dashed' : ''}" d="${pathFor(points)}"${markers}/>${edgeLabel(points, edge.label)}`;
}

function drawLegend(diagram) {
  const used = [...diagram.nodes, ...(diagram.groups ?? [])].map((item) => item.role ?? 'app');
  const y = diagram.height - 32;
  let x = 24;
  const items = Object.keys(LEGEND)
    .filter((role) => used.includes(role))
    .map((role) => {
      const item = `<g class="node ${role}"><rect x="${x}" y="${y - 10}" width="14" height="14" rx="${role === 'outside' ? 7 : 4}"/></g><text class="legend" x="${x + 22}" y="${y + 1}">${LEGEND[role]}</text>`;
      x += 22 + LEGEND[role].length * 6.2 + 28;
      return item;
    });
  const note = diagram.note
    ? `<text class="legend" x="${diagram.width - 24}" y="${y + 1}" text-anchor="end">${esc(diagram.note)}</text>`
    : '';
  return `<line class="rule" x1="24" x2="${diagram.width - 24}" y1="${y - 24}" y2="${y - 24}"/>${items.join('')}${note}`;
}

function style(p, font) {
  const role = (name) =>
    `.node.${name} rect, .node.${name} path { fill: ${p[name].fill}; stroke: ${p[name].stroke}; }\n.group.${name} .group-label { fill: ${p[name].label}; }`;
  return [
    `@font-face { font-family: 'Vesper Diagram'; font-weight: 400 600; src: url(data:font/woff2;base64,${font.toString('base64')}) format('woff2'); }`,
    `text { font-family: ${FONT_FAMILY}; fill: ${p.ink}; }`,
    `.frame { fill: ${p.bg}; stroke: ${p.frame}; }`,
    `.group rect { fill: ${p.group}; stroke: ${p.frame}; }`,
    '.group-label { font-size: 11px; font-weight: 600; letter-spacing: 0.08em; }',
    '.node rect, .node path { stroke-width: 1.25; }',
    ...['app', 'runtime', 'data', 'outside'].map(role),
    '.node.data .rim { fill: none; }',
    '.node.outside rect { stroke-dasharray: 4 3; }',
    `.title { font-size: ${TITLE_SIZE}px; font-weight: 600; text-anchor: middle; }`,
    `.detail { font-size: ${DETAIL_SIZE}px; font-weight: 400; fill: ${p.muted}; text-anchor: middle; }`,
    `.edge { fill: none; stroke: ${p.edge}; stroke-width: 1.5; }`,
    '.edge.dashed { stroke-dasharray: 5 4; }',
    `.arrowhead { fill: ${p.edge}; }`,
    `.edge-label, .legend { font-size: ${DETAIL_SIZE}px; font-weight: 400; fill: ${p.muted}; }`,
    `.rule { stroke: ${p.frame}; }`,
  ].join('\n');
}

export function render(name, diagram, mode, tokens, font) {
  const p = palette(tokens[mode]);
  const items = [...(diagram.groups ?? []), ...diagram.nodes];
  const boxes = new Map(items.map((item) => [item.id, item]));
  const { width, height } = diagram;
  return [
    `<!-- Generated by scripts/render-diagrams.mjs from docs/diagrams/${name}.mjs; do not edit. -->`,
    `<svg xmlns="http://www.w3.org/2000/svg" width="${width}" height="${height}" viewBox="0 0 ${width} ${height}" role="img" aria-labelledby="title">`,
    `<title id="title">${esc(diagram.title)}</title>`,
    `<style>\n${style(p, font)}\n</style>`,
    '<defs>',
    '<marker id="arrow" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="7" markerHeight="7" orient="auto"><path class="arrowhead" d="M0,1 L9,5 L0,9 z"/></marker>',
    '<marker id="arrow-start" viewBox="0 0 10 10" refX="1" refY="5" markerWidth="7" markerHeight="7" orient="auto"><path class="arrowhead" d="M10,1 L1,5 L10,9 z"/></marker>',
    '</defs>',
    `<rect class="frame" x="0.5" y="0.5" width="${width - 1}" height="${height - 1}" rx="24"/>`,
    ...(diagram.groups ?? []).map(drawGroup),
    ...diagram.edges.map((edge) => drawEdge(edge, boxes)),
    ...diagram.nodes.map(drawNode),
    drawLegend(diagram),
    '</svg>',
    '',
  ].join('\n');
}

export const sourceNames = () =>
  readdirSync(SOURCES)
    .filter((file) => file.endsWith('.mjs'))
    .map((file) => basename(file, '.mjs'))
    .sort();

export async function renderAll(names = sourceNames()) {
  const tokens = readTokens(readFileSync(TOKENS, 'utf8'));
  const font = readFileSync(FONT);
  const out = {};
  for (const name of names) {
    const { default: diagram } = await import(pathToFileURL(join(SOURCES, `${name}.mjs`)).href);
    for (const mode of ['light', 'dark']) {
      out[`${name}.${mode}.svg`] = render(name, diagram, mode, tokens, font);
    }
  }
  return out;
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const names = process.argv.slice(2);
  for (const [file, svg] of Object.entries(await renderAll(names.length ? names : undefined))) {
    writeFileSync(join(OUTPUT, file), svg);
    console.log(`Rendered ${file}`);
  }
}
