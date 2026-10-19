"""Check generated Cargo archives contain runtime sources and complete legal files."""
import json
from pathlib import Path
import subprocess
import sys
import tarfile


def main():
    """Validate each publishable workspace archive after cargo package."""
    metadata = json.loads(subprocess.check_output([
        'cargo', 'metadata', '--locked', '--offline', '--no-deps',
        '--format-version', '1',
    ]))
    root = Path(metadata['workspace_root'])
    legal = ['LICENSE', 'NOTICE', *[
        path.relative_to(root).as_posix()
        for path in sorted((root / 'LICENSES').iterdir()) if path.is_file()
    ]]
    common = {'Cargo.toml', 'Cargo.toml.orig', 'Cargo.lock', '.cargo_vcs_info.json',
              'README.md', 'CONTRIBUTING.md', 'LICENSE', 'NOTICE'}
    checked = 0
    for package in metadata['packages']:
        if package['publish'] == []:
            continue
        prefix = f"{package['name']}-{package['version']}"
        archive = Path(metadata['target_directory']) / 'package' / f'{prefix}.crate'
        with tarfile.open(archive) as contents:
            files = {item.name.removeprefix(prefix + '/') for item in contents if item.isfile()}
            unexpected = files - common
            unexpected = {name for name in unexpected if not name.startswith(
                ('src/', 'examples/', 'tests/fixtures/', 'LICENSES/'))}
            if unexpected:
                raise ValueError(f'{prefix}: unexpected package files: {sorted(unexpected)}')
            for name in ['README.md', *legal]:
                if name not in files:
                    raise ValueError(f'{prefix}: missing {name}')
            for name in legal:
                source = contents.extractfile(f'{prefix}/{name}')
                if source is None or source.read() != (root / name).read_bytes():
                    raise ValueError(f'{prefix}: {name} differs from workspace copy')
        checked += 1
    print(f'Checked {checked} package archives; contents and legal files match.')


if __name__ == '__main__':
    try:
        main()
    except (OSError, ValueError, KeyError, tarfile.TarError,
            subprocess.CalledProcessError) as error:
        print(error, file=sys.stderr)
        sys.exit(1)
