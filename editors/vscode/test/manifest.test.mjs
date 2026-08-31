// The manifest must agree with the code and with the filesystem.
//
// `package.json` is a set of promises to VS Code: these commands exist, these
// settings exist, these files are here. Nothing enforces any of them. A
// command in a menu that nothing registers is a greyed-out entry; a setting
// read by the extension but undeclared reads as `undefined` with no warning;
// a missing file is silent until a user notices the icon never appears — which
// is exactly what happened to `./icons/ae-light.svg`, referenced for four
// releases without ever existing.

import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync, existsSync, readdirSync } from "node:fs";
import { join } from "node:path";
import { extRoot } from "./tokenize.mjs";

const pkg = JSON.parse(readFileSync(join(extRoot, "package.json"), "utf8"));
const srcFiles = readdirSync(join(extRoot, "src")).filter((f) => f.endsWith(".ts"));
const srcText = srcFiles
  .map((f) => readFileSync(join(extRoot, "src", f), "utf8"))
  .join("\n");

const declaredCommands = new Set(
  (pkg.contributes?.commands ?? []).map((c) => c.command),
);

test("every command referenced by a menu or keybinding is declared", () => {
  const referenced = new Set();
  for (const group of Object.values(pkg.contributes?.menus ?? {})) {
    for (const item of group) if (item.command) referenced.add(item.command);
  }
  for (const kb of pkg.contributes?.keybindings ?? []) {
    if (kb.command) referenced.add(kb.command);
  }
  assert.ok(referenced.size > 0, "no menu or keybinding references any command");

  const undeclared = [...referenced].filter((c) => !declaredCommands.has(c));
  assert.deepEqual(
    undeclared,
    [],
    `menus/keybindings point at commands that contributes.commands does not \
declare — they render greyed out: ${undeclared.join(", ")}`,
  );
});

test("every declared command is actually registered in the source", () => {
  const unregistered = [...declaredCommands].filter(
    (c) => !srcText.includes(`"${c}"`) && !srcText.includes(`'${c}'`),
  );
  assert.deepEqual(
    unregistered,
    [],
    `these commands appear in the palette but nothing registers a handler, so \
invoking them raises "command not found": ${unregistered.join(", ")}`,
  );
});

test("every command the source registers is declared in the manifest", () => {
  const registered = new Set();
  const re = /registerCommand\(\s*["'`]([^"'`]+)["'`]/g;
  for (const m of srcText.matchAll(re)) registered.add(m[1]);
  assert.ok(registered.size > 0, "no registerCommand calls found in src/");

  const hidden = [...registered].filter((c) => !declaredCommands.has(c));
  assert.deepEqual(
    hidden,
    [],
    `these commands are registered but undeclared, so they never appear in the \
palette: ${hidden.join(", ")}`,
  );
});

test("every setting the source reads is declared", () => {
  const declared = new Set(
    Object.keys(pkg.contributes?.configuration?.properties ?? {}),
  );
  assert.ok(declared.size > 0, "no configuration properties declared");

  // `config.get("lsp.path")` after `getConfiguration("aethershell")`.
  const read = new Set();
  for (const m of srcText.matchAll(/\.get(?:<[^>]*>)?\(\s*["'`]([^"'`]+)["'`]/g)) {
    read.add(m[1]);
  }
  assert.ok(
    read.size > 0,
    "no configuration reads found in src/ — the regex stopped matching and this test would pass no matter what the manifest declared",
  );

  const undeclared = [...read].filter(
    (k) => !declared.has(k) && !declared.has(`aethershell.${k}`),
  );
  assert.deepEqual(
    undeclared,
    [],
    `read by the extension but not declared, so they are invisible in Settings \
and always undefined: ${undeclared.join(", ")}`,
  );
});

test("declared settings use the extension's own namespace", () => {
  for (const key of Object.keys(
    pkg.contributes?.configuration?.properties ?? {},
  )) {
    assert.ok(
      key.startsWith("aethershell."),
      `setting "${key}" is outside the aethershell.* namespace`,
    );
  }
});

test("every relative path in the manifest exists", () => {
  const refs = [];
  JSON.stringify(pkg).replace(/"(\.\/[^"]+)"/g, (_, p) => refs.push(p));
  assert.ok(refs.length >= 5, `only ${refs.length} relative paths found`);

  // ./out/** is build output, produced by `npm run compile`.
  const missing = refs.filter(
    (r) => !r.startsWith("./out/") && !existsSync(join(extRoot, r)),
  );
  assert.deepEqual(missing, [], `manifest points at missing files: ${missing}`);
});

test("the language contribution is self-consistent", () => {
  const langs = pkg.contributes?.languages ?? [];
  const ae = langs.find((l) => l.id === "aethershell");
  assert.ok(ae, "no language with id 'aethershell'");
  assert.ok(ae.extensions?.includes(".ae"), "language does not claim .ae");

  for (const g of pkg.contributes?.grammars ?? []) {
    if (g.language) {
      assert.ok(
        langs.some((l) => l.id === g.language),
        `grammar declares language "${g.language}" which is not contributed`,
      );
    }
  }

  for (const s of pkg.contributes?.snippets ?? []) {
    assert.ok(
      langs.some((l) => l.id === s.language),
      `snippets declare language "${s.language}" which is not contributed`,
    );
  }
});

test("activation events cover the contributed language", () => {
  const events = pkg.activationEvents ?? [];
  const commandsActivate = declaredCommands.size > 0;
  assert.ok(
    events.includes("onLanguage:aethershell"),
    "the extension does not activate on its own language",
  );
  // VS Code activates on declared commands implicitly since 1.74, so a command
  // needs no explicit activationEvent; this only guards the language one.
  assert.ok(commandsActivate);
});

test("the grammar files the manifest names parse as JSON", () => {
  for (const g of pkg.contributes?.grammars ?? []) {
    const p = join(extRoot, g.path);
    const parsed = JSON.parse(readFileSync(p, "utf8"));
    assert.equal(
      parsed.scopeName,
      g.scopeName,
      `${g.path}: manifest says scopeName ${g.scopeName}, file says ${parsed.scopeName}`,
    );
  }
});
