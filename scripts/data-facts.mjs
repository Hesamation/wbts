#!/usr/bin/env node
// Local dataset tooling. No network, signing, deployment or payment operations.
import { randomBytes } from 'node:crypto';
import { existsSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { buildTree, leafHash, slotOf } from '../data-escrow/compat/merkle.ts';

const scheme = 'inferpay-keyed-merkle-sha256-v1';
function checkKey(key) {
  if (typeof key !== 'string' || !/^[a-z0-9_.-]{1,80}$/.test(key)) throw Error('fact keys must be lowercase ASCII slugs, 1–80 bytes');
}
function fact(raw) {
  if (typeof raw.topic !== 'string' || !/^[\x00-\x7f]*$/.test(raw.topic)) throw Error('fact topic must be ASCII');
  checkKey(raw.topic.toLowerCase());
  if (typeof raw.value !== 'string' || Buffer.byteLength(raw.value) > 512) throw Error('fact value must be a string up to 512 UTF-8 bytes');
  if (typeof raw.source !== 'string' || Buffer.byteLength(raw.source) > 1024) throw Error('fact source must be a string up to 1024 UTF-8 bytes');
  if (raw.kind !== undefined && raw.kind !== 'fact') throw Error('only string-valued fact entries are supported');
  return { kind: 'fact', topic: raw.topic, value: raw.value, source: raw.source };
}
function makeTree(rows) {
  if (!Array.isArray(rows) || !rows.length || rows.length > 10000) throw Error('dataset requires 1–10000 entries');
  const leaves = new Map();
  for (const { entry, salt } of rows) {
    fact(entry);
    if (!/^[0-9a-f]{64}$/.test(salt)) throw Error('invalid stored salt');
    const slot = slotOf(['fact', entry.topic.toLowerCase()]);
    if (leaves.has(slot)) throw Error('duplicate topic or 32-bit key-slot collision');
    leaves.set(slot, leafHash(entry, salt));
  }
  return buildTree(leaves);
}
function writeNew(path, value, mode = 0o600) {
  writeFileSync(path, `${JSON.stringify(value, null, 2)}\n`, { flag: 'wx', mode });
}
try {
  const [command, input, target, destination] = process.argv.slice(2);
  if (command === 'seal' && input && target && !destination) {
    const raw = JSON.parse(readFileSync(input, 'utf8'));
    if (!Array.isArray(raw)) throw Error('input must be an array of {topic, value, source}');
    const rows = raw.map((entry) => ({ entry: fact(entry), salt: randomBytes(32).toString('hex') }));
    const tree = makeTree(rows);
    const output = resolve(target);
    if (existsSync(join(output, 'sealed.json')) || existsSync(join(output, 'catalog.json'))) throw Error('output already contains a dataset; use a new directory to preserve existing commitments');
    mkdirSync(output, { recursive: true, mode: 0o700 });
    writeNew(join(output, 'sealed.json'), { scheme, root: tree.root, rows });
    writeNew(join(output, 'catalog.json'), { scheme, root: tree.root, keys: rows.map((row) => row.entry.topic.toLowerCase()) }, 0o644);
    console.log(JSON.stringify({ root: tree.root, facts: rows.length, catalog: join(output, 'catalog.json'), private_dataset: join(output, 'sealed.json'), notice: 'Keep sealed.json private. Delivering an opening to this contract publishes that entry and its proof.' }, null, 2));
  } else if (command === 'prove' && input && target && destination) {
    checkKey(target);
    const sealed = JSON.parse(readFileSync(input, 'utf8'));
    if (sealed.scheme !== scheme) throw Error('unsupported dataset scheme');
    const tree = makeTree(sealed.rows);
    if (tree.root !== sealed.root) throw Error('stored dataset does not match its root');
    const row = sealed.rows.find(({ entry }) => entry.topic.toLowerCase() === target);
    const slot = slotOf(['fact', target]);
    if (!row && sealed.rows.some(({ entry }) => slotOf(['fact', entry.topic.toLowerCase()]) === slot)) throw Error('cannot prove vacancy: another key occupies this slot');
    const opening = row ? { entry: row.entry, salt: row.salt, path: tree.path(slot) } : { entry: null, salt: null, path: tree.path(slot) };
    writeNew(destination, opening);
    console.log(JSON.stringify({ key: target, present: Boolean(row), proof: resolve(destination), root: tree.root }, null, 2));
  } else {
    console.error('Usage:\n  node scripts/data-facts.mjs seal facts.json .wbts-data/seller\n  node scripts/data-facts.mjs prove .wbts-data/seller/sealed.json requested-key .wbts-data/opening.json');
    process.exitCode = 2;
  }
} catch (error) { console.error(`Stopped: ${error.message}`); process.exitCode = 1; }
