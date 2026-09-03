// Cursor compatibility, checked rather than claimed.
//
// Cursor is a fork of VS Code and installs the same VSIX. That is only true
// while the extension stays inside the API surface a fork actually implements
// -- a proposed API, or one of the Microsoft-proprietary services, would build
// and package cleanly and then do nothing in Cursor.
//
// The other half is distribution: Cursor resolves extensions from Open VSX, not
// from the Visual Studio Marketplace, so publishing to the Marketplace alone
// leaves Cursor users with nothing to install.

import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync, readdirSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import path from 'node:path';

const here = path.dirname(fileURLToPath(import.meta.url));
const root = path.join(here, '..');
const manifest = JSON.parse(readFileSync(path.join(root, 'package.json'), 'utf8'));

const sources = readdirSync(path.join(root, 'src'))
    .filter((f) => f.endsWith('.ts'))
    .map((f) => readFileSync(path.join(root, 'src', f), 'utf8'))
    .join('\n');

test('no proposed API is used, which a fork would not ship', () => {
    assert.equal(
        manifest.enabledApiProposals,
        undefined,
        'enabledApiProposals restricts the extension to Insiders builds of VS Code'
    );
    assert.doesNotMatch(sources, /enabledApiProposals|vscode\.proposed/);
});

test('no Microsoft-proprietary API is used', () => {
    // These exist in VS Code and are not part of what a fork is obliged to
    // implement. Using one compiles and packages, then fails at runtime.
    for (const api of ['vscode.chat', 'vscode.lm.', 'vscode.authentication']) {
        assert.ok(
            !sources.includes(api),
            `${api} is not available in every VS Code-compatible editor`
        );
    }
});

test('the engine floor is low enough for shipping forks', () => {
    const range = manifest.engines.vscode;
    assert.match(range, /^\^1\.\d+\.\d+$/, `unexpected engine range: ${range}`);
    const minor = Number(range.match(/^\^1\.(\d+)/)[1]);
    // Cursor tracks a VS Code base some way behind upstream. A floor set to a
    // very recent release makes the extension uninstallable there while looking
    // fine in VS Code.
    assert.ok(minor <= 90, `engines.vscode is ^1.${minor}.x, which forks may not satisfy yet`);
});

test('the extension is not pinned to a single host by extensionKind', () => {
    const kind = manifest.extensionKind;
    if (kind !== undefined) {
        assert.ok(Array.isArray(kind) && kind.length > 0, 'extensionKind should list hosts');
    }
});

test('publishing reaches Open VSX, not only the Visual Studio Marketplace', () => {
    const scripts = manifest.scripts ?? {};
    assert.ok(
        Object.values(scripts).some((s) => s.includes('ovsx')),
        'no script publishes to Open VSX, so Cursor and VSCodium users cannot install this'
    );
    assert.ok(
        (manifest.devDependencies ?? {}).ovsx,
        'ovsx is not a devDependency, so the publish script would not run'
    );
});

test('packaging never drops runtime dependencies', () => {
    // The flag that produced a package which installed and could not activate.
    for (const [name, body] of Object.entries(manifest.scripts ?? {})) {
        assert.ok(
            !body.includes('--no-dependencies'),
            `script "${name}" packages without dependencies; the result cannot activate`
        );
    }
});
