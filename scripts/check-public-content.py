import argparse
import pathlib
import sys
from public_content import UnsafeContent, markers, scan_git, scan_artifact
p=argparse.ArgumentParser()
p.add_argument('--source',action='store_true');p.add_argument('--history',action='store_true');p.add_argument('--artifacts',type=pathlib.Path)
a=p.parse_args()
try:
    private=markers();count=0
    if a.source or a.history:count+=scan_git(pathlib.Path.cwd(),private,a.history)
    if a.artifacts:
        if not a.artifacts.is_dir():raise UnsafeContent('artifact directory missing')
        for file in a.artifacts.iterdir():
            if not file.is_file() or file.is_symlink():raise UnsafeContent('unexpected artifact entry')
            scan_artifact(file,private);count+=1
    if not(a.source or a.history or a.artifacts):p.error('select source/history/artifacts')
    print(f'Privacy audit PASS: {count} entries')
except (UnsafeContent,OSError,ValueError) as e:
    print(f'Privacy audit FAILED: {type(e).__name__}; no content printed',file=sys.stderr);sys.exit(1)
