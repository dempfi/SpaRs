const { test } = require('node:test');
const assert = require('node:assert/strict');
const { execFileSync, spawnSync } = require('node:child_process');
const { mkdtempSync, writeFileSync, rmSync, readFileSync } = require('node:fs');
const { join } = require('node:path');
const { tmpdir } = require('node:os');
const { classify, gate, SELECTED_BY } = require('./ci-changes.cjs');

const FULL = ['ubuntu-latest', 'ubuntu-24.04-arm', 'macos-latest'];
const NOTHING = { code: false, native: false, os: [] };
const LINUX = { code: true, native: false, os: ['ubuntu-latest'] };
const ALL = { code: true, native: true, os: FULL };

test('only files that no job or test reads run nothing', () => {
  assert.deepEqual(classify(['.github/dependabot.yml', '.claude/settings.json']), NOTHING);
  assert.deepEqual(classify([]), NOTHING);
});

test('ignored files are not read by any test or checker', () => {
  const search = spawnSync('git', ['grep', '-l', '--untracked', '-E', 'dependabot|\\.claude/', '--',
    '.github/scripts', 'tools', 'bindings/node', 'crates', 'consumer', ':!*/node_modules/*'], { encoding: 'utf8' });
  assert.ok(search.status === 0 || search.status === 1, search.stderr);
  const readers = search.stdout.split('\n').filter(path => path && !path.startsWith('.github/scripts/ci-changes'));
  assert.deepEqual(readers, []);
});

test('guides run only the Linux acceptance job, which executes their examples', () => {
  for (const path of ['README.md', 'AGENTS.md', 'docs/COMPATIBILITY.md', 'reference/README.md',
    'fixtures/README.md', '.github/pull_request_template.md', '.codex/agents/docs_review.toml']) {
    assert.deepEqual(classify([path]), LINUX, path);
  }
  assert.deepEqual(classify(['docs/guide.md', '.github/dependabot.yml']), LINUX);
});

test('Markdown inside packages and nested reference folders needs the full matrix', () => {
  for (const path of ['bindings/node/README.md', 'crates/spars/README.md', 'reference/nested/notes.md']) {
    assert.deepEqual(classify([path]), ALL, path);
  }
});

test('source, lockfiles, fixtures, workflows and unknown paths need the full matrix', () => {
  for (const path of ['crates/spars/src/lib.rs', 'Cargo.lock', 'bindings/node/package.json', 'fixtures/hash.expected.json',
    'models/catalog.json', 'tools/verify.py', 'tools/release/Cargo.toml', '.github/workflows/ci.yml',
    '.github/workflows/pr-title.yml', '.github/workflows/release.yml', '.github/scripts/ci-changes.cjs',
    'LICENSE', 'CHANGELOG.txt', 'new-top-level-dir/file']) {
    assert.deepEqual(classify([path]), ALL, path);
  }
  assert.deepEqual(classify(['docs/guide.md', '.github/dependabot.yml', 'crates/spars/src/lib.rs']), ALL);
});

test('the command line writes GitHub outputs and runs everything without a listing', () => {
  const script = join(__dirname, 'ci-changes.cjs');
  const dir = mkdtempSync(join(tmpdir(), 'ci-changes-'));
  try {
    const listing = join(dir, 'changed.txt');
    writeFileSync(listing, 'docs/guide.md\n\n');
    assert.equal(execFileSync(process.execPath, [script, listing], { encoding: 'utf8' }),
      'code=true\nnative=false\nos=["ubuntu-latest"]\n');
    assert.equal(execFileSync(process.execPath, [script], { encoding: 'utf8' }),
      `code=true\nnative=true\nos=${JSON.stringify(FULL)}\n`);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});

function needs(code, native, results) {
  return {
    changes: { result: 'success', outputs: { code, native } },
    'reference-and-rust': { result: results[0] },
    'release-tooling': { result: results[1] },
    'native-installation': { result: results[2] },
  };
}

test('the gate passes when selected jobs succeed and unselected jobs are skipped', () => {
  assert.deepEqual(gate(needs('true', 'true', ['success', 'success', 'success'])), []);
  assert.deepEqual(gate(needs('true', 'false', ['success', 'success', 'skipped'])), []);
  assert.deepEqual(gate(needs('false', 'false', ['skipped', 'skipped', 'skipped'])), []);
});

test('the gate fails on failed, cancelled or unexpectedly skipped jobs', () => {
  for (const results of [['failure', 'success', 'success'], ['success', 'cancelled', 'success'],
    ['success', 'success', 'skipped'], ['skipped', 'success', 'success']]) {
    assert.notDeepEqual(gate(needs('true', 'true', results)), [], results.join(' '));
  }
  assert.notDeepEqual(gate(needs('false', 'false', ['success', 'skipped', 'skipped'])), []);
});

test('the gate fails when the job selection failed or produced no valid outputs', () => {
  for (const changes of [{ result: 'failure' }, { result: 'cancelled' }, { result: 'success', outputs: {} },
    { result: 'success', outputs: { code: '', native: '' } }, { result: 'success', outputs: { code: 'yes', native: 'false' } }]) {
    const value = { ...needs('false', 'false', ['skipped', 'skipped', 'skipped']), changes };
    assert.notDeepEqual(gate(value), [], JSON.stringify(changes));
  }
});

test('the gate rejects unknown or missing jobs', () => {
  const extra = { ...needs('true', 'true', ['success', 'success', 'success']), lint: { result: 'success' } };
  assert.notDeepEqual(gate(extra), []);
  const missing = needs('true', 'true', ['success', 'success', 'success']);
  delete missing['native-installation'];
  assert.notDeepEqual(gate(missing), []);
});

test('the gate command exits non-zero on a failed job', () => {
  const script = join(__dirname, 'ci-changes.cjs');
  const run = value => spawnSync(process.execPath, [script, 'gate'], { env: { ...process.env, NEEDS: JSON.stringify(value) } });
  assert.equal(run(needs('true', 'true', ['success', 'success', 'success'])).status, 0);
  assert.equal(run(needs('true', 'true', ['success', 'skipped', 'failure'])).status, 1);
  assert.equal(spawnSync(process.execPath, [script, 'gate'], { env: { ...process.env, NEEDS: '' } }).status, 1);
});

test('ci.yml gates every job on the selection and requires them all', () => {
  const { parse } = require('../../tools/node_modules/yaml');
  const ci = parse(readFileSync('.github/workflows/ci.yml', 'utf8'));
  const gated = Object.keys(ci.jobs).filter(job => !['changes', 'required'].includes(job));
  assert.deepEqual(new Set(gated), new Set(Object.keys(SELECTED_BY)));
  for (const job of gated) {
    assert.equal(ci.jobs[job].needs, 'changes', job);
    assert.equal(ci.jobs[job].if, `needs.changes.outputs.${SELECTED_BY[job]} == 'true'`, job);
  }
  assert.equal(ci.jobs.required.name, 'CI required');
  assert.equal(ci.jobs.required.if, 'always()');
  assert.deepEqual(new Set(ci.jobs.required.needs), new Set(['changes', ...gated]));
  assert.equal(ci.jobs.changes.defaults.run.shell, 'bash', 'pipefail makes a classifier crash fail the step');
});
