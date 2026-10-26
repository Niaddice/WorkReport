"""Release integration tests: real temporary Git history, mocked builds/GitHub, no network."""
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]
REAL_GIT = shutil.which('git')
BASH = '/bin/bash' if Path('/bin/bash').exists() else shutil.which('bash')

FAKE_TOOL = r'''#!/usr/bin/env python3
import json, os, plistlib, subprocess, sys, time
from pathlib import Path
tool = Path(sys.argv[0]).name
args = sys.argv[1:]
root = Path(os.environ['RELEASE_TEST_PROJECT'])
state_file = Path(os.environ['RELEASE_TEST_STATE'])
with open(os.environ['RELEASE_TEST_CALLS'], 'a') as log:
    log.write(json.dumps([tool, *args]) + '\n')
if tool == 'git':
    command = [os.environ['RELEASE_TEST_GIT'], '-c', 'user.name=Release Test', '-c', 'user.email=release@example.invalid', '-c', 'protocol.allow=never', '-c', 'protocol.file.allow=always']
    if any(a in ('fetch', 'push', 'ls-remote') for a in args):
        command += ['-c', 'url.' + Path(os.environ['RELEASE_TEST_REMOTE']).as_uri() + '.insteadOf=https://github.com/Niaddice/WorkReport.git']
    sys.exit(subprocess.call(command + args))
if tool == 'uname':
    print(os.environ.get('RELEASE_TEST_PLATFORM', 'Darwin') if args == ['-s'] else 'arm64')
elif tool == 'npm':
    if args == ['test'] and os.environ.get('RELEASE_TEST_FAIL_TESTS'):
        sys.exit(7)
    if args[:3] == ['run', 'tauri', '--']:
        if os.environ.get('RELEASE_TEST_FAIL_BUILD'):
            sys.exit(13)
        version = json.loads((root / 'package.json').read_text())['version']
        bundle = root / 'src-tauri/target/release/bundle'
        time.sleep(.02)
        platform = os.environ.get('RELEASE_TEST_PLATFORM', 'Darwin')
        if platform == 'Darwin':
            app = bundle / 'macos/WorkReport.app/Contents'
            app.mkdir(parents=True, exist_ok=True)
            (app / 'Info.plist').write_bytes(plistlib.dumps({'CFBundleShortVersionString': version}))
            (app / 'fake-binary').write_text('local build fixture')
        else:
            files = ['deb/WorkReport_' + version + '_arm64.deb', 'appimage/WorkReport_' + version + '_arm64.AppImage'] if platform == 'Linux' else ['msi/WorkReport_' + version + '_x64.msi', 'nsis/WorkReport_' + version + '_setup.exe']
            for file in files:
                path = bundle / file
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text('local build fixture')
elif tool in ('cargo', 'rustc'):
    pass
elif tool == 'codesign':
    if '--verify' in args:
        sys.exit(1 if os.environ.get('RELEASE_TEST_FAIL_CODESIGN') else 0)
    if '-dv' in args or '--display' in args:
        if os.environ.get('RELEASE_TEST_ADHOC', '1') == '0':
            print('Authority=Developer ID Application: Release Test (TEAMID)')
        else:
            print('Signature=adhoc')
        sys.exit(0)
    sys.exit(0)
elif tool == 'spctl':
    sys.exit(1 if os.environ.get('RELEASE_TEST_FAIL_SPCTL') else 0)
elif tool == 'gh':
    state = json.loads(state_file.read_text()) if state_file.exists() else {}
    def save(): state_file.write_text(json.dumps(state))
    if args[:2] == ['auth', 'status']:
        pass
    elif args[:2] == ['repo', 'view']:
        print(json.dumps({'nameWithOwner': 'Niaddice/WorkReport', 'defaultBranchRef': {'name': 'main'}, 'viewerPermission': 'ADMIN'}))
    elif args[:1] == ['api']:
        print('\n'.join(state))
    elif args[:2] == ['release', 'create']:
        tag = args[2]
        assets = args[3:args.index('--repo')]
        assert all(Path(a).is_file() for a in assets), assets
        target = args[args.index('--target') + 1]
        actual = subprocess.check_output([os.environ['RELEASE_TEST_GIT'], '--git-dir', os.environ['RELEASE_TEST_REMOTE'], 'rev-parse', tag + '^{commit}'], text=True).strip()
        assert actual == target
        body = Path(args[args.index('--notes-file') + 1]).read_text() if '--notes-file' in args else 'generated notes'
        state[tag] = {'draft': '--draft' in args or bool(os.environ.get('RELEASE_TEST_FAIL_RELEASE')), 'assets': assets, 'body': body}
        save()
        if os.environ.get('RELEASE_TEST_FAIL_RELEASE'): sys.exit(17)
    elif args[:2] == ['release', 'view']:
        print(str(state[args[2]]['draft']).lower())
    elif args[:2] == ['release', 'upload']:
        assert state[args[2]]['draft'], 'must not overwrite published assets'
        state[args[2]]['assets'] = args[3:args.index('--repo')]
        save()
    elif args[:2] == ['release', 'edit']:
        assert '--draft=false' in args
        state[args[2]]['draft'] = False
        save()
    else:
        raise AssertionError(args)
else:
    raise AssertionError(tool)
'''


