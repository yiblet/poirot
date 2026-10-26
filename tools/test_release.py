"""Exercise release dependency ordering and archive-only workspace isolation."""
import io
import json
from pathlib import Path
import tarfile
import tempfile
import tomllib
import unittest

from verify_release import extracted_workspace, release_order


def package(name, dependencies=(), publish=None):
    """Describe a workspace member using Cargo metadata's relevant fields."""
    return {'name': name, 'version': '0.1.0', 'manifest_path': f'/workspace/{name}/Cargo.toml',
            'publish': publish, 'dependencies': list(dependencies)}


def dependency(name, kind=None, req='^0.1.0'):
    """Describe a versioned workspace path dependency."""
    return {'name': name, 'path': f'/workspace/{name}', 'req': req, 'kind': kind}


class OrderTests(unittest.TestCase):
    """Check release closure errors and stable dependency-first order."""

    def test_dependency_order_and_unpublished_exclusion(self):
        """Order transitive dependencies before the facade and exclude test tools."""
        packages = [package('api', [dependency('io'), dependency('functions')]),
                    package('io', [dependency('parsing')]), package('functions'),
                    package('test-runner', publish=[]), package('parsing')]
        ordered = [p['name'] for p in release_order(packages)]
        self.assertEqual(ordered, ['functions', 'parsing', 'io', 'api'])
        self.assertEqual(ordered, [p['name'] for p in release_order(list(reversed(packages)))])

    def test_private_or_outside_dependency_rejected(self):
        """A published package cannot depend on an omitted local package."""
        for others in ([], [package('private', publish=[])]):
            with self.subTest(others=others), self.assertRaisesRegex(ValueError, 'not publishable'):
                release_order([package('api', [dependency('private')]), *others])

    def test_unversioned_dependency_rejected(self):
        """Require an explicit registry requirement for local dependencies."""
        with self.assertRaisesRegex(ValueError, 'needs a release version'):
            release_order([package('api', [dependency('part', req='*')]), package('part')])

    def test_cycle_rejected(self):
        """Report cycles rather than producing a partial release order."""
        with self.assertRaisesRegex(ValueError, 'publish dependency cycle: a, b'):
            release_order([package('a', [dependency('b')]), package('b', [dependency('a')])])

    def test_dev_dependency_does_not_constrain_publication_order(self):
        """Permit dev-only back edges while including both archives in verification."""
        ordered = release_order([package('a', [dependency('b', kind='dev')]),
                                 package('b', [dependency('a')])])
        self.assertEqual([p['name'] for p in ordered], ['a', 'b'])


class ExtractionTests(unittest.TestCase):
    """Check that verification uses normalized archives without source escapes."""

    def setUp(self):
        """Create isolated archive and extraction directories."""
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.destination = self.root / 'extracted'
        self.destination.mkdir()
        self.package = package('api')

    def archive(self, manifest, extra=None):
        """Write a small archive containing a normalized manifest and source."""
        files = {'api-0.1.0/Cargo.toml': manifest.encode(), 'api-0.1.0/src/lib.rs': b'// archived source'}
        files.update(extra or {})
        with tarfile.open(self.root / 'api-0.1.0.crate', 'w') as archive:
            for name, data in files.items():
                member = tarfile.TarInfo(name)
                member.size = len(data)
                archive.addfile(member, io.BytesIO(data))

    def test_archive_only_patches(self):
        """Resolve sibling packages to the extracted archives, never original manifests."""
        self.archive('[package]\nname="api"\nversion="0.1.0"\n')
        extracted_workspace([self.package], self.root, self.destination)
        manifest = tomllib.loads((self.destination / 'Cargo.toml').read_text())
        self.assertEqual(manifest['workspace']['members'], ['api-0.1.0'])
        self.assertEqual(manifest['patch']['crates-io'], {'api': {'path': 'api-0.1.0'}})
        self.assertEqual((self.destination / 'api-0.1.0/src/lib.rs').read_bytes(), b'// archived source')
        self.assertNotIn('/workspace', json.dumps(manifest))

    def test_source_dependency_rejected_including_target_specific(self):
        """Reject paths and Git sources retained in normalized manifests."""
        for section in ('dependencies', 'target."cfg(unix)".build-dependencies'):
            for source in ('path="/workspace/part"', 'git="https://example.com/part"'):
                with self.subTest(section=section, source=source):
                    self.archive(f'[{section}.part]\nversion="0.1.0"\n{source}\n')
                    with self.assertRaisesRegex(ValueError, 'source path or Git dependency'):
                        extracted_workspace([self.package], self.root, self.destination)

    def test_escape_rejected(self):
        """Archive members cannot write outside the extraction directory."""
        self.archive('', {'../escaped': b'bad'})
        with self.assertRaisesRegex(ValueError, 'unsafe archive member'):
            extracted_workspace([self.package], self.root, self.destination)
        self.assertFalse((self.root / 'escaped').exists())


if __name__ == '__main__':
    unittest.main()
