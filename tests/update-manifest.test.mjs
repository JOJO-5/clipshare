import assert from 'node:assert/strict';
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { test } from 'node:test';
import { buildManifest } from '../scripts/update-manifest.mjs';

const files = [
  'ClipShare.Win7_0.1.4_x64-setup.exe',
  'ClipShare_0.1.4_x64-setup.exe',
  'ClipShare_0.1.4_x64_en-US.msi',
];

function fixture(t, names) {
  const directory = mkdtempSync(join(tmpdir(), 'clipshare-update-'));
  t.after(() => rmSync(directory, { recursive: true, force: true }));
  for (const [index, name] of names.entries()) {
    writeFileSync(join(directory, name), 'installer');
    writeFileSync(join(directory, name + '.sig'), Buffer.from('signature-' + index).toString('base64'));
  }
  return directory;
}

test('the three Windows targets get their own installer and signature', t => {
  const directory = fixture(t, files);
  const manifest = buildManifest(directory, 'JOJO-5/clipshare', 'v0.1.4', 'Release notes');
  assert.equal(manifest.version, '0.1.4');
  assert.deepEqual(Object.keys(manifest.platforms), ['windows-win7-x86_64', 'windows-x86_64-nsis', 'windows-x86_64-msi']);
  for (const [index, update] of Object.values(manifest.platforms).entries()) {
    assert.equal(update.url, 'https://github.com/JOJO-5/clipshare/releases/download/v0.1.4/' + files[index]);
    assert.equal(Buffer.from(update.signature, 'base64').toString(), 'signature-' + index);
  }
});

test('stable manifests require signatures for all Windows installers', t => {
  const directory = fixture(t, files);
  rmSync(join(directory, files[0] + '.sig'));
  assert.throws(() => buildManifest(directory, 'JOJO-5/clipshare', 'v0.1.4'), /Win7/);
});

test('preview manifests are bound to their tag and can omit MSI', t => {
  const directory = fixture(t, files.slice(0, 2));
  const manifest = buildManifest(directory, 'JOJO-5/clipshare', 'v0.1.4-updater-test.1');
  assert.match(manifest.platforms['windows-x86_64-nsis'].url, /\/v0\.1\.4-updater-test\.1\//);
  assert.ok(!Object.hasOwn(manifest.platforms, 'windows-x86_64-msi'));
});
