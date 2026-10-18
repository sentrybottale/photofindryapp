#!/usr/bin/env python3
"""Read-only, standard-library gate for the retained local frontend assets.

This validates checked-in bytes without npm, node_modules, a compiler, network
access or writes. It does not establish trust in a malicious source checkout.
"""

import argparse
import base64
import gzip
import hashlib
import json
import os
from pathlib import Path
import random
import re
import shutil
import stat
import sys
import tempfile
from urllib.parse import urlsplit


SOURCES = ("package.json", "package-lock.json", "scripts/build.mjs", "src/AppShell.svelte", "src/Appearance.svelte", "src/Icon.svelte", "src/Navigation.svelte", "src/PhotoSummary.svelte", "src/main.js")
ASSETS = {"ui-framework.js": 96 * 1024, "ui-framework.css": 8 * 1024, "ui-framework.NOTICES.txt": 64 * 1024}
NOTICE_HEADER = "PhotoFindry browser shell: bundled third-party runtime notices\n\nFirst-party browser client code is MIT licensed; proprietary engines have separate terms.\n\n"
# SHA-256 of the complete trimmed licence text supplied by these pinned packages.
# A runtime upgrade requires reviewing its licence and updating this allowlist.
LICENSES = {
    ("clsx", "2.1.1"): "3d5b63706380c8c746b862cba2ff5eb9fefd7fc81fbfac2964006c3f319573d2",
    ("esm-env", "1.2.2"): "c6cc76cbbf3a30949d558e0d00b3cbcf3dff3a3d7ab9a99424748ab8e8aba2dc",
    ("svelte", "5.57.2"): "673a1ac4a9d80146239c60fe59345386a88a3afff864868a2bf65fe569a76728",
}
VERSION = re.compile(r"[0-9]+\.[0-9]+\.[0-9]+(?:-[0-9A-Za-z.-]+)?\Z")
SHA256 = re.compile(r"[0-9a-f]{64}\Z")
FORBIDDEN_JS = re.compile(rb"\b(?:eval|import)\s*\(|new\s+Function\s*\(|sourceMappingURL|localhost:5173|__SVELTE_HMR|fetch\s*\(")


class InvalidFrontend(Exception):
    pass


def require(condition, message):
    if not condition:
        raise InvalidFrontend(message)


def read_file(filename, limit):
    try:
        with open(filename, "rb", opener=lambda name, flags: os.open(name, flags | getattr(os, "O_NOFOLLOW", 0))) as stream:
            require(stat.S_ISREG(os.fstat(stream.fileno()).st_mode), f"Expected a regular file: {filename.name}")
            value = stream.read(limit + 1)
    except OSError as error:
        raise InvalidFrontend(f"Cannot read a regular frontend file: {filename.name}") from error
    require(len(value) <= limit, f"Frontend file exceeds its reviewed byte limit: {filename.name}")
    return value


def no_duplicate_keys(pairs):
    result = {}
    for key, value in pairs:
        require(key not in result, f"Duplicate JSON field: {key}")
        result[key] = value
    return result


def parse_json(value, label):
    try:
        result = json.loads(value, object_pairs_hook=no_duplicate_keys)
    except (ValueError, UnicodeError) as error:
        raise InvalidFrontend(f"Unreadable JSON: {label}") from error
    require(isinstance(result, dict), f"Expected a JSON object: {label}")
    return result


def digest(value):
    return hashlib.sha256(value).hexdigest()


def source_hash(frontend):
    result = hashlib.sha256()
    for name in SOURCES:
        result.update((name + "\0").encode("utf8"))
        result.update(read_file(frontend / name, 2 * 1024 * 1024 if name == "package-lock.json" else 128 * 1024))
    return result.hexdigest()


