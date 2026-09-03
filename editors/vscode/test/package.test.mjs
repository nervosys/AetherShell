// What ships must be able to start.
//
// The 1.6.0 package was first built with `vsce package --no-dependencies`,
// which produced 18 files and 38 KB. It installed without complaint and could
// not activate: `extension.ts` imports `vscode-languageclient/node` at the top
// level, that module was not in the package, and the editor got
// MODULE_NOT_FOUND the moment an AetherShell file was opened.
//
// Nothing catches that by inspection -- the manifest was right, the compile was
// clean, the install succeeded. Only loading the packaged output finds it.

import { test } from 'node:test';
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { readFileSync, existsSync, mkdtempSync, rmSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { tmpdir } from 'node:os';
import { createRequire, builtinModules } from 'node:module';
import path from 'node:path';

const require = createRequire(import.meta.url);
const here = path.dirname(fileURLToPath(import.meta.url));
const root = path.join(here, '..');

const manifest = JSON.parse(readFileSync(path.join(root, 'package.json'), 'utf8'));
const vsix = path.join(root, `aethershell-${manifest.version}.vsix`);

const havePackage = existsSync(vsix);

// These three shell out to `vsce`, which needs npx on PATH. A test that cannot
// run should say why rather than report a failure that is about the machine
// instead of the code.
let haveNpx = true;
try {
    execFileSync('npx', ['--version'], { stdio: 'ignore', shell: true });
} catch {
    haveNpx = false;
}

// Needs the package on disk.
const skipNoPackage = havePackage
    ? false
    : `no ${path.basename(vsix)}; run \`npm run package\` first`;

// Needs the package *and* vsce, for the tests that read its listing.
const skip = skipNoPackage || (haveNpx ? false : 'npx is not on PATH, so vsce cannot be invoked');

test('every runtime dependency the code imports is declared', () => {
    // A dependency used but not declared is not packaged either, with the same
    // result as not bundling it.
    const srcDir = path.join(root, 'src');
    const sources = readFileSync(path.join(srcDir, 'extension.ts'), 'utf8');
    const imported = [...sources.matchAll(/from '([^'.][^']*)'/g)]
        .map((m) => m[1])
        .filter((m) => m !== 'vscode')
        // Node builtins are not packages and are never declared.
        .filter((m) => !builtinModules.includes(m.replace(/^node:/, '').split('/')[0]))
        .map((m) => (m.startsWith('@') ? m.split('/').slice(0, 2).join('/') : m.split('/')[0]));

    const declared = new Set(Object.keys(manifest.dependencies ?? {}));
    for (const dep of new Set(imported)) {
        assert.ok(
            declared.has(dep),
            `extension.ts imports "${dep}" but package.json does not declare it`
        );
    }
});

test('the packaged vsix contains its declared runtime dependencies', { skip }, () => {
    const listing = execFileSync('npx', ['vsce', 'ls', '--tree'], {
        cwd: root, encoding: 'utf8', shell: true,
    });
    for (const dep of Object.keys(manifest.dependencies ?? {})) {
        assert.ok(
            listing.includes(dep),
            `"${dep}" is declared but not packaged; the extension will fail to activate ` +
                'with MODULE_NOT_FOUND. Do not package with --no-dependencies.'
        );
    }
});

test('the packaged extension carries each dependency as a resolvable module', { skip }, () => {
    // A module resolves when its directory and its package.json are present.
    // Checked against the package listing rather than by unpacking: a .vsix is
    // a zip, and the unpackers available vary by machine.
    const listing = execFileSync('npx', ['vsce', 'ls'], {
        cwd: root, encoding: 'utf8', shell: true,
    });
    // vsce prints native separators, so compare on a normalised copy rather
    // than trying to match both forms.
    const normalised = listing.replace(/\\/g, '/');
    for (const dep of Object.keys(manifest.dependencies ?? {})) {
        assert.ok(
            normalised.includes(`node_modules/${dep}/package.json`),
            `"${dep}" has no package.json in the package, so it cannot resolve`
        );
    }
});

test('the package is not implausibly small', { skip: skipNoPackage }, () => {
    const { size } = require('node:fs').statSync(vsix);
    assert.ok(
        size > 100 * 1024,
        `the package is ${Math.round(size / 1024)} KB. The build that could not ` +
            'activate was 38 KB, because it carried no dependencies.'
    );
});

test('the entry point named by the manifest is the one that was built', { skip }, () => {
    const listing = execFileSync('npx', ['vsce', 'ls'], { cwd: root, encoding: 'utf8', shell: true });
    const main = manifest.main.replace(/^\.\//, '');
    assert.ok(listing.includes(main), `the manifest points at ${main}, which is not packaged`);
});
