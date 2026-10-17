import argparse
import hashlib
import json
import pathlib
import shutil
import subprocess
import tempfile
import zipfile
from public_content import scan_artifact, markers, UnsafeContent
p=argparse.ArgumentParser();p.add_argument('--dir',type=pathlib.Path,required=True);p.add_argument('--manifest',type=pathlib.Path);a=p.parse_args()
manifest=json.loads((a.manifest or a.dir/'release-manifest.json').read_text())
expected={x['name'] for x in manifest['assets']}|{'release-manifest.json','SHA256SUMS.txt'}
if {p.name for p in a.dir.iterdir()}!=expected:raise SystemExit('Unexpected/missing release files')
private=markers()
checksums=''
for asset in manifest['assets']:
    path=a.dir/asset['name']
    if path.name!=asset['name'] or path.is_symlink():raise SystemExit('Invalid asset path')
    if hashlib.sha256(path.read_bytes()).hexdigest()!=asset['sha256'] or path.stat().st_size!=asset['bytes']:raise SystemExit('Asset hash/size mismatch')
    scan_artifact(path,private)
    checksums+=asset['sha256']+'  '+asset['name']+'\n'
    with tempfile.TemporaryDirectory(prefix='signout-verify-') as d:
        dest=pathlib.Path(d)
        if asset['platform']=='macos':
            with zipfile.ZipFile(path) as z:z.extractall(dest)
            apps=list(dest.glob('*.app'))
            if len(apps)!=1:raise SystemExit('Expected one app')
            binary=apps[0]/'Contents/MacOS/chrome-account-signout'
            if set(subprocess.check_output(['lipo','-archs',str(binary)],text=True).split())!={'arm64','x86_64'}:raise SystemExit('Missing slices')
            subprocess.run(['codesign','--verify','--strict','--deep',str(apps[0])],check=True,capture_output=True)
        elif asset['platform']=='windows':
            tool=shutil.which('7zz') or shutil.which('7z')
            if not tool:raise SystemExit('Missing payload extractor')
            subprocess.run([tool,'x','-y','-o'+d,str(path.resolve())],check=True,capture_output=True)
            binaries=list(dest.rglob('chrome-account-signout.exe'))
            if len(binaries)!=1:raise SystemExit('Expected one app executable in installer')
            binary=binaries[0]
        else:raise SystemExit('Unknown platform')
        if hashlib.sha256(binary.read_bytes()).hexdigest()!=asset['binary_sha256']:raise SystemExit('Payload hash mismatch')
if (a.dir/'SHA256SUMS.txt').read_text()!=checksums:raise SystemExit('Checksum list mismatch')
if json.loads((a.dir/'release-manifest.json').read_text())!=manifest:raise SystemExit('Downloaded manifest mismatch')
print('Release verification PASS:',len(manifest['assets']),'packages including extracted payloads')
