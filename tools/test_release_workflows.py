"""Exercise release workflow scripts without registry writes or GitHub credentials."""
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import textwrap
import unittest
from unittest.mock import patch


WORKFLOW = Path(__file__).resolve().parent.parent / '.github/workflows/release.yml'


def script(name, workflow=WORKFLOW):
    """Read an executable run block from the release workflow."""
    steps = workflow.read_text().split('      - name: ')
    step = next(item for item in steps if item.startswith(name + '\n'))
    body = []
    for line in step.split('        run: |\n', 1)[1].splitlines(keepends=True):
        if line.strip() and not line.startswith('          '):
            break
        body.append(line)
    return textwrap.dedent(''.join(body))


class MatrixTests(unittest.TestCase):
    """Keep release builds expanded across toolchains and feature configurations."""

    def test_routine_release_and_label_expansion(self):
        """Run two routine builds, six release builds, or four extra label builds."""
        program = script('Select build configurations', WORKFLOW.with_name('ci.yml'))
        program = program.split("python3 - <<'PYTHON'\n", 1)[1].split('\nPYTHON', 1)[0]
        for expanded, label, flags in [(False, False, ['']),
                                       (True, False, ['', '--no-default-features', '--all-features']),
                                       (True, True, ['--no-default-features', '--all-features'])]:
            with self.subTest(expanded=expanded, label=label), tempfile.TemporaryDirectory() as directory:
                output = Path(directory) / 'output'
                with patch.dict(os.environ, {'GITHUB_OUTPUT': str(output),
                                              'ALL_BUILDS': str(expanded).lower(),
                                              'LABEL_EXPANSION': str(label).lower()}):
                    exec(program, {})
                matrix = json.loads(output.read_text().removeprefix('matrix='))['include']
                expected = {(toolchain, flag) for toolchain in ('stable', '1.90.0') for flag in flags}
                self.assertEqual({(item['toolchain'], item['flags']) for item in matrix}, expected)
                self.assertEqual(len(matrix), len(expected))


class PreparationTests(unittest.TestCase):
    """Start candidates manually and update only active release PRs on pushes."""

    def test_manual_start_and_active_release_detection(self):
        """Ignore ordinary or fork PRs and do no API lookup for a manual start."""
        workflow = WORKFLOW.with_name('release-pr.yml')
        cases = [('workflow_dispatch', [], True),
                 ('push', [], False),
                 ('push', [{'headRefName': 'feat/feature', 'isCrossRepository': False}], False),
                 ('push', [{'headRefName': 'release/next-master', 'isCrossRepository': False}], True),
                 ('push', [{'headRefName': 'release/next-master', 'isCrossRepository': True}], False)]
        for event, pulls, expected in cases:
            with self.subTest(event=event, pulls=pulls), tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                gh = root / 'gh'
                gh.write_text('''#!/usr/bin/env python3
import os
import subprocess
import sys
if os.environ['GITHUB_EVENT_NAME'] == 'workflow_dispatch':
    raise SystemExit('Manual preparation should not query GitHub')
assert sys.argv[1:3] == ['pr', 'list']
assert sys.argv[sys.argv.index('--base') + 1] == 'master'
assert sys.argv[sys.argv.index('--state') + 1] == 'open'
query = sys.argv[sys.argv.index('--jq') + 1]
subprocess.run(['jq', '-r', query], input=os.environ['PULLS_FIXTURE'], text=True, check=True)
''')
                gh.chmod(0o755)
                output = root / 'output'
                env = {**os.environ, 'PATH': str(root) + os.pathsep + os.environ['PATH'],
                       'GITHUB_EVENT_NAME': event, 'GITHUB_REPOSITORY': 'fixture/repository',
                       'GITHUB_OUTPUT': str(output), 'PULLS_FIXTURE': json.dumps(pulls)}
                result = subprocess.run(['bash', '-e', '-o', 'pipefail', '-c',
                                         script('Decide whether to prepare a release', workflow)],
                                        env=env, capture_output=True, text=True)
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertEqual(output.read_text(), f'prepare={str(expected).lower()}\n')


class VersionTests(unittest.TestCase):
    """Require one production version and pin outputs to the checked-out commit."""

    def test_production_versions_and_commit(self):
        """Ignore unpublished tooling and reject empty or inconsistent releases."""
        program = script('Verify merged commit and aligned production versions')
        program = program.split("python3 - <<'PYCODE'\n", 1)[1].split('\nPYCODE', 1)[0]
        for versions, expected in [(['0.1.2', '0.1.2'], 'v0.1.2'),
                                   (['0.2.0-rc.1'], 'v0.2.0-rc.1'),
                                   (['0.1.2', '0.1.3'], None), ([], None)]:
            with self.subTest(versions=versions), tempfile.TemporaryDirectory() as directory:
                output = Path(directory) / 'output'
                packages = [{'id': str(index), 'version': version, 'publish': None}
                            for index, version in enumerate(versions)]
                packages.append({'id': 'tool', 'version': '9.0.0', 'publish': []})
                metadata = {'packages': packages, 'workspace_members': [p['id'] for p in packages]}
                responses = [json.dumps(metadata).encode(), 'abc123\n']
                with patch.dict(os.environ, {'GITHUB_OUTPUT': str(output)}):
                    with patch('subprocess.check_output', side_effect=responses):
                        if expected is None:
                            with self.assertRaises(SystemExit):
                                exec(program, {})
                            self.assertFalse(output.exists())
                        else:
                            exec(program, {})
                            self.assertEqual(output.read_text(), f'tag={expected}\ncommit=abc123\n')


