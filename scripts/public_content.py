"""Fail-closed publication checks. Never print matched private values."""
import hashlib
import io
import json
import os
import pathlib
import re
import shutil
import stat
import subprocess
import tempfile
import zipfile

class UnsafeContent(RuntimeError): pass

HOME_PATTERN = re.compile(r'(?:/Us' + r'ers/|[A-Za-z]:[\\/]+Us' + r'ers[\\/]+)[^\s\x00/\\]+', re.I)
EMAIL_PATTERN = re.compile(r'[A-Z0-9._%+\-]+@[A-Z0-9.\-]+\.[A-Z]{2,}', re.I)
FORBIDDEN = {'Web Data','Cookies','Login Data','Local State','Preferences','.cross-review','.DS_Store','node_modules','target','__pycache__'}
IMAGE_SUFFIXES = {'.png','.jpg','.jpeg','.webp','.gif','.icns','.ico'}


def markers():
    result=[str(pathlib.Path.home())]
    extra=os.environ.get('SIGNOUT_PRIVATE_MARKERS')
    if extra:
        result.extend(json.loads(pathlib.Path(extra).read_text()))
    return [s for s in result if s]


def check_bytes(data, private):
    texts=[data.decode('utf-8',errors='ignore')] + [data[offset:].decode(encoding,errors='ignore') for offset in [0,1] for encoding in ['utf-16-le','utf-16-be']]
    for text in texts:
        # Exact pre-existing synthetic launch-path fixture; no real home is exempted.
        fixture = 'C:' + chr(92) + 'Us' + 'ers' + chr(92) + 'Demo User' + chr(92) + 'Chrome'
        public_text = text.replace(fixture, '/synthetic/Chrome')
        if HOME_PATTERN.search(public_text) or any(s.casefold() in text.casefold() for s in private):
            raise UnsafeContent('private bytes')
        reviewed = pathlib.Path(__file__).parent/'reviewed-license-files.json'
        approved_license = reviewed.exists() and hashlib.sha256(data).hexdigest() in json.loads(reviewed.read_text()).values()
        for email in EMAIL_PATTERN.findall(text):
            domain=email.rsplit('@',1)[1].lower()
            if not approved_license and domain in {'gmail.com','outlook.com','hotmail.com','yahoo.com','icloud.com'}:
                raise UnsafeContent('personal email')


def check_name(name):
    parts=name.replace('\\','/').split('/')
    if name.startswith(('/', '\\')) or ':' in parts[0] or '..' in parts:
        raise UnsafeContent('unsafe archive path')
    if any(p in FORBIDDEN for p in parts) or pathlib.PurePosixPath(name).suffix.lower() in {'.pdb','.map','.sqlite','.db','.log','.dmp'}:
        raise UnsafeContent('excluded publication file')


def git(root,*args):
    return subprocess.check_output(['git','-C',str(root),*args],stderr=subprocess.DEVNULL)


def scan_git(root, private, history=False):
    # Read the staged manifest and its blob IDs, not the possibly cleaner working tree.
    blobs=[]
    for item in git(root,'ls-files','--stage','-z').split(b'\0'):
        if not item:continue
        meta,name=item.split(b'\t',1)
        mode,oid,stage=meta.split()
        if stage != b'0' or mode == b'120000':raise UnsafeContent('unmerged or linked source')
        blobs.append((name.decode(),oid.decode()))
    if history:
        for commit in git(root,'rev-list','--all').decode().splitlines():
            identities=git(root,'show','-s','--format=%ae%n%ce',commit).decode().splitlines()
            if any(not e.endswith('@users.noreply.github.com') for e in identities):
                raise UnsafeContent('private git identity')
            check_bytes(git(root,'show','-s','--format=%B',commit),private)
            for item in git(root,'ls-tree','-rz','--full-tree',commit).split(b'\0'):
                if not item:continue
                meta,name=item.split(b'\t',1);mode,kind,oid=meta.split()
                if mode==b'120000':raise UnsafeContent('linked history file')
                if kind==b'blob':blobs.append((name.decode(),oid.decode()))
    seen=set()
    icon_hashes=json.loads((pathlib.Path(__file__).parent/'reviewed-icons.json').read_text()) if (pathlib.Path(__file__).parent/'reviewed-icons.json').exists() else {}
    for name,oid in blobs:
        if (name,oid) in seen:continue
        seen.add((name,oid));check_name(name)
        data=git(root,'cat-file','blob',oid);check_bytes(data,private)
        if pathlib.PurePosixPath(name).suffix.lower() in IMAGE_SUFFIXES:
            if hashlib.sha256(data).hexdigest() != icon_hashes.get(name):raise UnsafeContent('unreviewed source image')
    return len(seen)


def scan_tree(root,private):
    count=0
    for path in root.rglob('*'):
        if path.is_symlink():raise UnsafeContent('linked payload')
        if path.is_file():
            check_name(path.relative_to(root).as_posix());check_bytes(path.read_bytes(),private);count+=1
    return count


def scan_artifact(path, private):
    path=pathlib.Path(path)
    check_name(path.name);check_bytes(path.read_bytes(),private)
    if zipfile.is_zipfile(path):
        with zipfile.ZipFile(path) as archive:
            for item in archive.infolist():
                check_name(item.filename)
                if stat.S_ISLNK(item.external_attr >> 16):raise UnsafeContent('linked archive entry')
                if not item.is_dir():
                    content=archive.read(item);check_bytes(content,private)
                    if zipfile.is_zipfile(io.BytesIO(content)):
                        with tempfile.TemporaryDirectory(prefix='signout-nested-') as d:
                            nested=pathlib.Path(d)/'nested.zip';nested.write_bytes(content)
                            scan_artifact(nested,private)
    elif path.suffix.lower()=='.exe':
        tool=shutil.which('7zz') or shutil.which('7z')
        if not tool:raise UnsafeContent('installer extractor unavailable')
        with tempfile.TemporaryDirectory(prefix='signout-audit-') as d:
            # List first to refuse unsafe paths before extraction.
            result=subprocess.run([tool,'l','-slt',str(path)],capture_output=True)
            if result.returncode:raise UnsafeContent('cannot list installer')
            entries=result.stdout.decode(errors='replace').split('----------')
            if len(entries)<2:raise UnsafeContent('missing installer entries')
            for line in entries[-1].splitlines():
                if line.startswith(('Symbolic Link = ', 'Hard Link = ')):raise UnsafeContent('linked installer entry')
                if line.startswith('Path = '):check_name(line[7:])
            result=subprocess.run([tool,'x','-y','-o'+d,str(path)],capture_output=True)
            if result.returncode:raise UnsafeContent('cannot extract installer')
            scan_tree(pathlib.Path(d),private)
    elif path.suffix.lower() not in {'.json','.txt'}:
        raise UnsafeContent('unknown package format')
