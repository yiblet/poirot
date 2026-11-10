"""Exercise release version updates in isolated Git and Cargo workspaces."""
from pathlib import Path
import subprocess
import tempfile
import tomllib
import unittest
from unittest.mock import patch

from set_release_version import set_release_version


class ReleaseVersionTests(unittest.TestCase):
    """Keep release versions aligned without changing unrelated versioned material."""

    def setUp(self):
        """Create a facade, a workspace component and an independent benchmark."""
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        files = {
            'Cargo.toml': '''[workspace]
members = ["crates/poirot-functions"]
exclude = ["bench", "extras/foreign"]
resolver = "3"
[workspace.package]
version = "0.1.0"
[package]
name = "poirot"
version.workspace = true
edition = "2024"
[dependencies]
alias = { package = "poirot-functions", path = "crates/poirot-functions", version = "0.1.0" }
foreign = { path = "extras/foreign", version = "0.1.0" }
[package.metadata]
source = { path = ".", version = "0.1.0" }
''',
            'crates/poirot-functions/Cargo.toml': '''[package]
name = "poirot-functions"
version.workspace = true
edition = "2024"
''',
            'extras/foreign/Cargo.toml': '''[workspace]
[package]
name = "foreign"
version = "0.1.0"
edition = "2024"
''',
            'bench/Cargo.toml': '''[workspace]
[package]
name = "benchmark"
version = "9.0.0"
edition = "2024"
[dependencies]
poirot = { path = "..", version = "0.1.0" }
poirot-functions = { path = "../crates/poirot-functions" }
''',
            'README.md': 'poirot = "0.1.0"\nhttps://docs.rs/poirot/0.1.0/poirot/\nFixture 0.1.0\n',
            'src/lib.rs': '// Fixture 0.1.0\n',
            'crates/poirot-functions/src/lib.rs': '',
            'extras/foreign/src/lib.rs': '',
            'bench/src/lib.rs': '',
        }
        for name, content in files.items():
            path = self.root / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(content)
        for manifest in ('Cargo.toml', 'bench/Cargo.toml'):
            subprocess.run(['cargo', 'metadata', '--offline', '--format-version', '1',
                            '--manifest-path', str(self.root / manifest)], check=True,
                           stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        subprocess.run(['git', 'init', '--quiet'], cwd=self.root, check=True)
        subprocess.run(['git', 'add', 'Cargo.toml', 'Cargo.lock', 'README.md', 'src',
                        'crates', 'extras', 'bench/Cargo.toml', 'bench/Cargo.lock', 'bench/src'],
                       cwd=self.root, check=True)

    def manifest(self, name='Cargo.toml'):
        """Read the semantic manifest fields independently of text replacement."""
        return tomllib.loads((self.root / name).read_text())

    def test_release_metadata_and_unrelated_versions(self):
        """Update renamed path dependencies, lockfiles and README references only."""
        set_release_version(self.root, '0.1.1', offline=True)
        self.assertEqual(self.manifest()['workspace']['package']['version'], '0.1.1')
        self.assertEqual(self.manifest()['dependencies']['alias']['version'], '0.1.1')
        self.assertEqual(self.manifest()['dependencies']['foreign']['version'], '0.1.0')
        self.assertEqual(self.manifest()['package']['metadata']['source']['version'], '0.1.0')
        bench = self.manifest('bench/Cargo.toml')
        self.assertEqual(bench['package']['version'], '9.0.0')
        self.assertEqual(bench['dependencies']['poirot']['version'], '0.1.1')
        self.assertNotIn('version', bench['dependencies']['poirot-functions'])
        for name in ('Cargo.lock', 'bench/Cargo.lock'):
            packages = {item['name']: item['version'] for item in self.manifest(name)['package']}
            self.assertEqual(packages['poirot'], '0.1.1')
            self.assertEqual(packages['poirot-functions'], '0.1.1')
            self.assertEqual(packages['foreign'], '0.1.0')
        self.assertEqual((self.root / 'README.md').read_text(),
                         'poirot = "0.1.1"\nhttps://docs.rs/poirot/0.1.1/poirot/\nFixture 0.1.0\n')
        self.assertEqual((self.root / 'src/lib.rs').read_text(), '// Fixture 0.1.0\n')

    def test_readme_inline_dependency_versions(self):
        """Update featured dependency examples while preserving their options."""
        readme = self.root / 'README.md'
        readme.write_text('''poirot = { version = "0.1.0", features = ["pcre2"] }
poirot = { features = ["pcre2"], version = '0.1.0' }
other = { version = "0.1.0", features = ["pcre2"] }
poirot = { path = ".", features = ["pcre2"] }
''')
        set_release_version(self.root, '0.2.0', offline=True)
        self.assertEqual(readme.read_text(), '''poirot = { version = "0.2.0", features = ["pcre2"] }
poirot = { features = ["pcre2"], version = "0.2.0" }
other = { version = "0.1.0", features = ["pcre2"] }
poirot = { path = ".", features = ["pcre2"] }
''')

    def test_prerelease_and_repeated_runs(self):
        """Accept Cargo prereleases and leave a repeated update unchanged."""
        set_release_version(self.root, '0.2.0-rc.1', offline=True)
        before = {p: p.read_bytes() for p in self.root.rglob('*')
                  if p.is_file() and '.git' not in p.parts}
        set_release_version(self.root, '0.2.0-rc.1', offline=True)
        self.assertEqual(before, {p: p.read_bytes() for p in before})
        self.assertEqual(self.manifest()['workspace']['package']['version'], '0.2.0-rc.1')

    def test_invalid_versions_leave_metadata_unchanged(self):
        """Reject malformed versions before invoking Cargo or writing files."""
        before = (self.root / 'Cargo.toml').read_bytes()
        for version in ('v0.1.1', '0.1', '01.1.0', '1.0.0-01', '1.0.0-rc..1', '1.0.0\n'):
            with self.subTest(version=version), self.assertRaises(ValueError):
                set_release_version(self.root, version, offline=True)
        self.assertEqual((self.root / 'Cargo.toml').read_bytes(), before)

    def test_failed_lock_refresh_restores_files(self):
        """Roll back manifests, README and an already refreshed root lockfile."""
        before = {p: p.read_bytes() for p in self.root.rglob('*')
                  if p.is_file() and '.git' not in p.parts}
        run = subprocess.run

        def fail_benchmark(command, **kwargs):
            """Fail only after the real root lockfile has already been updated."""
            if '--manifest-path' in command and str(self.root / 'bench/Cargo.toml') in command:
                self.assertEqual(self.manifest()['workspace']['package']['version'], '0.1.1')
                packages = self.manifest('Cargo.lock')['package']
                self.assertEqual(next(p['version'] for p in packages if p['name'] == 'poirot'), '0.1.1')
                raise subprocess.CalledProcessError(1, command)
            return run(command, **kwargs)

        with patch('set_release_version.subprocess.run', side_effect=fail_benchmark):
            with self.assertRaises(subprocess.CalledProcessError):
                set_release_version(self.root, '0.1.1', offline=True)
        self.assertEqual(before, {p: p.read_bytes() for p in before})


if __name__ == '__main__':
    unittest.main()
