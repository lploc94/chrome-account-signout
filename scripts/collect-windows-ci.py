import hashlib
import json
import pathlib
import shutil
import subprocess
import tempfile
def collect(root):
    installers=list((root/'target/release/bundle/nsis').glob('*.exe'))
    if len(installers)!=1:raise SystemExit('Expected one NSIS installer')
    def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
    tool=shutil.which('7zz') or shutil.which('7z')
    if not tool:raise SystemExit('Missing NSIS payload extractor')
    # Tauri patches bundle-type metadata into the installer payload, then restores
    # the unbundled build output. Record the bytes that will actually be installed.
    with tempfile.TemporaryDirectory(prefix='signout-ci-payload-') as directory:
        subprocess.run([tool,'x','-y','-o'+directory,str(installers[0].resolve())],check=True,capture_output=True)
        binaries=list(pathlib.Path(directory).rglob('chrome-account-signout.exe'))
        if len(binaries)!=1 or binaries[0].is_symlink() or not binaries[0].is_file():raise SystemExit('Expected one app executable in installer')
        binary_hash=sha(binaries[0])
    out=root/'ci-artifact';out.mkdir(exist_ok=True)
    shutil.copyfile(installers[0],out/'ChromeAccountSignout-windows-x64-setup.exe')
    report={'source_commit':subprocess.check_output(['git','rev-parse','HEAD'],cwd=root,text=True).strip(),'installer_sha256':sha(installers[0]),'binary_sha256':binary_hash,'rust':subprocess.check_output(['rustc','-V'],text=True).strip(),'node':subprocess.check_output(['node','-v'],text=True).strip()}
    (out/'windows-build.json').write_text(json.dumps(report,indent=2)+'\n')
    print('Collected installer and exact source/payload hashes')

if __name__=='__main__':
    collect(pathlib.Path(__file__).resolve().parents[1])
