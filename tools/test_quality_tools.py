"""Check that the quality tools reject errors rather than merely running."""
from contextlib import redirect_stdout
from io import StringIO
import subprocess
from unittest.mock import patch
import verify
import hashlib
import json
import tempfile
import unittest
from pathlib import Path

from check_docs import rust_examples, rustdoc_passed
from check_quality import check_fixtures, check_markdown, check_agents, ReviewerConfig
from benchmark import parse_measurements
from json_types import ModelMetadata, json_int, parse_json, validate_json


class QualityChecks(unittest.TestCase):
    def test_json_boundary_rejects_invalid_values(self) -> None:
        self.assertEqual(parse_json('{"values": [1, true, null]}'), {"values": [1, True, None]})
        with self.assertRaises(ValueError):
            validate_json({1: "invalid key"})
        with self.assertRaises(ValueError):
            validate_json(Path("unsupported"))
        with self.assertRaises(ValueError):
            json_int(True)

    def test_model_metadata_validates_reference_versions(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp)/"manifest.json"
            path.write_text('{"model":"en_core_web_md","model_version":"3.8.0","versions":{"spacy":3},"weights_sha256":"abc"}')
            with self.assertRaises(ValueError):
                ModelMetadata.load(path)

    def test_benchmark_rejects_malformed_measurements(self) -> None:
        self.assertEqual(parse_measurements('{"load_seconds":0.5}', "load", 100, ["measure"]),
                         {"load_seconds":0.5,"peak_process_rss_bytes":100,"command":["measure"]})
        with self.assertRaises(ValueError):
            parse_measurements('{"load_seconds":true}', "load", 100, [])
        with self.assertRaises(KeyError):
            parse_measurements('{"load_seconds":0.5}', "short", 100, [])

    def test_reviewer_configuration_validates_strings(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp)/"reviewer.toml"
            path.write_text('name = 1')
            with self.assertRaisesRegex(ValueError, "missing name"):
                ReviewerConfig.load(path)

    def test_reviewer_inventory_requires_each_documented_role(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            folder = root/".codex/agents"
            folder.mkdir(parents=True)
            for name in ("reference_review", "test_review", "docs_review", "api_docs_review", "performance_review"):
                (folder/f"{name}.toml").write_text(
                    f'name = "{name}"\ndescription = "Review"\n'
                    'developer_instructions = "Inspect evidence"\nsandbox_mode = "read-only"\n')
            check_agents(root)
            performance = folder/"performance_review.toml"
            content = performance.read_text()
            performance.unlink()
            with self.assertRaisesRegex(ValueError, "inventory"):
                check_agents(root)
            replacement = folder/"other_review.toml"
            replacement.write_text(content.replace("performance_review", "other_review"))
            with self.assertRaisesRegex(ValueError, "inventory"):
                check_agents(root)
            replacement.unlink()
            performance.write_text(content.replace('name = "performance_review"', 'name = "test_review"'))
            with self.assertRaisesRegex(ValueError, "match filenames"):
                check_agents(root)
            performance.write_text(content.replace("read-only", "workspace-write"))
            with self.assertRaisesRegex(ValueError, "read-only"):
                check_agents(root)

    def test_verification_failure_creates_report_and_still_fails(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            output = Path(tmp)/"target/reports/verification.json"
            failure = output.with_name("verification-failure.json")
            command = ["tools/node_modules/.bin/pyright", "--project", "pyrightconfig.json"]
            result = subprocess.CompletedProcess(command, 1, "deliberate failure", "diagnostic")
            with patch("sys.argv", ["verify.py", "--report", str(output)]), \
                    patch("verify.subprocess.run", return_value=result), redirect_stdout(StringIO()):
                with self.assertRaisesRegex(SystemExit, "Failed"):
                    verify.main()
            self.assertFalse(output.exists())
            self.assertEqual(parse_json(failure.read_text()), [{
                "command": command, "exit_code": 1,
                "stdout": "deliberate failure", "stderr": "diagnostic",
            }])

    def test_modified_fixture_fails(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root/'fixtures').mkdir()
            fixture = root/'fixtures/case.json'
            fixture.write_text('[]')
            (root/'fixtures/checksums.json').write_text(json.dumps({'fixtures/case.json':hashlib.sha256(fixture.read_bytes()).hexdigest()}))
            check_fixtures(root)
            fixture.write_text('[1]')
            with self.assertRaisesRegex(ValueError, 'Frozen fixture'):
                check_fixtures(root)
            fixture.unlink()
            with self.assertRaisesRegex(ValueError, 'inventory'):
                check_fixtures(root)

    def test_examples_must_execute(self) -> None:
        self.assertEqual(rust_examples('```rust\nassert_eq!(1, 1);\n```'), 1)
        for flag in ('no_run', 'ignore', 'compile_fail'):
            with self.assertRaises(ValueError):
                rust_examples(f'```rust,{flag}\nassert!(false);\n```')
            with self.assertRaises(ValueError):
                rust_examples(f'```{flag}\nassert!(false);\n```')

    def test_noncanonical_rust_fences_fail(self) -> None:
        for start, end in [('~~~rust', '~~~'), ('````rust', '````'), ('   ```rust', '   ```')]:
            with self.assertRaises(ValueError):
                rust_examples(f'{start}\nassert!(false);\n{end}')

    def test_only_executed_examples_pass(self) -> None:
        output = 'test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out;'
        self.assertTrue(rustdoc_passed(0, output, 2))
        self.assertFalse(rustdoc_passed(1, output, 2))
        self.assertFalse(rustdoc_passed(0, output, 3))
        self.assertFalse(rustdoc_passed(0, output.replace('0 ignored', '1 ignored'), 2))
        self.assertFalse(rustdoc_passed(0, output.replace('0 filtered out', '1 filtered out'), 2))

    def test_markdown_keeps_structure_and_rejects_wrapping(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            path = root/'doc.md'
            path.write_text('# Title\n\nA paragraph.\n\n```rust\nlet a = 1;\nlet b = 2;\n```\n\n- One\n- Two\n')
            check_markdown(path, root)
            path.write_text('A hard-wrapped\nparagraph.\n')
            with self.assertRaisesRegex(ValueError, 'one source line'):
                check_markdown(path, root)

    def test_broken_link_fails(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            path = root/'doc.md'
            path.write_text('[missing](absent.md)\n')
            with self.assertRaisesRegex(ValueError, 'link'):
                check_markdown(path, root)

    def test_markdown_rejects_absolute_paths_without_echoing_them(self) -> None:
        # Deliberately invalid paths use a fictional root, never a host directory.
        examples = ['/__spars_test_root__/project/file.rs', '/__spars_test_root__/models/model',
                    'Z:\\__spars_test_root__\\model', '\\\\fixture-server\\fixture-share\\model',
                    'file:///__spars_test_root__/model', '~/models/model', '$HOME/models', '${PWD}/assets',
                    '//fixture-server/fixture-share/model']
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            path = root/'doc.md'
            for value in examples:
                for body in (f'Load `{value}`.\n', f'```sh\nload "{value}"\n```\n',
                             f'[model]({value})\n', f'The model is at {value}.\n',
                             f'[source](https://example.org)[local]({value})\n',
                             f'[source](https://example.org)({value})\n',
                             f'```text\n[source](https://example.org)[local]({value})\n```\n',
                             f"```sh\nURL='https://example.org';MODEL='{value}'\n```\n"):
                    with self.subTest(value=value, body=body):
                        path.write_text(body)
                        with self.assertRaisesRegex(ValueError, 'project-relative') as raised:
                            check_markdown(path, root)
                        self.assertNotIn(value, str(raised.exception))
                        self.assertNotIn(str(root), str(raised.exception))

    def test_markdown_preserves_relative_paths_urls_and_syntax(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            path = root/'doc.md'
            path.write_text('# Example\n\nUse `./assets/model`, `../SpaRs`, and `src/lib.rs`.\n\n'
                            '[Source](https://example.org/source/LICENSE)\n\n'
                            '[Topic](https://example.org/(topic)/source)\n\n'
                            '<https://[::1]/source>\n\n'
                            '```rust\n// A comment\nlet ratio = 12 / 3;\n/* Another comment */\n```\n\n'
                            '<details>\n\nText and/or markup.\n\n</details>\n')
            check_markdown(path, root)

    def test_unclosed_fence_fails(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            path = root/'doc.md'
            path.write_text('```rust\nassert!(true);\n')
            with self.assertRaisesRegex(ValueError, 'unclosed'):
                check_markdown(path, root)


if __name__ == '__main__':
    unittest.main()
