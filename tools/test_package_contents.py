"""Exercise archive validation without registry access or a Cargo build."""
import io
from pathlib import Path
import tarfile
import tempfile
import unittest

from check_package_contents import ROOT_DOCUMENTS, validate_archive


class ArchiveTests(unittest.TestCase):
    """Cover legal equality, required provenance and packaged Markdown targets."""

    def setUp(self):
        """Create a minimal canonical workspace and archive description."""
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        (self.root / 'LICENSES').mkdir()
        (self.root / 'LICENSES/MIT.txt').write_bytes(b'canonical license')
        self.package = {'name': 'poirot-parsing', 'version': '0.1.0',
                        'manifest_path': str(self.root / 'crates/poirot-parsing/Cargo.toml')}
        self.files = {'README.md': b'# Parsing', 'src/lib.rs': b'',
                      'LICENSES/MIT.txt': b'canonical license'}

    def validate(self):
        """Write and validate the current test archive."""
        archive = self.root / 'test.crate'
        prefix = f"{self.package['name']}-{self.package['version']}"
        with tarfile.open(archive, 'w') as contents:
            for name, data in self.files.items():
                item = tarfile.TarInfo(f'{prefix}/{name}')
                item.size = len(data)
                contents.addfile(item, io.BytesIO(data))
        validate_archive(archive, self.package, self.root)

    def test_component_legal_files_required(self):
        """Reject missing component notices rather than silently accepting them."""
        del self.files['LICENSES/MIT.txt']
        with self.assertRaisesRegex(ValueError, 'missing.*LICENSES/MIT.txt'):
            self.validate()

    def test_changed_legal_file_rejected(self):
        """Reject a notice that differs from its canonical workspace copy."""
        self.files['LICENSES/MIT.txt'] = b'old license'
        with self.assertRaisesRegex(ValueError, 'differs from workspace copy'):
            self.validate()

    def test_root_documents_allowed(self):
        """Accept the root's deliberate packaged guide set."""
        self.package['manifest_path'] = str(self.root / 'Cargo.toml')
        self.files.update({name: b'# Guide' for name in ROOT_DOCUMENTS})
        self.validate()

    def test_unexpected_file_rejected(self):
        """Reject accidental extra documents outside the package policy."""
        self.files['docs/private.md'] = b'# Private'
        with self.assertRaisesRegex(ValueError, 'unexpected package files'):
            self.validate()

    def test_matrix_provenance_required(self):
        """Require the regression source record linked by the matrix README."""
        self.package['name'] = 'poirot-functions'
        with self.assertRaisesRegex(ValueError, 'missing.*REGRESSION_SOURCES.md'):
            self.validate()

    def test_io_fixtures_required(self):
        """Keep compile-time fixture inputs in the IO archive."""
        self.package['name'] = 'poirot-io'
        with self.assertRaisesRegex(ValueError, 'missing.*poi-StringFormulas.xls'):
            self.validate()

    def test_archive_symlink_rejected(self):
        """Require Cargo's flattened regular files rather than dangling symlinks."""
        archive = self.root / 'linked.crate'
        with tarfile.open(archive, 'w') as contents:
            item = tarfile.TarInfo('poirot-parsing-0.1.0/LICENSES')
            item.type = tarfile.SYMTYPE
            item.linkname = '../../LICENSES'
            contents.addfile(item)
        with self.assertRaisesRegex(ValueError, 'non-file archive member'):
            validate_archive(archive, self.package, self.root)

    def test_local_markdown_links(self):
        """Accept packaged links and external URLs while rejecting escapes."""
        self.files['README.md'] = b'[MIT](LICENSES/MIT.txt#license) [web](https://example.com)'
        self.validate()
        self.files['README.md'] = b'[MIT](../../LICENSES/MIT.txt)'
        with self.assertRaisesRegex(ValueError, 'missing link target'):
            self.validate()


if __name__ == '__main__':
    unittest.main()