class PublishingTests(unittest.TestCase):
    """Verify immutable annotated tags and retryable publication decisions."""

    def setUp(self):
        """Create isolated Git history and replace network commands with fixtures."""
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.bin = self.root / 'bin'
        self.bin.mkdir()
        self.real_git = shutil.which('git')
        self.env = {**os.environ, 'PATH': str(self.bin) + os.pathsep + os.environ['PATH'],
                    'REAL_GIT': self.real_git, 'RUNNER_TEMP': str(self.root),
                    'RELEASE_TAG': 'v0.1.2', 'RELEASE_SSH_KEY': 'fixture, not a private key',
                    'GITHUB_REPOSITORY': 'fixture/repository', 'GH_TOKEN': 'fixture',
                    'CARGO_REGISTRY_TOKEN': 'fixture', 'PUSH_LOG': str(self.root / 'push.log'),
                    'PUBLISH_LOG': str(self.root / 'publish.log')}
        subprocess.run([self.real_git, 'init', '--quiet'], cwd=self.root, check=True)
        self.git('config', 'user.name', 'Release test')
        self.git('config', 'user.email', 'release-test@example.invalid')
        self.git('commit', '--quiet', '--allow-empty', '-m', 'Initial fixture')
        self.env['RELEASE_COMMIT'] = self.git('rev-parse', 'HEAD').strip()
        self.command('git', '''if [ "$1" = push ]; then
  printf '%s\\n' "$@" > "$PUSH_LOG"
else
  exec "$REAL_GIT" "$@"
fi
''')
        self.command('gh', "printf '%s\\n' 'github.com ssh-ed25519 fixture'\n")
        self.command('cargo', 'printf "%s\\n" "$*" >> "$PUBLISH_LOG"\n')
        self.command('curl', 'printf "%s" "${REGISTRY_STATUS:-404}"\n')

    def git(self, *args):
        """Inspect or prepare real local Git state without the push fixture."""
        return subprocess.check_output([self.real_git, *args], cwd=self.root, text=True)

    def command(self, name, body):
        """Install one isolated command fixture on the workflow's PATH."""
        path = self.bin / name
        path.write_text('#!/bin/sh\nset -eu\n' + body)
        path.chmod(0o755)

    def run_step(self, name):
        """Execute the actual shell step with all network commands replaced."""
        return subprocess.run(['bash', '-e', '-o', 'pipefail', '-c', script(name)],
                              cwd=self.root, env=self.env, capture_output=True, text=True)

    def test_annotation_retry_and_key_cleanup(self):
        """Create the conventional annotation once and clean temporary key files."""
        result = self.run_step('Create annotated release tag')
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(self.git('cat-file', '-t', 'v0.1.2').strip(), 'tag')
        self.assertEqual(self.git('for-each-ref', '--format=%(contents:subject)',
                                  'refs/tags/v0.1.2').strip(), 'chore: release v0.1.2')
        original = self.git('rev-parse', 'v0.1.2')
        result = self.run_step('Create annotated release tag')
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(self.git('rev-parse', 'v0.1.2'), original)
        self.assertIn('refs/tags/v0.1.2', (self.root / 'push.log').read_text())
        self.assertFalse((self.root / 'release-key').exists())
        self.assertFalse((self.root / 'release-known-hosts').exists())

    def test_conflicting_tag_is_preserved(self):
        """Refuse a version already tagged at a different commit."""
        self.git('tag', '-a', 'v0.1.2', '-m', 'chore: release v0.1.2')
        original = self.git('rev-parse', 'v0.1.2')
        self.git('commit', '--quiet', '--allow-empty', '-m', 'Another fixture')
        self.env['RELEASE_COMMIT'] = self.git('rev-parse', 'HEAD').strip()
        result = self.run_step('Create annotated release tag')
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(self.git('rev-parse', 'v0.1.2'), original)
        self.assertFalse((self.root / 'push.log').exists())

    def test_nonconventional_annotation_is_rejected(self):
        """Reject the wrong annotation even if it points at the right commit."""
        self.git('tag', '-a', 'v0.1.2', '-m', 'Release 0.1.2')
        self.assertNotEqual(self.run_step('Create annotated release tag').returncode, 0)
        self.assertFalse((self.root / 'push.log').exists())

    def test_lightweight_tag_is_rejected(self):
        """Require an annotation even when the tag's commit is correct."""
        self.git('tag', 'v0.1.2')
        self.assertNotEqual(self.run_step('Create annotated release tag').returncode, 0)
        self.assertFalse((self.root / 'push.log').exists())

    def test_registry_results(self):
        """Publish missing versions, skip existing ones and fail on API errors."""
        (self.root / 'publish-order.txt').write_text('poirot-parsing 0.1.2\npoirot 0.1.2\n')
        for status, success, calls in [('404', True, 2), ('200', True, 0), ('503', False, 0)]:
            with self.subTest(status=status):
                log = self.root / 'publish.log'
                log.unlink(missing_ok=True)
                self.env['REGISTRY_STATUS'] = status
                result = self.run_step('Publish production crates in dependency order')
                self.assertEqual(result.returncode == 0, success, result.stderr)
                lines = log.read_text().splitlines() if log.exists() else []
                self.assertEqual(len(lines), calls)
                if calls:
                    self.assertEqual(lines[0], 'publish -p poirot-parsing --locked --no-verify')
                    self.assertEqual(lines[1], 'publish -p poirot --locked --no-verify')

    def test_registry_network_failure_stops_publication(self):
        """A failed lookup must not be interpreted as an unpublished crate."""
        (self.root / 'publish-order.txt').write_text('poirot 0.1.2\n')
        self.command('curl', 'exit 28\n')
        result = self.run_step('Publish production crates in dependency order')
        self.assertNotEqual(result.returncode, 0)
        self.assertFalse((self.root / 'publish.log').exists())


if __name__ == '__main__':
    unittest.main()
