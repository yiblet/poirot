"""Package and build the publishable dependency closure without registry writes."""
import argparse
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tarfile
import tempfile
import tomllib

from check_package_contents import validate_archive


def release_order(packages):
    """Return publishable packages in deterministic dependency-first order."""
    by_path = {Path(p['manifest_path']).parent.resolve(): p for p in packages}
    public = {p['name']: p for p in packages if p['publish'] != []}
    edges = {name: set() for name in public}
    for name, package in public.items():
        for dependency in package['dependencies']:
            if not dependency.get('path'):
                continue
            target = by_path.get(Path(dependency['path']).resolve())
            if target is None or target['name'] not in public:
                raise ValueError(f"{name}: dependency {dependency['name']} is not publishable in this workspace")
            if dependency['req'] == '*':
                raise ValueError(f"{name}: dependency {dependency['name']} needs a release version")
            # Dev dependencies must be packaged too, but do not constrain publish order.
            if dependency['kind'] != 'dev':
                edges[name].add(target['name'])
    ordered = []
    while edges:
        ready = sorted(name for name, dependencies in edges.items() if not dependencies)
        if not ready:
            raise ValueError(f'publish dependency cycle: {", ".join(sorted(edges))}')
        for name in ready:
            ordered.append(public[name])
            del edges[name]
        for dependencies in edges.values():
            dependencies.difference_update(ready)
    return ordered


def extracted_workspace(packages, archive_dir, destination):
    """Extract validated archives and patch only their normalized manifests."""
    members = []
    patches = []
    for package in packages:
        prefix = f"{package['name']}-{package['version']}"
        with tarfile.open(archive_dir / f'{prefix}.crate') as archive:
            for member in archive:
                path = destination / member.name
                if not member.isfile() or not path.resolve().is_relative_to(destination.resolve()):
                    raise ValueError(f'{prefix}: unsafe archive member: {member.name}')
                path.parent.mkdir(parents=True, exist_ok=True)
                with archive.extractfile(member) as source, path.open('wb') as output:
                    shutil.copyfileobj(source, output)
        manifest = tomllib.loads((destination / prefix / 'Cargo.toml').read_text())
        for section in ('dependencies', 'dev-dependencies', 'build-dependencies'):
            groups = [manifest] + list(manifest.get('target', {}).values())
            for group in groups:
                if any('path' in dep or 'git' in dep
                       for dep in group.get(section, {}).values()):
                    raise ValueError(f'{prefix}: normalized manifest retains a source path or Git dependency')
        members.append(prefix)
        patches.append(f'{json.dumps(package["name"])} = {{ path = {json.dumps(prefix)} }}')
    (destination / 'Cargo.toml').write_text(
        '[workspace]\nresolver = "3"\nmembers = ' + json.dumps(members)
        + '\n\n[patch.crates-io]\n' + '\n'.join(patches) + '\n')


def main():
    """Build archives, check their contracts, examples, documentation and doctests."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--offline', action='store_true', help='use cached Cargo dependencies only')
    parser.add_argument('--allow-dirty', action='store_true', help='validate uncommitted source changes')
    parser.add_argument('--order', action='store_true', help='print dependency-first release order and exit')
    args = parser.parse_args()
    root = Path(__file__).resolve().parent.parent
    network = ['--offline'] if args.offline else []
    metadata = json.loads(subprocess.check_output(
        ['cargo', 'metadata', '--locked', '--no-deps', '--format-version', '1', *network], cwd=root))
    packages = [p for p in metadata['packages'] if p['id'] in metadata['workspace_members']]
    ordered = release_order(packages)
    if args.order:
        for package in ordered:
            print(f"{package['name']} {package['version']}")
        return
    exclusions = [arg for p in packages if p['publish'] == [] for arg in ('--exclude', p['name'])]
    artifacts = Path(metadata['target_directory']) / 'release-artifacts'
    archive_dir = artifacts / 'package'
    # This dedicated generated directory must never retain obsolete release archives.
    if archive_dir.exists():
        shutil.rmtree(archive_dir)
    subprocess.run(['cargo', 'package', '--workspace', *exclusions, '--all-features',
                    '--locked', '--no-verify', '--target-dir', str(artifacts), *network,
                    *(['--allow-dirty'] if args.allow_dirty else [])], cwd=root, check=True)
    for package in ordered:
        archive = archive_dir / f"{package['name']}-{package['version']}.crate"
        validate_archive(archive, package, root)
    with tempfile.TemporaryDirectory(prefix='poirot-release-') as temporary:
        destination = Path(temporary)
        extracted_workspace(ordered, archive_dir, destination)
        shutil.copyfile(root / 'Cargo.lock', destination / 'Cargo.lock')
        common = ['--workspace', '--all-features', '--locked', *network,
                  '--target-dir', str(artifacts / 'build')]
        # Cargo adapts the seed lock once to the extracted workspace, preserving versions.
        subprocess.run(['cargo', 'check', '--all-targets',
                        *[arg for arg in common if arg != '--locked']], cwd=destination, check=True)
        environment = os.environ.copy()
        environment['RUSTDOCFLAGS'] = environment.get('RUSTDOCFLAGS', '') + ' -D warnings'
        subprocess.run(['cargo', 'test', '--doc', '--jobs', '1', *common,
                        '--', '--test-threads=1'], cwd=destination, env=environment, check=True)
        subprocess.run(['cargo', 'doc', '--no-deps', *common],
                       cwd=destination, env=environment, check=True)
    print(f"Verified {len(ordered)} archives, all targets, doctests and API documentation in {archive_dir}.")


if __name__ == '__main__':
    try:
        main()
    except (OSError, ValueError, KeyError, tarfile.TarError, subprocess.CalledProcessError) as error:
        print(error, file=sys.stderr)
        sys.exit(1)
