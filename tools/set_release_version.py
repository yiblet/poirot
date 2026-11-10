"""Set Poirot's release version, internal requirements, lockfiles and README."""
import argparse
import json
from pathlib import Path
import re
import subprocess
import sys
import tomllib


NUMBER = r'(?:0|[1-9][0-9]*)'
IDENTIFIER = rf'(?:{NUMBER}|[0-9]*[A-Za-z-][0-9A-Za-z-]*)'
VERSION = (rf'{NUMBER}\.{NUMBER}\.{NUMBER}'
           rf'(?:-{IDENTIFIER}(?:\.{IDENTIFIER})*)?'
           r'(?:\+[0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*)?')


def set_manifest_version(text, section, version):
    """Replace an explicit version in one manifest table, preserving formatting."""
    pattern = rf'(?ms)(^\[{re.escape(section)}\][^\n]*\n)(.*?)(?=^\[|\Z)'

    def replace(match):
        """Change only the table's version field, leaving inherited versions alone."""
        body = re.sub(r'''(?m)^([ \t]*version\s*=\s*)["'][^"']*["']''',
                      lambda field: field[1] + json.dumps(version), match[2])
        return match[1] + body

    return re.sub(pattern, replace, text)


def update_requirements(text, parent, release_directories, version):
    """Update versioned inline path dependencies pointing at release packages."""
    def replace(match):
        """Preserve unrelated dependencies and path-only benchmark dependencies."""
        entry = next(iter(tomllib.loads(match[0]).values()))
        if ('path' not in entry or 'version' not in entry
                or (parent / entry['path']).resolve() not in release_directories):
            return match[0]
        return re.sub(r'''\bversion\s*=\s*["'][^"']*["']''',
                      lambda _: 'version = ' + json.dumps(version), match[0])

    def replace_section(match):
        """Limit dependency edits to Cargo dependency tables, excluding metadata."""
        return match[1] + re.sub(r'(?m)^[\w-]+\s*=\s*\{[^\n]*\}', replace, match[2])

    sections = (r'(?ms)(^\[(?:workspace\.|target\.[^\n]+\.)?'
                r'(?:dependencies|dev-dependencies|build-dependencies)\][^\n]*\n)'
                r'(.*?)(?=^\[|\Z)')
    updated = re.sub(sections, replace_section, text)
    manifest = tomllib.loads(updated)
    groups = [manifest, manifest.get('workspace', {}), *manifest.get('target', {}).values()]
    for group in groups:
        for section in ('dependencies', 'dev-dependencies', 'build-dependencies'):
            for name, entry in group.get(section, {}).items():
                if (isinstance(entry, dict) and 'path' in entry and 'version' in entry
                        and (parent / entry['path']).resolve() in release_directories
                        and entry['version'] != version):
                    raise ValueError(f'{parent}: {name}: use an inline table for its release requirement')
    return updated


def set_release_version(root, version, offline=False):
    """Update tracked release metadata and restore it if Cargo refresh fails."""
    if re.fullmatch(VERSION, version) is None:
        raise ValueError('expected a SemVer version such as 0.1.1 or 0.2.0-rc.1, without a v prefix')
    network = ['--offline'] if offline else []
    metadata = json.loads(subprocess.check_output(
        ['cargo', 'metadata', '--locked', '--no-deps', '--format-version', '1', *network], cwd=root))
    packages = [package for package in metadata['packages']
                if package['id'] in metadata['workspace_members']]
    release_directories = {Path(package['manifest_path']).parent.resolve() for package in packages}
    tracked = subprocess.check_output(
        ['git', 'ls-files', '-z', '--', 'Cargo.toml', 'Cargo.lock', '**/Cargo.toml',
         '**/Cargo.lock', 'README.md'], cwd=root).decode().split('\0')
    paths = [root / name for name in tracked if name]
    originals = {path: path.read_bytes() for path in paths}
    updates = {}
    for path in paths:
        if path.name != 'Cargo.toml':
            continue
        text = originals[path].decode()
        if path.parent.resolve() in release_directories:
            text = set_manifest_version(text, 'package', version)
        if path == root / 'Cargo.toml':
            text = set_manifest_version(text, 'workspace.package', version)
        updates[path] = update_requirements(text, path.parent, release_directories, version)
        parsed = tomllib.loads(updates[path])
        if path == root / 'Cargo.toml' and parsed['workspace']['package']['version'] != version:
            raise ValueError('could not update workspace.package.version')
        if path.parent.resolve() in release_directories:
            own_version = parsed['package']['version']
            if own_version != version and own_version != {'workspace': True}:
                raise ValueError(f'{path}: could not update package.version')
    readme = root / 'README.md'
    text = originals[readme].decode()
    text = re.sub(r'(?m)^(poirot\s*=\s*)"[^"]+"',
                  lambda match: match[1] + json.dumps(version), text)
    text = re.sub(r'''(?m)^(poirot[ \t]*=[ \t]*\{[^\n}]*\bversion[ \t]*=[ \t]*)["'][^"']*["']''',
                  lambda match: match[1] + json.dumps(version), text)
    updates[readme] = re.sub(rf'(https://docs\.rs/poirot/){VERSION}(/poirot/)',
                             lambda match: match[1] + version + match[2], text)
    try:
        for path, text in updates.items():
            if text.encode() != originals[path]:
                path.write_text(text)
        for path in paths:
            if path.name == 'Cargo.lock':
                subprocess.run(['cargo', 'metadata', '--format-version', '1', *network,
                                '--manifest-path', str(path.with_name('Cargo.toml'))],
                               cwd=root, check=True, stdout=subprocess.DEVNULL)
    except (OSError, subprocess.CalledProcessError):
        for path, content in originals.items():
            path.write_bytes(content)
        raise


def main():
    """Set release metadata without committing, tagging or publishing."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('version', help='release version without a v prefix')
    parser.add_argument('--offline', action='store_true', help='refresh lockfiles using cached dependencies')
    args = parser.parse_args()
    root = Path(__file__).resolve().parent.parent
    set_release_version(root, args.version, args.offline)
    print(f'Set release metadata to {args.version}; review the diff before committing.')


if __name__ == '__main__':
    try:
        main()
    except (OSError, ValueError, KeyError, subprocess.CalledProcessError) as error:
        print(error, file=sys.stderr)
        sys.exit(1)
