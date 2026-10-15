"""Check frozen fixtures, Markdown structure, links, and reviewer configuration."""
import hashlib
import re
import tomllib
from pathlib import Path
from dataclasses import dataclass
from json_types import read_json, string_map, validate_json, json_object
from doc_paths import check_paths


REVIEWER_NAMES: frozenset[str] = frozenset({
    "reference_review", "test_review", "docs_review", "api_docs_review", "performance_review",
})


@dataclass(frozen=True)
class ReviewerConfig:
    name: str
    description: str
    developer_instructions: str
    sandbox_mode: str

    @classmethod
    def load(cls, path: Path) -> "ReviewerConfig":
        raw: object = tomllib.loads(path.read_text())
        data = json_object(validate_json(raw))
        values: list[str] = []
        for key in ("name", "description", "developer_instructions", "sandbox_mode"):
            value = data.get(key)
            if not isinstance(value, str) or not value.strip():
                raise ValueError(f"{path}: missing {key}")
            values.append(value)
        return cls(*values)


def check_fixtures(root: Path) -> None:
    lock = string_map(read_json(root/'fixtures/checksums.json'))
    actual = {str(p.relative_to(root)) for p in (root/'fixtures').glob('*.json') if p.name != 'checksums.json'}
    if actual != set(lock):
        raise ValueError('Fixture inventory changed. Review new fixtures and their checksums.')
    for name, digest in lock.items():
        if hashlib.sha256((root/name).read_bytes()).hexdigest() != digest:
            raise ValueError(f'Frozen fixture changed: {name}')


def check_markdown(path: Path, root: Path) -> None:
    lines = path.read_text().splitlines()
    fence = None
    paragraph = False
    for number, line in enumerate(lines, 1):
        check_paths(line, path, root, number)
        stripped = line.strip()
        if stripped.startswith(('```', '~~~')):
            marker = stripped[:3]
            fence = None if fence == marker else marker
            paragraph = False
            continue
        if fence:
            continue
        structural = not stripped or stripped.startswith(('#', '|', '>', '<!--')) or re.match(r'^\s*(?:[-+*]|\d+[.)])\s', line) is not None
        if not structural and paragraph:
            raise ValueError(f'{path}:{number}: keep a prose paragraph on one source line')
        paragraph = bool(stripped) and not structural
        for target in re.findall(r'\[[^\]]*\]\(([^)]+)\)', line):
            if '://' in target or target.startswith(('#', 'mailto:')):
                continue
            target = target.split('#')[0]
            if target.startswith('/') or not (path.parent/target).exists():
                raise ValueError(f'{path}:{number}: broken or nonportable link {target}')
    if fence:
        raise ValueError(f'{path}: unclosed code fence')


def check_agents(root: Path) -> None:
    paths = sorted((root/'.codex/agents').glob('*.toml'))
    if frozenset(path.stem for path in paths) != REVIEWER_NAMES:
        raise ValueError('Reviewer inventory must match the documented roles: ' + ', '.join(sorted(REVIEWER_NAMES)))
    names: set[str] = set()
    for path in paths:
        data = ReviewerConfig.load(path)
        if data.name != path.stem or data.name in names:
            raise ValueError(f'{path}: agent names must be unique and match filenames')
        if data.sandbox_mode != 'read-only':
            raise ValueError(f'{path}: reviewers should be read-only')
        names.add(data.name)


def check_portable_artifacts(root: Path) -> None:
    from report_paths import portable_json

    paths = [*sorted((root/'fixtures').rglob('*.json')), *sorted((root/'target/reports').rglob('*.json'))]
    for path in paths:
        value = read_json(path)
        if portable_json(value, root) != value:
            raise ValueError(f'{path.relative_to(root)}: remove absolute filesystem paths from saved data')


def main() -> None:
    root = Path(__file__).resolve().parent.parent
    check_fixtures(root)
    check_portable_artifacts(root)
    paths = [*root.glob('*.md'), *sorted((root/'docs').rglob('*.md')), root/'fixtures/README.md',
             *sorted((root/'.github').rglob('*.md')), *sorted((root/'reference').glob('*.md'))]
    for path in paths:
        check_markdown(path, root)
    check_agents(root)
    print(f'PASS frozen fixture checksums, {len(paths)} Markdown files, and {len(REVIEWER_NAMES)} reviewer configurations')


if __name__ == '__main__':
    main()
