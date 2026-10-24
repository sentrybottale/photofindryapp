#!/bin/bash
# PhotoFindry authenticated Linux CLI setup protocol 1.
# Independently trust this script's immutable URL/hash and the verifier pin below.
# Release changes are authenticated by the retained publisher key, not new shell.
set -euo pipefail
umask 077
[ "$(uname -s):$(uname -m)" = Linux:x86_64 ] || { echo 'A 64-bit Intel/AMD Linux computer is required.' >&2; exit 1; }
[ "$(id -u)" != 0 ] || { echo 'Run as your normal user, without sudo.' >&2; exit 1; }
for tool in curl tar sha256sum base64 systemctl; do
  command -v "$tool" > /dev/null || { echo "This setup needs $tool. Install it with your distribution's package manager, then retry." >&2; exit 1; }
done
systemctl --user show-environment > /dev/null 2>&1 || { echo 'The systemd user manager is not available for this session. Log in to a desktop or SSH session with systemd, then retry.' >&2; exit 1; }
stage="$(mktemp -d "${TMPDIR:-/tmp}/photofindry-cli.XXXXXXXX")"
trap 'result=$?; rm -rf -- "$stage"; if [ "$result" != 0 ]; then echo "Installation did not finish. Your library is kept in place. Retry this complete command." >&2; fi' EXIT
cd "$stage"
verifier_sha256='b514cf84245591233d95e8abdbfe8ac466fc7a53e2670d12dc08d4f90f54ceb4'
verifier_bytes='541800'
# Testing only: PHOTOFINDRY_SETUP_FILES names a local folder holding the same
# files this script would download. Every pin, size, signature and checksum
# below still applies; nothing about trust changes.
fetch() {
  local url="$1" output="$2" limit="$3" time="$4"
  if [ -n "${PHOTOFINDRY_SETUP_FILES:-}" ]; then
    cp -- "$PHOTOFINDRY_SETUP_FILES/${url##*/}" "$output"
  else
    curl --disable --fail --location --progress-bar --show-error --proto '=https' --proto-redir '=https' --connect-timeout 20 --max-time "$time" --max-filesize "$limit" --output "$output" "$url"
  fi
}
echo 'Verifying the PhotoFindry publisher and checking the latest signed Linux release…'
fetch "https://photofindry.com/installers/sha256/$verifier_sha256/CompanionReleaseVerify-linux-x86_64" CompanionReleaseVerify 4194304 60
[ "$(wc -c < CompanionReleaseVerify | tr -d ' ')" = "$verifier_bytes" ] || { echo 'The trusted setup verifier download is incomplete. Nothing was installed.' >&2; exit 1; }
printf '%s  CompanionReleaseVerify\n' "$verifier_sha256" | sha256sum --quiet -c -
chmod 500 CompanionReleaseVerify
fetch 'https://photofindry.com/updates/cli/linux-x86_64/release.json' cli-release.json 131072 60
./CompanionReleaseVerify < cli-release.json > /dev/null || { echo 'The Linux release did not authenticate with the trusted PhotoFindry publisher. Nothing was installed. Get the complete setup command from https://photofindry.com/docs/install.html if this keeps happening.' >&2; exit 1; }
# Parse only authenticated metadata. No source code, trust roots or commands come
# from the feed. The exact repository, file naming and data contract stay fixed.
payload="$(sed -n 's/.*"payload": *"\([A-Za-z0-9+/=]*\)".*/\1/p' cli-release.json | base64 -d)"
field() { printf '%s' "$payload" | grep -o "\"$1\":[^,}]*" | head -n 1 | cut -d: -f2- | tr -d '"'; }
version="$(field version)"
build="$(field build)"
archive_sha256="$(field archiveSHA256)"
archive_bytes="$(field archiveBytes)"
[[ "$version" =~ ^[0-9]{1,8}\.[0-9]{1,8}\.[0-9]{1,8}-beta\.[0-9]{1,8}$ && "$build" =~ ^[0-9]{1,10}$ && "$archive_sha256" =~ ^[a-f0-9]{64}$ && "$archive_bytes" =~ ^[0-9]{1,10}$ ]] || { echo 'The authenticated release metadata has an unsupported format. Nothing was installed.' >&2; exit 1; }
[ "$build" -gt 130 ] && [ "$build" -le 1000000000 ] && [ "$archive_bytes" -gt 0 ] && [ "$archive_bytes" -le 2147483648 ] && [ "$(field schema)" = 1 ] && [ "$(field kind)" = photofindry-cli-release ] && [ "$(field platform)" = linux-x86_64 ] && [ "$(field dataContract)" = photofindry-library-compatible-v1 ] || { echo 'This signed release is not compatible with this setup. Nothing was installed.' >&2; exit 1; }
archive="PhotoFindry-$version-linux-x86_64-companion.tar.gz"
archive_url="https://github.com/sentrybottale/photofindryapp/releases/download/v$version/$archive"
[ "$(field archiveURL)" = "$archive_url" ] || { echo 'The signed Linux archive location is not an official PhotoFindry release. Nothing was installed.' >&2; exit 1; }
echo "Downloading PhotoFindry $version (build $build)…"
fetch "$archive_url" "$archive" "$archive_bytes" 900
[ "$(wc -c < "$archive" | tr -d ' ')" = "$archive_bytes" ] || { echo 'The signed Linux archive download is incomplete. Nothing was installed.' >&2; exit 1; }
printf '%s  %s\n' "$archive_sha256" "$archive" | sha256sum --quiet -c -
mkdir package
tar -xzf "$archive" -C package --no-same-owner
echo 'Installing the signed companion and its login startup…'
package/bin/photofindry lifecycle install --release "$stage/cli-release.json" > /dev/null
installed="${XDG_DATA_HOME:-$HOME/.local/share}/PhotoFindry Companion/versions/$build/package/bin/photofindry"
"$installed" open --launch-state "${XDG_STATE_HOME:-$HOME/.local/state}/photofindry/launch" > /dev/null || echo 'The companion is running. Open it later with: photofindry open, or app.photofindry.com.'
echo "Ready. PhotoFindry $version starts at login. Status: \"$installed\" lifecycle status"
