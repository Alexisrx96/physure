// Runs the JavaScript that `phs transpile --target js` writes (tests/golden/js/NAME.mjs) against this package
// and compares what it prints with NAME.out, the interpreter's answers (audit T19). The Rust test
// physure-script/tests/js_golden.rs keeps NAME.mjs and NAME.out equal to the generator and the interpreter.
//
// Build the package first: `wasm-pack build --target nodejs --out-dir pkg-node`, then `node --test tests/js`.
//
// Each module is fed to Node on stdin with this package's directory as the working directory: the generated
// `import ... from "physure"` then resolves to this package by its own name (a package's self-reference through
// its "exports"), with the "node" condition, so to pkg-node.
import { test } from "node:test";
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { readdirSync, readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const packageDir = join(dirname(fileURLToPath(import.meta.url)), "..", "..");
const goldenDir = join(packageDir, "..", "tests", "golden", "js");
const lf = (text) => text.replace(/\r\n/g, "\n");

const modules = readdirSync(goldenDir).filter((name) => name.endsWith(".mjs")).sort();

test("the golden directory holds generated modules", () => {
  assert.ok(modules.length >= 8, `found ${modules.length} in ${goldenDir}`);
});

for (const name of modules) {
  test(`${name} prints what the interpreter answers`, () => {
    const run = spawnSync(process.execPath, ["--input-type=module"], {
      cwd: packageDir,
      input: readFileSync(join(goldenDir, name), "utf8"),
      encoding: "utf8",
    });
    assert.equal(run.status, 0, `${name} failed:\n${run.stderr}`);
    const expected = readFileSync(join(goldenDir, name.replace(/\.mjs$/, ".out")), "utf8");
    assert.equal(lf(run.stdout), lf(expected));
  });
}
