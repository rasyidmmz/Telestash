// One-shot: find unused i18n keys by scanning t()/i18n.t() usage in app/src.
// Handles: static keys, template-literal dynamic prefixes, i18next plural suffixes.
import { readFileSync, readdirSync, statSync } from 'node:fs';
import { join, extname } from 'node:path';

const SRC = 'src';
const LOCALES = 'src/i18n/locales';
const REF = 'en.json';

const files = [];
(function walk(dir) {
  for (const name of readdirSync(dir)) {
    const p = join(dir, name);
    if (statSync(p).isDirectory()) walk(p);
    else if (/\.(ts|tsx)$/.test(extname(p))) files.push(p);
  }
})(SRC);

const code = files.map((f) => readFileSync(f, 'utf8')).join('\n');

// 1. Static keys: t('a.b'), i18n.t("a.b"), t('a.b', {...})
const usedKeys = new Set();
for (const m of code.matchAll(/(?:i18n\.)?\bt\(\s*['"]([^'"]+)['"]/g)) usedKeys.add(m[1]);

// 2. Dynamic prefixes: t(`a.b_${...}`) → prefix "a.b_"
const usedPrefixes = [];
for (const m of code.matchAll(/(?:i18n\.)?\bt\(\s*`([^`$]*)\$\{/g)) usedPrefixes.push(m[1]);

// Flatten reference locale into dot keys.
const flat = [];
(function flatten(obj, prefix) {
  for (const [k, v] of Object.entries(obj)) {
    const key = prefix ? `${prefix}.${k}` : k;
    if (v && typeof v === 'object' && !Array.isArray(v)) flatten(v, key);
    else flat.push(key);
  }
})(JSON.parse(readFileSync(join(LOCALES, REF), 'utf8')), '');

const isUsed = (key) => {
  if (usedKeys.has(key)) return true;
  // plural suffixes: base used with count → key_one/key_other used
  const base = key.replace(/_(one|other|zero|two|few|many)$/, '');
  if (base !== key && usedKeys.has(base)) return true;
  // dynamic prefix match
  if (usedPrefixes.some((p) => key.startsWith(p))) return true;
  return false;
};

const unused = flat.filter((k) => !isUsed(k));
console.log(`total keys: ${flat.length}, used static: ${usedKeys.size}, dynamic prefixes: ${usedPrefixes.join(', ') || 'none'}`);
console.log(`UNUSED (${unused.length}):`);
for (const k of unused) console.log(`  ${k}`);
