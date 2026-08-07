#!/usr/bin/env node
'use strict';

const { execSync } = require('child_process');
const fs = require('fs');

function readStdin() {
  try {
    return fs.readFileSync(0, 'utf8');
  } catch (e) {
    return '';
  }
}

let data = {};
try {
  data = JSON.parse(readStdin());
} catch (e) {
  data = {};
}

const NERD = process.env.CLAUDE_STATUSLINE_NERDFONT === '1';

const RESET = '\x1b[0m';
const DIM = '\x1b[2m';
const GREEN = '\x1b[32m';
const YELLOW = '\x1b[33m';
const RED = '\x1b[31m';
const GRAY = '\x1b[90m';
const SEP = `${DIM}|${RESET}`;

function clamp(n, lo, hi) {
  return Math.min(hi, Math.max(lo, n));
}

function fmtPct(pct) {
  return pct == null ? 'n/a' : `${Math.round(pct)}%`;
}

function buildBar(width, usedPct, paceIdx) {
  const fillChar = NERD ? '█' : '#';
  const emptyChar = NERD ? '░' : '-';
  const filled = usedPct == null ? 0 : Math.round(clamp(usedPct, 0, 100) / 100 * width);
  const chars = [];
  for (let i = 0; i < width; i++) chars.push(i < filled ? fillChar : emptyChar);
  if (paceIdx != null) chars[clamp(paceIdx, 0, width - 1)] = '|';
  return chars.join('');
}

function colorForPace(usedPct, elapsedPct) {
  if (usedPct == null || elapsedPct == null) return GRAY;
  const diff = usedPct - elapsedPct;
  if (diff <= 0) return GREEN;
  if (diff <= 15) return YELLOW;
  return RED;
}

function colorForContext(pct) {
  if (pct == null) return GRAY;
  if (pct < 50) return GREEN;
  if (pct <= 80) return YELLOW;
  return RED;
}

// ---- Model ----
const modelName = (data.model && data.model.display_name) || 'unknown';
const modelIcon = NERD ? ' ' : '';
const modelSeg = `${modelIcon}${modelName}`;

// ---- Git branch (name + dirty marker) ----
let gitSeg = null;
const cwd = (data.workspace && data.workspace.current_dir) || data.cwd || process.cwd();
try {
  const out = execSync('git status --porcelain -b', { cwd, stdio: ['ignore', 'pipe', 'ignore'] }).toString();
  const lines = out.split('\n').filter(Boolean);
  if (lines.length > 0 && lines[0].startsWith('##')) {
    const header = lines[0].replace(/^##\s*/, '');
    const noCommitMatch = header.match(/^No commits yet on (.+)$/);
    let branch = noCommitMatch ? noCommitMatch[1] : header.split('...')[0].trim();
    const dirty = lines.length > 1;
    if (branch.startsWith('HEAD')) {
      try {
        branch = execSync('git rev-parse --short HEAD', { cwd, stdio: ['ignore', 'pipe', 'ignore'] }).toString().trim();
      } catch (e) {
        // leave branch as-is if rev-parse fails
      }
    }
    const gitIcon = NERD ? ' ' : '';
    gitSeg = `${gitIcon}${branch}${dirty ? '*' : ''}`;
  }
} catch (e) {
  gitSeg = null;
}

// ---- Context window bar ----
const cw = data.context_window || {};
const ctxPct = cw.used_percentage == null ? null : cw.used_percentage;
const ctxColor = colorForContext(ctxPct);
const ctxIcon = NERD ? ' ' : 'ctx ';
const ctxSeg = `${ctxIcon}${ctxColor}[${buildBar(10, ctxPct, null)}]${RESET} ${fmtPct(ctxPct)}`;

// ---- Rate-limit windows (5h / 7d) with pace marker ----
const nowSec = Date.now() / 1000;

function windowSeg(rl, totalSeconds, icon, label) {
  const iconStr = NERD ? `${icon} ` : `${label} `;
  if (!rl || rl.used_percentage == null) {
    return { seg: `${iconStr}${GRAY}[${buildBar(10, null, null)}]${RESET} n/a`, usedPct: null, elapsedPct: null };
  }
  const usedPct = rl.used_percentage;
  let elapsedPct = null;
  let paceIdx = null;
  if (rl.resets_at != null) {
    const elapsedSec = totalSeconds - (rl.resets_at - nowSec);
    elapsedPct = clamp((elapsedSec / totalSeconds) * 100, 0, 100);
    paceIdx = Math.round((elapsedPct / 100) * 10);
  }
  const color = colorForPace(usedPct, elapsedPct);
  const bar = buildBar(10, usedPct, paceIdx);
  return { seg: `${iconStr}${color}[${bar}]${RESET} ${fmtPct(usedPct)}`, usedPct, elapsedPct };
}

const rl = data.rate_limits || {};
const fiveH = windowSeg(rl.five_hour, 18000, '', '5h');
const sevenD = windowSeg(rl.seven_day, 604800, '', '7d');

// ---- Pet mood: worst of context% and how far 5h usage is ahead of pace ----
const fiveHAhead = fiveH.usedPct != null && fiveH.elapsedPct != null ? Math.max(0, fiveH.usedPct - fiveH.elapsedPct) : 0;
const worst = Math.max(ctxPct || 0, fiveHAhead);
let pet;
if (worst < 15) pet = ':)';
else if (worst < 40) pet = ':/';
else if (worst < 70) pet = '>:(';
else pet = 'X_X';

// ---- Assemble ----
const segments = [modelSeg];
if (gitSeg) segments.push(gitSeg);
segments.push(ctxSeg, fiveH.seg, sevenD.seg, pet);

console.log(segments.join(` ${SEP} `));
