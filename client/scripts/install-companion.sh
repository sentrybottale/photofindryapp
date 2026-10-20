#!/bin/bash
# PhotoFindry authenticated CLI setup protocol 1.
# Independently trust this script's immutable URL/hash and the verifier pin below.
# Release changes are authenticated by the retained publisher key, not new shell.
set -euo pipefail
umask 077
[ "$(uname -s):$(uname -m)" = Darwin:arm64 ] || { echo 'An Apple-silicon Mac is required.' >&2; exit 1; }
[ "$(id -u)" != 0 ] || { echo 'Run as your Mac user, without sudo.' >&2; exit 1; }
[ "$(sw_vers -productVersion | cut -d. -f1)" -ge 14 ] || { echo 'macOS 14 or later is required.' >&2; exit 1; }
stage="$(mktemp -d /private/tmp/photofindry-cli.XXXXXXXX)"
trap 'result=$?; rm -rf -- "$stage"; if [ "$result" != 0 ]; then echo "Installation did not finish. Keep your library in place. Retry this complete command; if an older PhotoFindry app owns the library, quit it first." >&2; fi' EXIT
cd "$stage"
verifier_sha256='f2543f2ab26d332fbf749835d90ea84e0513b5b8aa17f6b3f4e677f73e6cf768'
verifier_bytes='58032'
echo 'Verifying the PhotoFindry publisher and checking the latest signed CLI release…'
curl --disable --fail --location --progress-bar --show-error --proto '=https' --proto-redir '=https' --connect-timeout 20 --max-time 60 --max-filesize 2097152 --output CompanionReleaseVerify "https://photofindry.com/installers/sha256/$verifier_sha256/CompanionReleaseVerify"
[ "$(/usr/bin/wc -c < CompanionReleaseVerify | /usr/bin/tr -d ' ')" = "$verifier_bytes" ] || { echo 'The trusted setup verifier download is incomplete. Nothing was installed.' >&2; exit 1; }
printf '%s  CompanionReleaseVerify\n' "$verifier_sha256" | /usr/bin/shasum -a 256 -c -
/bin/chmod 500 CompanionReleaseVerify
curl --disable --fail --location --progress-bar --show-error --proto '=https' --proto-redir '=https' --connect-timeout 20 --max-time 60 --max-filesize 131072 --output cli-release.json 'https://photofindry.com/updates/cli/release.json'
./CompanionReleaseVerify < cli-release.json > /dev/null || { echo 'The CLI release did not authenticate with the trusted PhotoFindry publisher. Nothing was installed. Get the complete setup command from https://photofindry.com/docs/install.html if this keeps happening.' >&2; exit 1; }
# Parse only authenticated metadata. No source code, trust roots or commands come
# from the feed. The exact repository, file naming and data contract stay fixed.
/usr/bin/plutil -extract payload raw -o - cli-release.json | /usr/bin/base64 -D > release-body.json
field() { /usr/bin/plutil -extract "$1" raw -o - release-body.json; }
version="$(field version)"
build="$(field build)"
archive_sha256="$(field archiveSHA256)"
archive_bytes="$(field archiveBytes)"
[[ "$version" =~ ^[0-9]{1,8}\.[0-9]{1,8}\.[0-9]{1,8}-beta\.[0-9]{1,8}$ && "$build" =~ ^[0-9]{1,10}$ && "$archive_sha256" =~ ^[a-f0-9]{64}$ && "$archive_bytes" =~ ^[0-9]{1,10}$ ]] || { echo 'The authenticated release metadata has an unsupported format. Nothing was installed.' >&2; exit 1; }
[ "$build" -ge 119 ] && [ "$build" -le 1000000000 ] && [ "$archive_bytes" -gt 0 ] && [ "$archive_bytes" -le 2147483648 ] && [ "$(field schema)" = 1 ] && [ "$(field kind)" = photofindry-cli-release ] && [ "$(field platform)" = macos-arm64 ] && [ "$(field dataContract)" = photofindry-library-compatible-v1 ] || { echo 'This signed release is not compatible with this setup. Nothing was installed.' >&2; exit 1; }
archive="PhotoFindry-$version-arm64-companion.zip"
archive_url="https://github.com/sentrybottale/photofindryapp/releases/download/v$version/$archive"
[ "$(field archiveURL)" = "$archive_url" ] || { echo 'The signed CLI archive location is not an official PhotoFindry release. Nothing was installed.' >&2; exit 1; }
echo "Downloading PhotoFindry $version (build $build)…"
curl --disable --fail --location --progress-bar --show-error --proto '=https' --proto-redir '=https' --connect-timeout 20 --max-time 900 --max-filesize "$archive_bytes" --output "$archive" "$archive_url"
[ "$(/usr/bin/wc -c < "$archive" | /usr/bin/tr -d ' ')" = "$archive_bytes" ] || { echo 'The signed CLI archive download is incomplete. Nothing was installed.' >&2; exit 1; }
printf '%s  %s\n' "$archive_sha256" "$archive" | /usr/bin/shasum -a 256 -c -
ditto -x -k "$archive" package
cli="$stage/package/bin/photofindry"
echo 'Installing the signed CLI, login startup and browser connection…'
"$cli" lifecycle install --release "$stage/cli-release.json" --current-library --data-dir "$HOME/Library/Application Support/PhotoRecall" --rollback-copy "$HOME/Library/Application Support/PhotoFindry Recovery/CLI-$(uuidgen)"
"$cli" open-companion
echo 'Ready. Close Terminal; the companion keeps running. Next time, use app.photofindry.com.'
