#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
python3 - <<'PY'
import json, subprocess, sys
m=json.loads(subprocess.check_output(['cargo','metadata','--format-version','1','--locked']))
packages={p['id']:p for p in m['packages']}
bad=[p['name'] for p in packages.values() if p['name'].startswith(('webkit','serde_yaml')) or p['name'] in ('libadwaita','relm4','tokio','async-std')]
# Nucleo unconditionally uses rayon internally; no other crate may introduce it.
graph={n['id']:[d['pkg'] for d in n['deps']] for n in m['resolve']['nodes']}
bad += [packages[parent]['name']+' -> rayon' for parent,deps in graph.items() if packages[parent]['name']!='nucleo' and any(packages[d]['name']=='rayon' for d in deps)]
def closure(root):
    seen=set(); todo=[root]
    while todo:
        node=todo.pop()
        if node not in seen:
            seen.add(node); todo.extend(graph.get(node,[]))
    return {packages[p]['name'] for p in seen}
for p in packages.values():
    if p['name']=='nib-core':
        bad += ['core -> '+n for n in closure(p['id']) if n in ('regex','glib','gio','gtk4','gdk4','gdk-pixbuf','nucleo','rayon')]
    elif p['name']=='nib-vault':
        bad += ['vault -> '+n for n in closure(p['id']) if n in ('gtk4','gdk4','gdk-pixbuf','sourceview5')]
if bad: sys.exit('Forbidden dependencies: '+', '.join(sorted(set(bad))))
print('Dependency boundaries: clean')
PY