def verify(root):
    web = root / "web-companion"
    frontend = web / "frontend"
    # The responsive fixture server is test tooling, never imported by build.mjs
    # or emitted into the product. Its mutable reports live outside this tree.
    for folder, expected, allowed in (
        ("src", {"src/AppShell.svelte", "src/Appearance.svelte", "src/Icon.svelte", "src/Navigation.svelte", "src/PhotoSummary.svelte", "src/main.js"}, {"src/AppShell.svelte", "src/Appearance.svelte", "src/Icon.svelte", "src/Navigation.svelte", "src/PhotoSummary.svelte", "src/main.js"}),
        ("scripts", {"scripts/build.mjs"}, {"scripts/build.mjs", "scripts/serve-responsive-fixture.mjs"}),
    ):
        actual = set()
        for count, filename in enumerate((frontend / folder).rglob("*"), 1):
            require(count <= 128 and not filename.is_symlink(), "Unexpected or linked frontend source input.")
            if filename.is_file():
                actual.add(filename.relative_to(frontend).as_posix())
        require(expected <= actual <= allowed, f"Update the reviewed build-source or test-tool allowlist before adding files to frontend/{folder}.")
    manifest = parse_json(read_file(web / "ui-framework.manifest.json", 32 * 1024), "frontend manifest")
    require(set(manifest) == {"schemaVersion", "framework", "sourceSha256", "runtimeDependencies", "assets"}
            and type(manifest["schemaVersion"]) is int and manifest["schemaVersion"] == 1, "Unsupported frontend manifest schema.")
    require(isinstance(manifest["sourceSha256"], str) and SHA256.fullmatch(manifest["sourceSha256"]), "Invalid frontend source fingerprint.")
    require(manifest["sourceSha256"] == source_hash(frontend), "Generated frontend assets are stale for their current source or dependency lock.")
    package = parse_json(read_file(frontend / "package.json", 32 * 1024), "frontend package")
    lock = parse_json(read_file(frontend / "package-lock.json", 2 * 1024 * 1024), "frontend dependency lock")
    dependencies = package.get("devDependencies")
    require(package.get("private") is True and isinstance(dependencies, dict)
            and set(dependencies) == {"esbuild", "happy-dom", "svelte"}
            and all(isinstance(value, str) and VERSION.fullmatch(value) for value in dependencies.values()), "Frontend build dependencies must remain private, reviewed and exactly pinned.")
    packages = lock.get("packages")
    root_package = packages.get("") if isinstance(packages, dict) else None
    require(lock.get("lockfileVersion") == 3 and isinstance(packages, dict) and isinstance(root_package, dict)
            and root_package.get("devDependencies") == dependencies, "The frontend dependency lock does not match its exact build pins.")
    for name, metadata in packages.items():
        if name == "":
            continue
        require(isinstance(name, str) and name.startswith("node_modules/") and ".." not in name.split("/")
                and isinstance(metadata, dict) and isinstance(metadata.get("version"), str) and VERSION.fullmatch(metadata["version"]), "Unreviewed dependency source in the frontend lock.")
        resolved = urlsplit(metadata.get("resolved", ""))
        require(resolved.scheme == "https" and resolved.netloc == "registry.npmjs.org" and not resolved.query and not resolved.fragment, "Frontend dependencies must use the pinned registry packages.")
        integrity = metadata.get("integrity", "")
        require(isinstance(integrity, str) and integrity.startswith("sha512-"), "A frontend package is missing its SHA-512 integrity record.")
        try:
            require(len(base64.b64decode(integrity[7:], validate=True)) == 64, "Invalid frontend package integrity record.")
        except ValueError as error:
            raise InvalidFrontend("Invalid frontend package integrity record.") from error
    for name, version in dependencies.items():
        require(packages.get("node_modules/" + name, {}).get("version") == version, f"Frontend package pin differs from lock: {name}")
    require(manifest["framework"] == {"name": "svelte", "version": dependencies["svelte"]}, "The generated framework does not match the pinned Svelte compiler.")
    runtime = manifest["runtimeDependencies"]
    require(isinstance(runtime, list) and len(runtime) == 3, "Unreviewed frontend runtime dependency set.")
    names = []
    for dependency in runtime:
        require(isinstance(dependency, dict) and set(dependency) == {"name", "version", "license"}
                and isinstance(dependency["name"], str) and isinstance(dependency["version"], str)
                and dependency["license"] == "MIT" and (dependency["name"], dependency["version"]) in LICENSES, "Review new runtime versions and their complete notices before packaging.")
        name = dependency["name"]
        require(packages.get("node_modules/" + name, {}).get("version") == dependency["version"], "A bundled frontend runtime does not match the dependency lock.")
        names.append(name)
    require(names == ["clsx", "esm-env", "svelte"], "Frontend runtime dependencies must be unique and in canonical order.")
    records = manifest["assets"]
    require(isinstance(records, dict) and set(records) == set(ASSETS), "Unexpected generated frontend asset allowlist.")
    assets = {}
    for name, limit in ASSETS.items():
        record = records[name]
        require(isinstance(record, dict) and set(record) == {"bytes", "sha256"}
                and type(record["bytes"]) is int and 0 < record["bytes"] <= limit
                and isinstance(record["sha256"], str) and SHA256.fullmatch(record["sha256"]), f"Invalid generated asset record: {name}")
        value = read_file(web / name, limit)
        require(len(value) == record["bytes"] and digest(value) == record["sha256"], f"Generated frontend asset bytes differ from manifest: {name}")
        assets[name] = value
    script = assets["ui-framework.js"]
    compressed = len(gzip.compress(script, compresslevel=6, mtime=0))
    require(compressed <= 36 * 1024, "The compressed frontend exceeded its reviewed 36 KiB budget.")
    require(FORBIDDEN_JS.search(script) is None, "Generated frontend contains an unreviewed loader, eval, source map or development hook.")
    require(not (web / "ui-framework.js.map").exists() and not (web / "ui-framework.css.map").exists(), "Remove unshipped frontend source maps before packaging.")
    try:
        notices = assets["ui-framework.NOTICES.txt"].decode("utf8")
    except UnicodeError as error:
        raise InvalidFrontend("Unreadable frontend runtime notices.") from error
    require(notices.startswith(NOTICE_HEADER), "The frontend notices lost the client/engine attribution boundary.")
    sections = re.split(r"(?m)^=== ([^\n]+) ===\n", notices[len(NOTICE_HEADER):])
    require(sections[0] == "" and len(sections) == 7, "Missing or extra frontend runtime notices.")
    for index, dependency in enumerate(runtime):
        heading, license_text = sections[1 + index * 2:3 + index * 2]
        require(heading == f"{dependency['name']} {dependency['version']} (MIT)"
                and digest(license_text.strip().encode("utf8")) == LICENSES[(dependency["name"], dependency["version"])], "A bundled runtime's complete licence text is missing or changed.")
    return {"framework": manifest["framework"], "javascriptBytes": len(script), "gzipLevel6Bytes": compressed,
            "cssBytes": len(assets["ui-framework.css"]), "sourceSha256": manifest["sourceSha256"]}


