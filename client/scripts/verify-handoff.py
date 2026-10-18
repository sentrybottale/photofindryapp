#!/usr/bin/env python3
"""Verify an offline PhotoFindry rehearsal handoff using an independent trust file.

No downloading, installation, extraction or execution. This does not accept the
trust key from the handoff directory. A rehearsal signature is not a release.
"""
import argparse
import hashlib
import json
from pathlib import Path, PurePosixPath
import re
import stat
import subprocess
import tarfile

NAMESPACE = 'photofindry.release-rehearsal.v1'
IDENTITY = 'photofindry-rehearsal'
ASSETS = {'photofindry-client-source.tar.gz', 'qualification.json', 'evidence.json', 'client-source.json', 'PhotoFindry-rehearsal.zip'}
GATES = {'client-source', 'client-build', 'client-security', 'private-core-review',
         'package-audit', 'signed-delivery', 'migration-recovery', 'real-model',
         'update-rollback', 'public-processes', 'production-identity', 'live-publication'}

def fail(message): raise ValueError(message)

def regular(path, maximum):
    info = path.lstat()
    if not stat.S_ISREG(info.st_mode) or info.st_nlink != 1 or info.st_size > maximum:
        fail('Expected bounded unlinked regular file: ' + path.name)
    return info

def sha(path):
    h = hashlib.sha256()
    with path.open('rb') as stream:
        for chunk in iter(lambda: stream.read(1024*1024), b''): h.update(chunk)
    return h.hexdigest()

def parse_json(path):
    regular(path, 4*1024*1024)
    def unique(pairs):
        value = {}
        for key, item in pairs:
            if key in value: fail('Duplicate JSON key')
            value[key] = item
        return value
    return json.loads(path.read_bytes(), object_pairs_hook=unique)

