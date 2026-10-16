#!/usr/bin/env python3
"""Generate README/man key tables from registry entries; fail on parser drift."""
import argparse
import pathlib
import re

ROOT = pathlib.Path(__file__).resolve().parent.parent
STRING = r'"(?:[^"\\]|\\.)*"'
ARRAY = rf'&\[(?:\s*{STRING}\s*,?)*\s*\]'
ENTRY = re.compile(
    rf'\bentry\(\s*({STRING})\s*,\s*({STRING})\s*,\s*([WECF])\s*,\s*({ARRAY})\s*,\s*({ARRAY})\s*,',
    re.S,
)


def strings(value):
    return re.findall(r'"([^"\\]*(?:\\.[^"\\]*)*)"', value)


def display(scope, key):
    if scope in 'WE':
        for before, after in [('Control', 'Ctrl+'), ('Primary', 'Ctrl+'), ('Alt', 'Alt+'), ('Shift', 'Shift+')]:
            key = key.replace(f'<{before}>', after)
        parts = key.split('+')
        parts[-1] = {'Return': 'Enter', 'space': 'Space', 'bracketleft': '[', 'bracketright': ']'}.get(parts[-1], parts[-1].upper() if len(parts[-1]) == 1 else parts[-1])
        key = '+'.join(parts)
    return key


def generate():
    source = (ROOT / 'crates/nib/src/registry.rs').read_text()
    source = source.split('pub static ENTRIES: &[Entry] = &[', 1)[1].split('\n];', 1)[0]
    entries = list(ENTRY.finditer(source))
    if len(entries) != len(re.findall(r'\bentry\(', source)):
        raise SystemExit('registry format changed: update the documentation parser')
    markdown = []
    scdoc = []
    for title, scopes in [('Window and editor', 'WE'), ('Files column', 'F'), ('Commands and palette', 'C')]:
        markdown += [f'### {title}', '', '| Keys | Commands | Action |', '|---|---|---|']
        scdoc += [f'## {title.upper()}', '']
        for match in entries:
            _, label, scope, keys, commands = match.groups()
            if scope not in scopes:
                continue
            keys = [display(scope, key) for key in strings(keys)]
            commands = [':' + command for command in strings(commands)]
            label = label[1:-1]
            code = lambda values: ', '.join('`' + v.replace('|', '\\|') + '`' for v in values) or '—'
            markdown.append(f'| {code(keys)} | {code(commands)} | {label} |')
            binding = ', '.join(keys + commands) or 'Palette'
            # scdoc treats punctuation at the start of a line specially.
            scdoc += ['*' + binding.replace('\\', '\\\\').replace('*', '\\*') + '*', '\t' + label, '']
        markdown.append('')
    return '\n'.join(markdown), '\n'.join(scdoc)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--check', action='store_true')
    args = parser.parse_args()
    markdown, scdoc = generate()
    readme = ROOT / 'README.md'
    old = readme.read_text()
    begin, end = '<!-- registry:start -->', '<!-- registry:end -->'
    before, body = old.split(begin, 1)
    _, after = body.split(end, 1)
    outputs = {
        readme: before + begin + '\n\n' + markdown + '\n' + end + after,
        ROOT / 'data/nib.1.scd': (ROOT / 'data/nib.1.scd.in').read_text().replace('@REGISTRY@', scdoc),
    }
    for path, expected in outputs.items():
        if args.check:
            if not path.exists() or path.read_text() != expected:
                raise SystemExit(f'{path.relative_to(ROOT)} is stale; run scripts/generate-docs.py')
        else:
            path.write_text(expected)


if __name__ == '__main__':
    main()