def self_test(root):
    """Mutation regressions use only generated temporary copies, never live data."""
    verify(root)
    files = ["web-companion/frontend/" + name for name in SOURCES]
    files += ["web-companion/" + name for name in (*ASSETS, "ui-framework.manifest.json")]
    with tempfile.TemporaryDirectory(prefix="photofindry-frontend-check-") as temporary:
        fixture = Path(temporary)
        for name in files:
            destination = fixture / name
            destination.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(root / name, destination)
        baseline = {name: (fixture / name).read_bytes() for name in files}
        web = fixture / "web-companion"
        def restore():
            for name, value in baseline.items():
                filename = fixture / name
                if filename.is_symlink():
                    filename.unlink()
                filename.write_bytes(value)
        def manifest_update(mutate):
            filename = web / "ui-framework.manifest.json"
            value = json.loads(filename.read_text())
            mutate(value)
            filename.write_text(json.dumps(value))
        def update_asset(name, value):
            (web / name).write_bytes(value)
            manifest_update(lambda manifest: manifest["assets"].update({name: {"bytes": len(value), "sha256": digest(value)}}))
        def append_source():
            filename = web / "frontend/src/AppShell.svelte"
            filename.write_bytes(filename.read_bytes() + b"\n<!-- stale source -->\n")
        def dependency_mismatch():
            filename = web / "frontend/package-lock.json"
            value = json.loads(filename.read_text())
            value["packages"]["node_modules/esbuild"]["version"] = "0.0.1"
            filename.write_text(json.dumps(value))
            manifest_update(lambda manifest: manifest.update(sourceSha256=source_hash(web / "frontend")))
        cases = [
            ("stale component source", append_source),
            ("changed output bytes", lambda: (web / "ui-framework.js").write_bytes(baseline["web-companion/ui-framework.js"] + b";")),
            ("unknown asset", lambda: manifest_update(lambda value: value["assets"].update({"remote.js": {"bytes": 1, "sha256": "0" * 64}}))),
            ("oversized CSS with updated hash", lambda: update_asset("ui-framework.css", b"x" * (8 * 1024 + 1))),
            ("dynamic loader with updated hash", lambda: update_asset("ui-framework.js", b"(()=>{import('/unreviewed.js')})();")),
            ("truncated notice with updated hash", lambda: update_asset("ui-framework.NOTICES.txt", baseline["web-companion/ui-framework.NOTICES.txt"].replace(b"Permission is hereby granted", b"Permission removed", 1))),
            ("dependency pin mismatch", dependency_mismatch),
            ("duplicate manifest fields", lambda: (web / "ui-framework.manifest.json").write_bytes(b'{"schemaVersion":1,"schemaVersion":1}')),
            ("boolean byte count", lambda: manifest_update(lambda value: value["assets"]["ui-framework.js"].update(bytes=True))),
            ("linked source", lambda: ((web / "frontend/src/main.js").unlink(), (web / "frontend/src/main.js").symlink_to(web / "frontend/src/AppShell.svelte"))),
        ]
        for label, mutate in cases:
            restore()
            mutate()
            try:
                verify(fixture)
            except InvalidFrontend:
                continue
            raise InvalidFrontend(f"Verifier self-test unexpectedly accepted {label}.")
        restore()
        random_text = "".join(random.Random(7).choices("abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789", k=60000)).encode()
        update_asset("ui-framework.js", b"/*" + random_text + b"*/")
        try:
            verify(fixture)
        except InvalidFrontend:
            pass
        else:
            raise InvalidFrontend("Verifier self-test accepted an excessive compressed bundle.")
        restore()
        verify(fixture)
    return len(cases) + 3


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parent.parent)
    parser.add_argument("--json", action="store_true", help="Print compact verification measurements.")
    parser.add_argument("--self-test", action="store_true", help="Run mutation regressions in temporary frontend-only copies.")
    arguments = parser.parse_args()
    try:
        root = arguments.root.resolve(strict=True)
        if arguments.self_test:
            count = self_test(root)
            print(f"Frontend verifier: {count} mutation/baseline checks passed; no working files changed.")
        else:
            result = verify(root)
            print(json.dumps(result, sort_keys=True) if arguments.json else
                  f"Verified retained Svelte frontend: {result['javascriptBytes']} JS bytes, {result['gzipLevel6Bytes']} gzip bytes; source, lock, assets and complete notices match.")
    except (InvalidFrontend, OSError, TypeError, KeyError) as error:
        print(f"Frontend verification failed: {error}\nRebuild in web-companion/frontend with npm ci --ignore-scripts --no-audit --no-fund, npm run build, npm test and npm run check before packaging.", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
