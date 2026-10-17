import hashlib
import json
import pathlib
import shutil
import subprocess
root=pathlib.Path(__file__).resolve().parents[1]
installers=list((root/'target/release/bundle/nsis').glob('*.exe'))
if len(installers)!=1:raise SystemExit('Expected one NSIS installer')
binary=root/'target/release/chrome-account-signout.exe'
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
out=root/'ci-artifact';out.mkdir(exist_ok=True)
shutil.copyfile(installers[0],out/'ChromeAccountSignout-windows-x64-setup.exe')
report={'source_commit':subprocess.check_output(['git','rev-parse','HEAD'],cwd=root,text=True).strip(),'installer_sha256':sha(installers[0]),'binary_sha256':sha(binary),'rust':subprocess.check_output(['rustc','-V'],text=True).strip(),'node':subprocess.check_output(['node','-v'],text=True).strip()}
(out/'windows-build.json').write_text(json.dumps(report,indent=2)+'\n')
print('Collected installer and exact source/payload hashes')
