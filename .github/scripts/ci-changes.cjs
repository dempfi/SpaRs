// Decide which CI jobs a change needs from its changed repository paths, and check their results.
//
// `ignored` paths are read by no CI job or test. `docs` paths are read only by the Linux acceptance
// run (`tools/check_quality.py` and `tools/check_docs.py`), which executes guide examples against
// the models. Any other path, including ones added later, needs the full matrix, so unknown files
// fail safe toward running more checks.
const { readFileSync } = require('node:fs');

const LINUX_ONLY = ['ubuntu-latest'];
const FULL_MATRIX = ['ubuntu-latest', 'ubuntu-24.04-arm', 'macos-latest'];

function isIgnored(path) {
  return path === '.github/dependabot.yml' || path.startsWith('.claude/');
}

function isDocs(path) {
  const markdown = path.endsWith('.md');
  return (markdown && !path.includes('/')) ||
    (markdown && (path.startsWith('docs/') || path.startsWith('.github/'))) ||
    (markdown && /^reference\/[^/]+\.md$/.test(path)) ||
    path === 'fixtures/README.md' ||
    path.startsWith('.codex/');
}

/**
 * @param {readonly string[]} paths repository-relative paths changed since the compared commit
 * @returns {{code: boolean, native: boolean, os: string[]}} `code` selects the Linux acceptance
 *   and release-tooling jobs; `native` also selects the arm64, macOS and installation jobs; `os`
 *   is the acceptance matrix.
 */
function classify(paths) {
  const checked = paths.filter(path => !isIgnored(path));
  const code = checked.length > 0;
  const native = checked.some(path => !isDocs(path));
  return { code, native, os: native ? FULL_MATRIX : code ? LINUX_ONLY : [] };
}

// Running everything is the fallback when the changed paths cannot be listed.
const RUN_ALL = { code: true, native: true, os: FULL_MATRIX };

function outputs(result) {
  return `code=${result.code}\nnative=${result.native}\nos=${JSON.stringify(result.os)}\n`;
}

// The `changes` output that selects each gated job.
const SELECTED_BY = {
  'reference-and-rust': 'code',
  'release-tooling': 'code',
  'native-installation': 'native',
};

/**
 * Check the `needs` context of the required gate job.
 *
 * A selected job must succeed and an unselected job must be skipped, so a job that was skipped
 * because the selection broke cannot pass as intentionally skipped.
 *
 * @param {Record<string, {result: string, outputs?: Record<string, string>}>} needs
 * @returns {string[]} one message per problem; empty when the gate passes
 */
function gate(needs) {
  const changes = needs.changes;
  if (changes?.result !== 'success') return [`changes: expected success, got ${changes?.result}`];
  const problems = [];
  const selection = {};
  for (const name of ['code', 'native']) {
    const value = changes.outputs?.[name];
    if (value !== 'true' && value !== 'false') problems.push(`changes: invalid ${name} output ${JSON.stringify(value)}`);
    selection[name] = value === 'true';
  }
  if (problems.length) return problems;
  for (const [job, { result }] of Object.entries(needs)) {
    if (job === 'changes') continue;
    const output = SELECTED_BY[job];
    if (output === undefined) {
      problems.push(`${job}: not known to the gate; add it to SELECTED_BY`);
      continue;
    }
    const expected = selection[output] ? 'success' : 'skipped';
    if (result !== expected) problems.push(`${job}: expected ${expected}, got ${result}`);
  }
  for (const job of Object.keys(SELECTED_BY)) {
    if (!(job in needs)) problems.push(`${job}: missing from the gate's needs`);
  }
  return problems;
}

module.exports = { classify, outputs, gate, RUN_ALL, SELECTED_BY };

if (require.main === module) {
  if (process.argv[2] === 'gate') {
    const problems = gate(JSON.parse(process.env.NEEDS ?? '{}'));
    for (const problem of problems) console.error(problem);
    process.exitCode = problems.length ? 1 : 0;
  } else {
    const listing = process.argv[2];
    const result = listing === undefined ? RUN_ALL
      : classify(readFileSync(listing, 'utf8').split('\n').filter(line => line.length > 0));
    process.stdout.write(outputs(result));
  }
}
