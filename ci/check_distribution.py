"""Public-tree publication boundary. No downloads, credentials or execution of payloads."""
import json
from pathlib import Path, PurePosixPath
import re
import subprocess

PRIVATE = ('core/', 'engine/src/', 'engine/worker/', 'engine/contracts/', 'engine/node/', '.private/', '.runtime/')
SECRET = re.compile(rb'-----BEGIN (?:OPENSSH |RSA |EC )?PRIVATE KEY-----|gh[pousr]_[A-Za-z0-9]{30,}|github_pat_[A-Za-z0-9_]{40,}')

def validate(name, data):
    scoped = name.removeprefix('client/')
    if scoped.startswith(PRIVATE) or any(part in {'.env', '.DS_Store'} for part in PurePosixPath(name).parts):
        raise ValueError('Private implementation or local state path: ' + name)
    if PurePosixPath(name).suffix.lower() in {'.gguf', '.sqlite', '.db', '.dmg', '.zip', '.dylib', '.node'}:
        raise ValueError('Binary distribution/data must not enter the source tree: ' + name)
    if len(data) > 4 * 1024 * 1024 or data[:4] in {b'\x7fELF', b'\xcf\xfa\xed\xfe', b'\xca\xfe\xba\xbe'}:
        raise ValueError('Oversized or executable payload: ' + name)
    if SECRET.search(data):
        raise ValueError('Credential-shaped content: ' + name)
    if name.startswith('.github/workflows/'):
        text = data.decode('utf8')
        if 'pull_request_target:' in text or 'secrets.' in text:
            raise ValueError('PR workflows cannot run with repository secrets or target privileges')
        for action in re.findall(r'uses:\s*([^\s#]+)', text):
            if not re.fullmatch(r'[\w.-]+/[\w./-]+@[a-f0-9]{40}', action):
                raise ValueError('Actions must be pinned to full commit hashes')


def main():
    rows = subprocess.check_output(['git', 'ls-files', '-s', '-z']).split(b'\0')
    count = 0
    for row in filter(None, rows):
        meta, raw = row.split(b'\t', 1); mode = meta.split()[0]
        name = raw.decode('utf8')
        if mode not in {b'100644', b'100755'}:
            raise ValueError('Links and submodules require separate publication review: ' + name)
        path = Path(name)
        if path.is_symlink(): raise ValueError('Linked input: ' + name)
        validate(name, path.read_bytes()); count += 1
    # The reviewed source snapshot is a complete, bounded inventory inside client/.
    if Path('client/CLIENT-SOURCE.json').exists():
        import hashlib
        inventory = json.loads(Path('client/CLIENT-SOURCE.json').read_text())
        expected = {'CLIENT-SOURCE.json'}
        for item in inventory['files']:
            name = item['path']
            if name.startswith('/') or '..' in PurePosixPath(name).parts or name in expected:
                raise ValueError('Unsafe or duplicate client inventory path')
            expected.add(name)
            data = (Path('client') / name).read_bytes()
            validate('client/' + name, data)
            if len(data) != item['bytes'] or hashlib.sha256(data).hexdigest() != item['sha256']:
                raise ValueError('Client bytes differ from the reviewed inventory: ' + name)
        tracked = set(subprocess.check_output(['git', 'ls-files', 'client/'], text=True).splitlines())
        if tracked != {'client/' + name for name in expected}:
            raise ValueError('Client file set differs from the reviewed inventory')
    probe = json.loads(Path('ci/rehearsal-probe.json').read_text())
    if probe != {'schema': 1, 'expect': 'pass'}:
        raise ValueError('Intentional rehearsal stop: this head must not merge')
    print(f'Public distribution boundary passed: {count} files. Pattern checks do not replace review.')

if __name__ == '__main__': main()
