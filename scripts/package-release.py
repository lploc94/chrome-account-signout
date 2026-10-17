import argparse
import hashlib
import json
import pathlib
import shutil
import subprocess
import zipfile
from public_content import scan_artifact, markers
root=pathlib.Path(__file__).resolve().parents[1]
p=argparse.ArgumentParser();p.add_argument('--platform',choices=['macos','windows'],required=True);p.add_argument('--input',type=pathlib.Path);p.add_argument('--expected-commit');a=p.parse_args()
dist=root/'dist';dist.mkdir(exist_ok=True)
def sha(path):return hashlib.sha256(path.read_bytes()).hexdigest()
commit=subprocess.check_output(['git','rev-parse','HEAD'],cwd=root,text=True).strip()
mfile=dist/'release-manifest.json'
manifest=json.loads(mfile.read_text()) if mfile.exists() else {'version':'1.0.0','source_commit':commit,'assets':[]}
if manifest['source_commit']!=commit:raise SystemExit('Manifest source mismatch; use a fresh output directory')
if a.platform=='macos':
    app=root/'target/universal-apple-darwin/release/bundle/macos/Chrome Account Signout.app'
    binary=app/'Contents/MacOS/chrome-account-signout'
    arch=subprocess.check_output(['lipo','-archs',str(binary)],text=True).split()
    if set(arch)!={'arm64','x86_64'}:raise SystemExit('Universal slices missing')
    subprocess.run(['codesign','--verify','--strict','--deep',str(app)],check=True)
    output=dist/'ChromeAccountSignout-macos-universal.zip'
    with zipfile.ZipFile(output,'w',compression=zipfile.ZIP_DEFLATED) as z:
        for f in sorted(app.rglob('*')):
            if f.is_symlink():raise SystemExit('Unexpected app symlink')
            if f.is_file():z.write(f,f.relative_to(app.parent))
    entry={'name':output.name,'platform':'macos','sha256':sha(output),'bytes':output.stat().st_size,'binary_sha256':sha(binary),'architectures':sorted(arch)}
else:
    if not a.input or a.expected_commit!=commit:raise SystemExit('Exact source commit required')
    report=json.loads((a.input/'windows-build.json').read_text())
    if report['source_commit']!=commit:raise SystemExit('CI source mismatch')
    source=a.input/'ChromeAccountSignout-windows-x64-setup.exe'
    if sha(source)!=report['installer_sha256']:raise SystemExit('CI installer mismatch')
    output=dist/source.name;shutil.copyfile(source,output)
    entry={'name':output.name,'platform':'windows','sha256':sha(output),'bytes':output.stat().st_size,'binary_sha256':report['binary_sha256'],'experimental':True}
scan_artifact(output,markers())
manifest['assets']=[x for x in manifest['assets'] if x['platform']!=a.platform]+[entry]
mfile.write_text(json.dumps(manifest,indent=2)+'\n')
(dist/'SHA256SUMS.txt').write_text(''.join(x['sha256']+'  '+x['name']+'\n' for x in manifest['assets']))
print('Packaged and audited:',output.name,entry['bytes'],'bytes')
