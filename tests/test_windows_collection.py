import hashlib
import importlib.util
import json
import pathlib
import subprocess
import tempfile
import unittest
from unittest import mock

spec=importlib.util.spec_from_file_location('windows_collection',pathlib.Path(__file__).parents[1]/'scripts/collect-windows-ci.py')
collector=importlib.util.module_from_spec(spec)
spec.loader.exec_module(collector)

class WindowsCollectionTests(unittest.TestCase):
    def collect_fixture(self, mode='normal'):
        temporary=tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        root=pathlib.Path(temporary.name)
        directory=root/'target/release/bundle/nsis';directory.mkdir(parents=True)
        (directory/'setup.exe').write_bytes(b'installer fixture')
        (root/'target/release/chrome-account-signout.exe').write_bytes(b'unbundled executable')
        payload=b'executable patched for nsis package'
        def extract(command,**kwargs):
            destination=pathlib.Path(next(arg[2:] for arg in command if arg.startswith('-o')))
            if mode!='missing':(destination/'chrome-account-signout.exe').write_bytes(payload)
            if mode=='duplicate':
                nested=destination/'nested';nested.mkdir()
                (nested/'chrome-account-signout.exe').write_bytes(b'other executable')
            return subprocess.CompletedProcess(command,0)
        def output(command,**kwargs):
            return {'git':'a'*40+'\n','rustc':'rustc test\n','node':'v24.test\n'}[command[0]]
        with mock.patch.object(collector.shutil,'which',return_value='fixture-7z'),mock.patch.object(collector.subprocess,'run',side_effect=extract),mock.patch.object(collector.subprocess,'check_output',side_effect=output):
            collector.collect(root)
        return root,payload

    def test_reports_installed_payload_instead_of_unbundled_build_binary(self):
        root,payload=self.collect_fixture()
        report=json.loads((root/'ci-artifact/windows-build.json').read_text())
        self.assertEqual(report['binary_sha256'],hashlib.sha256(payload).hexdigest())
        self.assertNotEqual(report['binary_sha256'],hashlib.sha256(b'unbundled executable').hexdigest())
        self.assertEqual(report['installer_sha256'],hashlib.sha256(b'installer fixture').hexdigest())

    def test_missing_or_ambiguous_payload_is_rejected(self):
        for mode in ['missing','duplicate']:
            with self.subTest(mode=mode),self.assertRaises(SystemExit):self.collect_fixture(mode)