class ReleaseTest(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix='workreport-release-', dir='/private/tmp' if Path('/private/tmp').exists() else None)
        self.addCleanup(self.temp.cleanup)
        self.base = Path(self.temp.name)
        self.project = self.base / 'project with spaces'
        self.project.mkdir()
        for file in ('release.sh', '.gitignore', 'package.json', 'package-lock.json', 'src-tauri/tauri.conf.json', 'src-tauri/Cargo.toml', 'src-tauri/Cargo.lock'):
            dest = self.project / file
            dest.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(ROOT / file, dest)
        (self.project / 'README.md').write_text('local project readme\n')
        self.remote = self.base / 'remote.git'
        self.real_git('init', '--bare', '--initial-branch=main', str(self.remote))
        seed = self.base / 'seed'
        seed.mkdir()
        self.real_git('init', '--initial-branch=main', str(seed))
        (seed / '.gitignore').write_text('remote-cache/\n')
        (seed / 'remote-only.txt').write_text('preserve this file and its history\n')
        self.real_git('-C', str(seed), 'add', '--all')
        self.real_git('-C', str(seed), 'commit', '-m', 'remote initial commit')
        self.real_git('-C', str(seed), 'push', str(self.remote), 'main')
        self.initial_commit = self.real_git('--git-dir', str(self.remote), 'rev-parse', 'main').strip()
        tools = self.base / 'bin'
        tools.mkdir()
        for tool in ('git', 'gh', 'npm', 'cargo', 'rustc', 'uname', 'codesign', 'spctl'):
            script = tools / tool
            script.write_text(FAKE_TOOL)
            script.chmod(0o755)
        self.env = dict(os.environ, PATH=str(tools) + os.pathsep + os.environ['PATH'],
                        GIT_CONFIG_GLOBAL=os.devnull, GIT_CONFIG_NOSYSTEM='1', GIT_ALLOW_PROTOCOL='file',
                        RELEASE_TEST_GIT=REAL_GIT, RELEASE_TEST_PROJECT=str(self.project),
                        RELEASE_TEST_REMOTE=str(self.remote), RELEASE_TEST_STATE=str(self.base / 'github.json'),
                        RELEASE_TEST_CALLS=str(self.base / 'calls.jsonl'), RELEASE_TEST_PLATFORM='Linux')

    def real_git(self, *args):
        env = dict(os.environ, GIT_CONFIG_GLOBAL=os.devnull, GIT_CONFIG_NOSYSTEM='1', GIT_ALLOW_PROTOCOL='file')
        return subprocess.check_output([REAL_GIT, '-c', 'user.name=Release Test', '-c', 'user.email=release@example.invalid', *args], env=env, text=True, stderr=subprocess.DEVNULL)

    def run_release(self, *args, success=True, **extra):
        result = subprocess.run([BASH, str(self.project / 'release.sh'), *args], cwd=self.base,
                                env=dict(self.env, **extra), text=True, errors='backslashreplace', stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
        if success:
            self.assertEqual(result.returncode, 0, result.stdout)
        else:
            self.assertNotEqual(result.returncode, 0, result.stdout)
        return result.stdout

    def state(self):
        file = self.base / 'github.json'
        return json.loads(file.read_text()) if file.exists() else {}

    def calls(self):
        file = self.base / 'calls.jsonl'
        return [json.loads(line) for line in file.read_text().splitlines()] if file.exists() else []

    def test_dry_run_is_offline_and_does_not_mutate_the_project(self):
        before = {str(p.relative_to(self.project)): p.read_bytes() for p in self.project.rglob('*') if p.is_file()}
        self.run_release('v0.2.0', '--dry-run')
        after = {str(p.relative_to(self.project)): p.read_bytes() for p in self.project.rglob('*') if p.is_file()}
        self.assertEqual(before, after)
        self.assertFalse((self.project / '.git').exists())
        self.assertEqual(self.calls(), [])

    def test_bootstrap_preserves_remote_history_and_synchronizes_versions(self):
        notes = self.base / 'notes.md'
        notes.write_text('更新说明\n\n- 支持模型选择\n')
        message = 'release literal $(touch DO_NOT_CREATE) `whoami`'
        self.run_release('0.2.0', '-m', message, '--notes-file', str(notes))
        self.assertEqual((self.project / 'remote-only.txt').read_text(), 'preserve this file and its history\n')
        self.real_git('-C', str(self.project), 'merge-base', '--is-ancestor', self.initial_commit, 'HEAD')
        self.assertEqual(self.real_git('-C', str(self.project), 'log', '-1', '--pretty=%s').strip(), message)
        self.assertFalse((self.project / 'DO_NOT_CREATE').exists())
        for file in ('package.json', 'package-lock.json', 'src-tauri/tauri.conf.json'):
            self.assertEqual(json.loads((self.project / file).read_text())['version'], '0.2.0')
        self.assertIn('version = "0.2.0"', (self.project / 'src-tauri/Cargo.toml').read_text())
        self.assertIn('name = "work-report"\nversion = "0.2.0"', (self.project / 'src-tauri/Cargo.lock').read_text())
        self.assertEqual(self.state()['v0.2.0']['body'], notes.read_text())
        self.assertFalse(self.state()['v0.2.0']['draft'])
        self.assertTrue(all(Path(a).exists() for a in self.state()['v0.2.0']['assets']))
        tracked = self.real_git('-C', str(self.project), 'ls-files')
        self.assertNotIn('.release/', tracked)
        self.assertEqual(self.real_git('--git-dir', str(self.remote), 'rev-parse', 'main').strip(), self.real_git('-C', str(self.project), 'rev-parse', 'v0.2.0^{commit}').strip())

    def test_default_version_increments_and_explicit_duplicate_is_rejected(self):
        # Keep this scenario independent of the application's current release version.
        manifest = self.project / 'package.json'
        package = json.loads(manifest.read_text())
        package['version'] = '0.1.0'
        manifest.write_text(json.dumps(package))
        self.run_release()
        self.run_release()
        self.assertEqual(set(self.state()), {'v0.1.0', 'v0.1.1'})
        head = self.real_git('-C', str(self.project), 'rev-parse', 'HEAD')
        output = self.run_release('0.1.1', success=False)
        self.assertIn('已存在', output)
        self.assertEqual(head, self.real_git('-C', str(self.project), 'rev-parse', 'HEAD'))

    def test_failed_tests_never_push_or_publish(self):
        self.run_release('0.3.0', success=False, RELEASE_TEST_FAIL_TESTS='1')
        self.assertEqual(self.state(), {})
        self.assertEqual(self.real_git('--git-dir', str(self.remote), 'rev-parse', 'main').strip(), self.initial_commit)
        self.assertFalse(any(call[0] == 'git' and 'push' in call for call in self.calls()))

    def test_failed_build_never_pushes_or_publishes(self):
        self.run_release('0.3.1', success=False, RELEASE_TEST_FAIL_BUILD='1')
        self.assertEqual(self.state(), {})
        self.assertEqual(self.real_git('--git-dir', str(self.remote), 'rev-parse', 'main').strip(), self.initial_commit)
        self.assertFalse(any(call[0] == 'git' and 'push' in call for call in self.calls()))

    def test_failed_upload_can_resume_without_rebuilding(self):
        output = self.run_release('0.4.0', success=False, RELEASE_TEST_FAIL_RELEASE='1')
        self.assertIn('--resume', output)
        builds = sum(call[:4] == ['npm', 'run', 'tauri', '--'] for call in self.calls())
        self.assertTrue(self.state()['v0.4.0']['draft'])
        self.run_release('--resume', 'v0.4.0')
        self.assertFalse(self.state()['v0.4.0']['draft'])
        self.assertEqual(builds, sum(call[:4] == ['npm', 'run', 'tauri', '--'] for call in self.calls()))

    def test_resume_verifies_checksums_and_preserves_draft_intent(self):
        self.run_release('0.5.0', '--draft', success=False, RELEASE_TEST_FAIL_RELEASE='1')
        self.run_release('--resume', 'v0.5.0')
        self.assertTrue(self.state()['v0.5.0']['draft'])
        asset = Path(self.state()['v0.5.0']['assets'][0])
        asset.write_text('corrupt artifact')
        self.run_release('--resume', 'v0.5.0', success=False)
        self.assertTrue(self.state()['v0.5.0']['draft'])

    def test_remote_changes_are_not_overwritten(self):
        self.run_release('0.6.0')
        clone = self.base / 'another checkout'
        self.real_git('clone', str(self.remote), str(clone))
        (clone / 'new-remote-file').write_text('remote work')
        self.real_git('-C', str(clone), 'add', '--all')
        self.real_git('-C', str(clone), 'commit', '-m', 'new remote work')
        self.real_git('-C', str(clone), 'push', 'origin', 'main')
        output = self.run_release('0.7.0', success=False)
        self.assertIn('尚未合并', output)
        self.assertEqual(json.loads((self.project / 'package.json').read_text())['version'], '0.6.0')

    def test_wrong_push_remote_is_rejected(self):
        self.run_release('0.8.0')
        self.real_git('-C', str(self.project), 'config', 'remote.origin.pushurl', 'https://example.invalid/other.git')
        output = self.run_release('0.9.0', success=False)
        self.assertIn('push 地址不是', output)
        self.assertEqual(set(self.state()), {'v0.8.0'})

    @unittest.skipUnless(Path('/usr/libexec/PlistBuddy').exists() and shutil.which('ditto'), 'macOS packaging tools required')
    def test_macos_archive_contains_the_app_bundle(self):
        output = self.run_release('0.10.0', RELEASE_TEST_PLATFORM='Darwin')
        self.assertIn('ad-hoc', output)
        self.assertIn('xattr -dr com.apple.quarantine', output)
        import zipfile
        asset = next(Path(a) for a in self.state()['v0.10.0']['assets'] if a.endswith('.app.zip'))
        with zipfile.ZipFile(asset) as archive:
            self.assertIn('WorkReport.app/Contents/Info.plist', archive.namelist())

    @unittest.skipUnless(Path('/usr/libexec/PlistBuddy').exists() and shutil.which('ditto'), 'macOS packaging tools required')
    def test_macos_broken_signature_never_publishes(self):
        output = self.run_release('0.12.0', success=False, RELEASE_TEST_PLATFORM='Darwin', RELEASE_TEST_FAIL_CODESIGN='1')
        self.assertIn('代码签名无效', output)
        self.assertEqual(self.state(), {})
        self.assertEqual(self.real_git('--git-dir', str(self.remote), 'rev-parse', 'main').strip(), self.initial_commit)

    @unittest.skipUnless(Path('/usr/libexec/PlistBuddy').exists() and shutil.which('ditto'), 'macOS packaging tools required')
    def test_macos_unnotarized_developer_id_never_publishes(self):
        output = self.run_release('0.13.0', success=False, RELEASE_TEST_PLATFORM='Darwin', RELEASE_TEST_ADHOC='0', RELEASE_TEST_FAIL_SPCTL='1')
        self.assertIn('未通过 Gatekeeper', output)
        self.assertEqual(self.state(), {})
        self.assertEqual(self.real_git('--git-dir', str(self.remote), 'rev-parse', 'main').strip(), self.initial_commit)

    def test_windows_installer_assets_are_collected(self):
        self.run_release('0.11.0', RELEASE_TEST_PLATFORM='MINGW64_NT')
        assets = self.state()['v0.11.0']['assets']
        self.assertTrue(any(a.endswith('.exe') for a in assets))
        self.assertTrue(any(a.endswith('.msi') for a in assets))


if __name__ == '__main__':
    unittest.main(verbosity=2)
