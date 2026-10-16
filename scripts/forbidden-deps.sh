#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
python3 - <<'PY'
import json, subprocess, sys
m=json.loads(subprocess.check_output(['cargo','metadata','--format-version','1','--locked']))
bad=[p['name'] for p in m['packages'] if p['name'].startswith(('webkit','serde_yaml')) or p['name'] in ('libadwaita','relm4','tokio','async-std')]
bad += [d['name'] for p in m['packages'] if p['name'].startswith('nib') for d in p['dependencies'] if d['name']=='rayon']
core=next(p for p in m['packages'] if p['name']=='nib-core')
bad += [d['name'] for d in core['dependencies'] if d['name'] in ('regex','glib','gio','gtk4')]
if bad: sys.exit('Forbidden dependencies: '+', '.join(bad))
print('Dependency boundaries: clean')
PY
