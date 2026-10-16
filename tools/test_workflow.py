"""Keep workflow action references compatible with the repository pinning policy."""
from pathlib import Path
import re
import unittest


def check_action_pins(text: str) -> None:
    count = 0
    for number, line in enumerate(text.splitlines(), 1):
        if line.lstrip().startswith('#') or not re.search(r'''(?:\buses|["']uses["'])\s*:''', line):
            continue
        # Reject alternate YAML layouts instead of silently overlooking actions.
        match = re.fullmatch(
            r'''\s*(?:-\s*)?(?:uses|"uses"|'uses'):\s*(?P<quote>["']?)[\w.-]+/[\w./-]+@[0-9a-f]{40}(?P=quote)\s*(?:#.*)?''', line)
        if match is None:
            raise ValueError(f'Line {number}: use a block-style action declaration pinned to a full commit hash')
        count += 1
    if count == 0:
        raise ValueError('No action references were checked')


class WorkflowTests(unittest.TestCase):
    def test_ci_runs_for_pull_requests_and_only_main_pushes(self) -> None:
        workflow = (Path(__file__).resolve().parent.parent / '.github/workflows/ci.yml').read_text()
        triggers = workflow.split('\non:', 1)[1].split('\npermissions:', 1)[0]
        self.assertEqual(triggers, '\n  push:\n    branches: [main]\n  pull_request:')

    def test_remote_actions_use_full_commit_hashes(self) -> None:
        root = Path(__file__).resolve().parent.parent
        workflows = sorted((root / '.github/workflows').glob('*.y*ml'))
        self.assertTrue(workflows, 'No workflows were checked')
        for path in workflows:
            with self.subTest(workflow=path.name):
                check_action_pins(path.read_text())

    def test_platform_checks_keep_required_coverage_without_full_duplication(self) -> None:
        workflow = (Path(__file__).resolve().parent.parent / '.github/workflows/ci.yml').read_text()
        job = workflow.split('  reference-and-rust:', 1)[1].split('  native-installation:', 1)[0]
        # The matrix comes from .github/scripts/ci-changes.cjs, whose tests require the full
        # matrix for native changes.
        self.assertIn('os: ${{ fromJSON(needs.changes.outputs.os) }}', job)
        script = (Path(__file__).resolve().parent.parent / '.github/scripts/ci-changes.cjs').read_text()
        self.assertIn("const FULL_MATRIX = ['ubuntu-latest', 'ubuntu-24.04-arm', 'macos-latest'];", script)
        self.assertIn('name: reference-and-rust (${{ matrix.os }})', job)
        self.assertNotIn('continue-on-error:', job)
        self.assertIn('test "$(uname -m)" = arm64', job)
        steps = job.split('      - ')
        for command, condition in (
            ('npm --prefix tools ci', "runner.os == 'Linux'"),
            ('tools/verify.py --model-exports-only', "runner.os == 'Linux'"),
            ('tools/benchmark.py', "runner.os == 'Linux'"),
            ('tools/check_models.py', "runner.os == 'macOS'"),
            ('native-consumer-check "$model_path"', "runner.os == 'macOS'"),
        ):
            matches = [step for step in steps if command in step]
            self.assertEqual(len(matches), 1, command)
            self.assertEqual(re.findall(r'^        if: (.*)$', matches[0], re.MULTILINE), [condition])
            self.assertNotIn('continue-on-error:', matches[0])
        reference = next(step for step in steps if 'tools/check_models.py' in step)
        self.assertEqual(re.findall(r'^        run: (.*)$', reference, re.MULTILINE),
                         ['.venv/bin/python tools/check_models.py'])
        smoke = next(step for step in steps if 'native-consumer-check "$model_path"' in step)
        self.assertIn('en_core_web_sm en_core_web_md en_core_web_lg', smoke)
        self.assertIn('npm --prefix bindings/node run build', smoke)
        self.assertIn('cargo build --release --locked --offline --manifest-path crates/spars-model/Cargo.toml', smoke)
        self.assertIn('cargo build --release --locked --offline --manifest-path consumer/Cargo.toml', smoke)
        self.assertIn('--archive "assets/$model-3.8.0-py3-none-any.whl" --root "$model_root"', smoke)
        self.assertIn('env -i PATH= target/release/spars-model verify "$model_path"', smoke)
        self.assertIn('env -i PATH= consumer/target/release/native-consumer-check', smoke)
        self.assertIn('bindings/node/scripts/smoke.mts "$model_path"', smoke)
        self.assertIn('if: always()', next(step for step in steps if 'actions/upload-artifact@' in step))

    def test_installation_jobs_use_fresh_model_stores(self) -> None:
        workflow = (Path(__file__).resolve().parent.parent / '.github/workflows/ci.yml').read_text()
        for name in ('reference-and-rust', 'native-installation'):
            job = re.split(r'\n  [a-z][a-z-]*:', workflow.split('  ' + name + ':', 1)[1])[0]
            with self.subTest(job=name):
                install = next(step for step in job.split('      - ') if 'spars-model install' in step)
                allocations = list(re.finditer(r'^          model_root=\$\(mktemp -d\)$', install, re.MULTILINE))
                self.assertEqual(len(allocations), 1)
                self.assertIn('--root "$model_root"', install)
                self.assertLess(allocations[0].start(), install.index('spars-model install'))

    def test_rust_jobs_cache_both_workspaces_without_skipping_checks(self) -> None:
        workflow = (Path(__file__).resolve().parent.parent / '.github/workflows/ci.yml').read_text()
        for name in ('reference-and-rust', 'native-installation'):
            # Job boundaries have exactly two leading spaces.
            job = re.split(r'\n  [a-z][a-z-]*:', workflow.split('  ' + name + ':', 1)[1])[0]
            steps = job.split('      - ')
            cache = next(step for step in steps if 'Swatinem/rust-cache@' in step)
            self.assertIn('. -> target', cache)
            self.assertIn('consumer -> target', cache)
            self.assertIn("save-if: ${{ github.ref == 'refs/heads/main' }}", cache)
            self.assertLess(job.index('dtolnay/rust-toolchain@'), job.index('Swatinem/rust-cache@'))
            self.assertLess(job.index('Swatinem/rust-cache@'), job.index('cargo fetch'))
            self.assertNotIn('cache-hit', job)

    def test_unpinned_and_alternate_action_declarations_fail(self) -> None:
        pinned = '  - uses: example/action@' + 'a' * 40 + ' # v1\n'
        check_action_pins(pinned)
        check_action_pins('  - "uses": "example/action@' + 'a' * 40 + '"\n')
        for unpinned in ('  - uses: example/action@main', '  - { uses: example/action@main }',
                         '  - "uses": "example/action@v1"', '  - uses:\n      example/action@main',
                         '  - uses: example/action@abcdef'):
            with self.subTest(declaration=unpinned), self.assertRaises(ValueError):
                check_action_pins(pinned + unpinned)
        with self.assertRaisesRegex(ValueError, 'No action'):
            check_action_pins('# uses: example/action@main\n')


if __name__ == '__main__':
    unittest.main()
