import importlib.util
import io
import pathlib
import subprocess
import tempfile
import unittest
import zipfile

spec = importlib.util.spec_from_file_location('audit', pathlib.Path(__file__).parents[1]/'scripts/public_content.py')
audit = importlib.util.module_from_spec(spec)
spec.loader.exec_module(audit)

class PrivacyTests(unittest.TestCase):
    def test_private_bytes_utf8_utf16(self):
        for value in [('/Us'+'ers/private-owner/project/main.rs'), ('C:' + chr(92) + 'Us'+'ers' + chr(92) + 'private-owner' + chr(92) + 'build'), 'person@private.invalid']:
            for encoding in ['utf-8', 'utf-16-le']:
                with self.subTest(value=value, encoding=encoding):
                    with self.assertRaises(audit.UnsafeContent):
                        audit.check_bytes(value.encode(encoding), ['person@private.invalid'])
        audit.check_bytes(b'friend@example.test https://github.com/lploc94/chrome-account-signout', [])

    def test_archive_payload_and_browser_file(self):
        with tempfile.TemporaryDirectory() as d:
            p=pathlib.Path(d)/'release.zip'
            for name, data in [('App/Contents/MacOS/app', ('/Us'+'ers/private-owner/secret').encode()), ('App/Web Data', b'sqlite'), ('../escape', b'abc')]:
                with zipfile.ZipFile(p,'w') as z: z.writestr(name,data)
                with self.assertRaises(audit.UnsafeContent): audit.scan_artifact(p, [])
            with zipfile.ZipFile(p,'w') as z: z.writestr('App/Contents/MacOS/app',b'safe binary')
            audit.scan_artifact(p, [])

    def test_tracked_blobs_and_history_are_scanned(self):
        with tempfile.TemporaryDirectory() as d:
            p=pathlib.Path(d)
            def git(*args):return subprocess.check_output(['git','-C',d,*args],stderr=subprocess.DEVNULL)
            git('init','-q');git('config','user.name','Test');git('config','user.email','test@users.noreply.github.com')
            (p/'example.txt').write_text(('/Us'+'ers/private-owner/secret'))
            git('add','.');git('commit','-qm','initial')
            (p/'example.txt').write_text('safe')
            git('add','.');git('commit','-qm','fix')
            with self.assertRaises(audit.UnsafeContent):audit.scan_git(p, [], history=True)
            audit.scan_git(p, [], history=False)
            (p/'example.txt').write_text('safe working tree')
            git('show','HEAD:example.txt')
            (p/'example.txt').write_text('person@private.invalid');git('add','.')
            (p/'example.txt').write_text('safe working tree')
            with self.assertRaises(audit.UnsafeContent):audit.scan_git(p, ['person@private.invalid'], history=False)

    def test_private_git_author_is_rejected(self):
        with tempfile.TemporaryDirectory() as d:
            def git(*args):return subprocess.check_output(['git','-C',d,*args],stderr=subprocess.DEVNULL)
            git('init','-q');git('config','user.name','Test');git('config','user.email','person@private.invalid')
            p=pathlib.Path(d);(p/'safe').write_text('safe');git('add','.');git('commit','-qm','initial')
            with self.assertRaises(audit.UnsafeContent):audit.scan_git(p, [], history=True)

    def test_nested_archive_and_unaligned_wide_bytes(self):
        private='/Us'+'ers/private-owner/secret'
        with self.assertRaises(audit.UnsafeContent): audit.check_bytes(b'x'+private.encode('utf-16-le'), [])
        with tempfile.TemporaryDirectory() as d:
            inner=io.BytesIO()
            with zipfile.ZipFile(inner,'w',compression=zipfile.ZIP_DEFLATED) as z:z.writestr('payload',private)
            outer=pathlib.Path(d)/'outer.zip'
            with zipfile.ZipFile(outer,'w',compression=zipfile.ZIP_DEFLATED) as z:z.writestr('nested.zip',inner.getvalue())
            with self.assertRaises(audit.UnsafeContent):audit.scan_artifact(outer, [])
