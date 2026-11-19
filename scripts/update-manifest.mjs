import { existsSync, readFileSync, writeFileSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

export function buildManifest(directory, repository, tag, notes = '') {
  const versionMatch = /^v(\d+\.\d+\.\d+)(?:-([0-9A-Za-z.-]+))?$/.exec(tag);
  if (!versionMatch || !/^[\w.-]+\/[\w.-]+$/.test(repository)) {
    throw new Error('Expected a version tag and a GitHub owner/repository.');
  }
  const [, version, prerelease] = versionMatch;
  const platforms = {};
  const assets = [
    ['windows-win7-x86_64', `ClipShare.Win7_${version}_x64-setup.exe`],
    ['windows-x86_64-nsis', `ClipShare_${version}_x64-setup.exe`],
    ['windows-x86_64-msi', `ClipShare_${version}_x64_en-US.msi`],
  ];
  for (const [target, filename] of assets) {
    if (!existsSync(join(directory, filename))) {
      if (prerelease && target === 'windows-x86_64-msi') continue;
      throw new Error(`Missing update installer ${filename}`);
    }
    const signatureFilename = `${filename}.sig`;
    if (!existsSync(join(directory, signatureFilename))) throw new Error(`Missing signature ${signatureFilename}`);
    const signature = readFileSync(join(directory, signatureFilename), 'utf8').trim();
    if (!/^[A-Za-z0-9+/]+={0,2}$/.test(signature)) throw new Error(`Invalid signature ${signatureFilename}`);
    platforms[target] = {
      url: `https://github.com/${repository}/releases/download/${tag}/${filename}`,
      signature,
    };
  }
  return { version, notes, platforms };
}

if (process.argv[1] && fileURLToPath(import.meta.url) === resolve(process.argv[1])) {
  const [, , directory, repository, tag, notesFile] = process.argv;
  const notes = notesFile ? readFileSync(notesFile, 'utf8').trim() : '';
  const manifest = buildManifest(directory, repository, tag, notes);
  writeFileSync(join(directory, 'latest.json'), JSON.stringify(manifest, null, 2) + '\n');
}
