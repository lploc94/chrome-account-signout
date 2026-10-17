"""Maintainer-only license collection; review hashes/privacy before publication."""
import concurrent.futures,hashlib,json,pathlib,subprocess,urllib.request
root=pathlib.Path(__file__).resolve().parents[1]
metadata=json.loads(subprocess.check_output(['cargo','metadata','--locked','--format-version','1'],stderr=subprocess.DEVNULL))
groups=[]
for package in metadata['packages']:
    if not package['source']:continue
    base=pathlib.Path(package['manifest_path']).parent
    if any(f.is_file() and f.name.upper().startswith(('LICENSE','LICENCE','COPYING','NOTICE','COPYRIGHT')) for f in base.rglob('*')):continue
    repo=package.get('repository');vcs=base/'.cargo_vcs_info.json'
    if not repo or 'github.com/' not in repo or not vcs.exists():continue
    repository=repo.split('github.com/')[1].removesuffix('.git')
    commit=json.loads(vcs.read_text()).get('git',{}).get('sha1')
    group=next((g for g in groups if g['repo']==repository and g['commit']==commit),None)
    if group is None:
        group={'repo':repository,'commit':commit,'packages':[]};groups.append(group)
    group['packages'].append(package['name'])
def get_group(group):
    repo=group['repo'];commit=group['commit']
    result=subprocess.run(['gh','api',f'repos/{repo}/git/trees/{commit}?recursive=1'],capture_output=True)
    if result.returncode:return group,[]
    tree=json.loads(result.stdout).get('tree',[])
    paths=[x['path'] for x in tree if x['type']=='blob' and '/' not in x['path'] and x['path'].upper().startswith(('LICENSE','LICENCE','COPYING','COPYRIGHT','NOTICE'))]
    files=[]
    for path in paths:
        url=f'https://raw.githubusercontent.com/{repo}/{commit}/{path}'
        try:files.append((url,urllib.request.urlopen(url,timeout=30).read().decode('utf-8')))
        except Exception:pass
    return group,files
with concurrent.futures.ThreadPoolExecutor(max_workers=6) as pool:upstreams=list(pool.map(get_group,groups))
licenses={};catalog=[];missing=[]
for p in metadata['packages']:
    if not p['source']:continue
    base=pathlib.Path(p['manifest_path']).parent
    files=[f for f in base.rglob('*') if f.is_file() and f.name.upper().startswith(('LICENSE','LICENCE','COPYING','NOTICE','COPYRIGHT'))]
    content=[]
    for file in files:
        try:content.append((f'crate {p["name"]} {p["version"]}: {file.relative_to(base)}',file.read_text()))
        except UnicodeError:pass
    if not content:
        for group,entries in upstreams:
            if p['name'] in group['packages']:content.extend(entries)
    ids=[]
    for source,text in content:
        key=hashlib.sha256(text.encode()).hexdigest()[:16]
        licenses.setdefault(key,(source,text));ids.append(key)
    if not ids:missing.append(p['name'])
    catalog.append(f'{p["name"]} {p["version"]} | {p.get("license") or "see upstream"} | {p.get("repository") or p.get("homepage") or "https://crates.io/crates/"+p["name"]} | texts: {", ".join(dict.fromkeys(ids)) or "SPDX metadata only (not a runtime dependency on supported platforms)"}')
# Runtime JavaScript adapter; bundler/test tooling is not shipped.
for file in (root/'node_modules/@tauri-apps/api').glob('LICENSE*'):
    text=file.read_text();key=hashlib.sha256(text.encode()).hexdigest()[:16];licenses[key]=('@tauri-apps/api '+file.name,text)
    catalog.append('@tauri-apps/api | MIT OR Apache-2.0 | text: '+key)
output=['Third-party software notices','This catalog includes build and other-platform crates, in addition to runtime dependencies.','Project source licensing is not set by these dependency notices.','',*sorted(catalog),'']
for key,(source,text) in sorted(licenses.items()):output.extend(['='*72,f'Text {key} — {source}',text,''])
(root/'THIRD-PARTY-NOTICES.txt').write_text('\n'.join(output))
print('Notices:',len(catalog),'packages,',len(licenses),'distinct license texts; metadata-only:',missing)
