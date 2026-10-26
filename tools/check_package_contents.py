"""Validate self-contained Cargo archives against canonical workspace records."""
import json
from pathlib import Path, PurePosixPath
import posixpath
import re
import subprocess
import sys
import tarfile
from urllib.parse import unquote, urlsplit


ROOT_DOCUMENTS = {
    'CONTRIBUTING.md', 'docs/compatibility.md', 'docs/resources.md',
    'docs/evaluation.md', 'docs/architecture.md',
}
PACKAGE_RECORDS = {
    'poirot-functions': {
        'src/matrix/REGRESSION_SOURCES.md', 'src/math/SOURCES.sha256',
        'src/statistics/SAMPLE_SOURCES.md',
    },
    'poirot-io': {
        'tests/fixtures/poi-SimpleMultiCell.xls',
        'tests/fixtures/poi-StringFormulas.xls',
        'tests/fixtures/poi-unicodeNameRecord.xls',
    },
}


def validate_archive(archive, package, root):
    """Check one archive's file policy, legal records and local Markdown links."""
    prefix = f"{package['name']}-{package['version']}"
    common = {'Cargo.toml', 'Cargo.toml.orig', 'Cargo.lock', '.cargo_vcs_info.json',
              'README.md'}
    extra = PACKAGE_RECORDS.get(package['name'], set())
    is_root = Path(package['manifest_path']).parent == root
    allowed = common | extra | (ROOT_DOCUMENTS if is_root else set())
    legal = {path.relative_to(root).as_posix(): path.read_bytes()
             for path in sorted((root / 'LICENSES').iterdir()) if path.is_file()}
    required = {'README.md'} | set(legal) | extra
    if is_root:
        required |= ROOT_DOCUMENTS
    with tarfile.open(archive) as contents:
        files = {}
        for item in contents:
            if item.isdir():
                continue
            if not item.isfile():
                raise ValueError(f'{prefix}: non-file archive member: {item.name}')
            if not item.name.startswith(prefix + '/'):
                raise ValueError(f'{prefix}: unexpected archive path: {item.name}')
            name = item.name.removeprefix(prefix + '/')
            source = contents.extractfile(item)
            if source is None or name in files:
                raise ValueError(f'{prefix}: invalid archive member: {name}')
            files[name] = source.read()
        unexpected = {name for name in files if name not in allowed | set(legal)
                      and not name.startswith(('src/', 'examples/'))}
        if unexpected:
            raise ValueError(f'{prefix}: unexpected package files: {sorted(unexpected)}')
        missing = required - files.keys()
        if missing:
            raise ValueError(f'{prefix}: missing {sorted(missing)}')
        for name, expected in legal.items():
            if files[name] != expected:
                raise ValueError(f'{prefix}: {name} differs from workspace copy')
        for name in extra:
            expected = Path(package['manifest_path']).parent / name
            if files[name] != expected.read_bytes():
                raise ValueError(f'{prefix}: {name} differs from workspace copy')
        validate_links(files, prefix)


def validate_links(files, prefix):
    """Require relative Markdown links to resolve within the packaged files."""
    for name, data in files.items():
        if not name.endswith('.md'):
            continue
        for target in re.findall(r'\]\(([^\s)]+)(?:\s+[^)]*)?\)', data.decode('utf-8')):
            link = urlsplit(target.strip('<>'))
            if link.scheme or link.netloc or not link.path:
                continue
            resolved = posixpath.normpath(str(PurePosixPath(name).parent / unquote(link.path)))
            if resolved not in files:
                raise ValueError(f'{prefix}: {name}: missing link target {target}')


def main():
    """Validate every publishable workspace archive after cargo package."""
    metadata = json.loads(subprocess.check_output([
        'cargo', 'metadata', '--locked', '--offline', '--no-deps',
        '--format-version', '1',
    ]))
    root = Path(metadata['workspace_root'])
    checked = 0
    for package in metadata['packages']:
        if package['publish'] == []:
            continue
        prefix = f"{package['name']}-{package['version']}"
        archive = Path(metadata['target_directory']) / 'package' / f'{prefix}.crate'
        validate_archive(archive, package, root)
        checked += 1
    print(f'Checked {checked} package archives; legal records and local links match.')


if __name__ == '__main__':
    try:
        main()
    except (OSError, ValueError, KeyError, tarfile.TarError,
            subprocess.CalledProcessError) as error:
        print(error, file=sys.stderr)
        sys.exit(1)