def verify(root, trust):
    if root.is_symlink() or not root.is_dir(): fail('Expected handoff directory')
    if trust.resolve().is_relative_to(root.resolve()): fail('Trust must be obtained independently of the handoff')
    regular(trust, 16384)
    manifest_path = root / 'SHA256SUMS'
    regular(manifest_path, 16384); regular(root / 'SHA256SUMS.sig', 16384)
    body = manifest_path.read_bytes()
    subprocess.run(['ssh-keygen', '-Y', 'verify', '-f', str(trust), '-I', IDENTITY,
                    '-n', NAMESPACE, '-s', str(root / 'SHA256SUMS.sig')], input=body,
                   stdout=subprocess.PIPE, stderr=subprocess.PIPE, check=True)
    rows = {}; previous = ''
    for line in body.decode('ascii').splitlines():
        match = re.fullmatch(r'([0-9a-f]{64})  ([A-Za-z0-9.-]+)', line)
        if not match: fail('Invalid signed inventory')
        checksum, name = match.groups()
        if name not in ASSETS or name in rows or name <= previous: fail('Unexpected, duplicate or unsorted asset')
        regular(root / name, 2*1024**3)
        if sha(root / name) != checksum: fail('Asset digest mismatch: ' + name)
        rows[name] = checksum; previous = name
    if not body.endswith(b'\n') or not (ASSETS - {'PhotoFindry-rehearsal.zip'}) <= rows.keys(): fail('Incomplete signed inventory')
    if {p.name for p in root.iterdir()} != set(rows) | {'SHA256SUMS', 'SHA256SUMS.sig'}: fail('Unexpected handoff content')
    source = parse_json(root / 'client-source.json')
    evidence = parse_json(root / 'qualification.json')
    if source.get('schema') != 1 or source.get('kind') != 'photofindry-client-source': fail('Invalid source inventory')
    if evidence.get('schema') != 1 or evidence.get('gateSet') != 'photofindry-browser-release.v1' or evidence.get('publication') != 'HELD': fail('Invalid rehearsal record')
    revision = evidence.get('sourceRevision')
    if not isinstance(revision, str) or not re.fullmatch('[0-9a-f]{40}', revision) or source.get('privateBuildRevision') != revision: fail('Candidate source mismatch')
    if evidence.get('sourceArchiveSHA256') != rows['photofindry-client-source.tar.gz'] or evidence.get('sourceInventorySHA256') != rows['client-source.json']: fail('Evidence/source binding mismatch')
    if evidence.get('appSHA256') != rows.get('PhotoFindry-rehearsal.zip'): fail('Evidence/app binding mismatch')
    records = parse_json(root / 'evidence.json')
    if not isinstance(records, dict): fail('Invalid gate evidence records')
    gates = evidence.get('gates')
    if not isinstance(gates, dict) or set(gates) != GATES: fail('Incomplete gate set')
    for name, gate in gates.items():
        if not isinstance(gate, dict) or set(gate) != {'status','scope','evidenceSHA256'}: fail('Invalid gate shape')
        if gate['status'] not in {'PASS','FAIL','NOT-PERFORMED'} or not isinstance(gate['scope'], str) or not 1 <= len(gate['scope']) <= 2000: fail('Invalid gate status/scope')
        checksum = gate['evidenceSHA256']
        if gate['status'] == 'NOT-PERFORMED':
            if checksum is not None: fail('Absent evidence cannot have a digest')
        else:
            record = records.get(name)
            if (not isinstance(checksum, str) or not re.fullmatch('[0-9a-f]{64}', checksum)
                    or not isinstance(record, dict)
                    or set(record) != {'schema', 'gate', 'sourceRevision', 'status', 'checks', 'limitations'}
                    or record['schema'] != 1 or record['gate'] != name or record['sourceRevision'] != revision
                    or record['status'] != gate['status'] or not isinstance(record['checks'], list)
                    or not record['checks'] or len(record['checks']) > 100
                    or not all(isinstance(c,str) and 1 <= len(c) <= 2000 for c in record['checks'])
                    or not isinstance(record['limitations'], str)
                    or hashlib.sha256(json.dumps(record,sort_keys=True,separators=(',',':')).encode()).hexdigest() != checksum):
                fail('Completed gate lacks matching canonical evidence: ' + name)
    if set(records) != {name for name, gate in gates.items() if gate['status'] != 'NOT-PERFORMED'}:
        fail('Unexpected or missing gate evidence')
    # Check all archive entries against signed inventory without extracting anything.
    expected = {}
    for row in source.get('files', []):
        name = row.get('path', '')
        if (not name or PurePosixPath(name).is_absolute() or '..' in name.split('/')
                or '\\' in name or name in expected or '.git' in name.split('/')): fail('Unsafe source inventory')
        expected[name] = row
    if not 1 <= len(expected) <= 512: fail('Invalid source count')
    seen = set(); total = 0
    with tarfile.open(root / 'photofindry-client-source.tar.gz', 'r:gz') as archive:
        for entry in archive:
            total += entry.size
            prefix = 'photofindry-client/'
            if not entry.isfile() or entry.pax_headers or not entry.name.startswith(prefix) or entry.size > 4*1024**2 or total > 64*1024**2: fail('Unsafe source archive entry')
            name = entry.name[len(prefix):]
            if name in seen: fail('Duplicate source archive entry')
            seen.add(name)
            data = archive.extractfile(entry).read(4*1024**2+1)
            if name == 'CLIENT-SOURCE.json':
                if data != (root / 'client-source.json').read_bytes(): fail('Embedded source inventory differs')
            elif name not in expected or len(data) != expected[name]['bytes'] or hashlib.sha256(data).hexdigest() != expected[name]['sha256']:
                fail('Source archive/inventory mismatch')
    if seen != set(expected) | {'CLIENT-SOURCE.json'}: fail('Source archive is incomplete')
    return {'authenticatedRehearsal': True, 'publication': 'HELD', 'productionReleaseQualified': False,
            'sourceRevision': revision, 'gates': {name: gate['status'] for name, gate in gates.items()}}

if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--handoff', type=Path, required=True)
    parser.add_argument('--allowed-signers', type=Path, required=True)
    args = parser.parse_args()
    try: print(json.dumps(verify(args.handoff, args.allowed_signers), indent=2))
    except (ValueError, OSError, subprocess.CalledProcessError, tarfile.TarError) as error: raise SystemExit(str(error))
