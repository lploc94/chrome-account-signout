"""Build using reproducible public source prefixes instead of local home paths."""
import os
import pathlib
import subprocess
import sys
root=pathlib.Path(__file__).resolve().parents[1]
env=os.environ.copy()
remaps=[(root,'/src/chrome-account-signout'),(pathlib.Path.home(),'/build-home')]
env['CARGO_ENCODED_RUSTFLAGS']='\x1f'.join(['--remap-path-prefix='+str(a)+'='+b for a,b in remaps])
env['SOURCE_DATE_EPOCH']=subprocess.check_output(['git','show','-s','--format=%ct','HEAD'],cwd=root,text=True).strip()
cmd=['npm.cmd' if os.name=='nt' else 'npm','run','tauri','build','--',*sys.argv[1:]]
sys.exit(subprocess.call(cmd,cwd=root,env=env))
