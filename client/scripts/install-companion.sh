#!/bin/bash
# PhotoFindry CLI setup. Review this file and independently trust its pinned release.
set -euo pipefail
umask 077
[ "$(uname -s):$(uname -m)" = Darwin:arm64 ] || { echo 'An Apple-silicon Mac is required.' >&2; exit 1; }
[ "$(id -u)" != 0 ] || { echo 'Run as your Mac user, without sudo.' >&2; exit 1; }
[ "$(sw_vers -productVersion | cut -d. -f1)" -ge 14 ] || { echo 'macOS 14 or later is required.' >&2; exit 1; }
stage="$(mktemp -d /private/tmp/photofindry-cli.XXXXXXXX)"
trap 'result=$?; rm -rf -- "$stage"; if [ "$result" != 0 ]; then echo "Installation did not finish. Retry this command. If an older PhotoFindry app owns the library, quit it first. Your library is kept." >&2; fi; exit "$result"' EXIT
cd "$stage"
base='https://github.com/sentrybottale/photofindryapp/releases/download/v0.2.0-beta.17'
archive='PhotoFindry-0.2.0-beta.17-arm64-companion.zip'
echo 'Downloading the PhotoFindry CLI companion…'
for file in "$archive" cli-release.json; do
  curl --disable --fail --location --progress-bar --show-error --proto '=https' --proto-redir '=https' --connect-timeout 20 --max-time 900 --max-filesize 450000000 --output "$file" "$base/$file"
done
# Pin the reviewed first-install bytes before extracting or running downloaded code.
shasum -a 256 -c <<'HASHES'
b8edb0377e366f5668c9cecc16d87f9bfb07fb89ab6057597544c98963130771  PhotoFindry-0.2.0-beta.17-arm64-companion.zip
b5aeb3775770c63c479ff6458ac9544a7596bda211024940e1ba0ab9069dff3c  cli-release.json
HASHES
ditto -x -k "$archive" package
cli="$stage/package/bin/photofindry"
echo 'Installing the signed CLI, login startup and browser connection…'
"$cli" lifecycle install --release "$stage/cli-release.json" --current-library --data-dir "$HOME/Library/Application Support/PhotoRecall" --rollback-copy "$HOME/Library/Application Support/PhotoFindry Recovery/CLI-$(uuidgen)"
"$cli" open-companion
echo 'Ready. Close Terminal; the companion keeps running. Next time, use app.photofindry.com.'
