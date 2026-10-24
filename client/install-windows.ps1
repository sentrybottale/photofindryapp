# PhotoFindry authenticated Windows CLI setup protocol 1.
# Independently trust this script's immutable URL/hash and the verifier pins below.
# Release changes are authenticated by the retained publisher key, not new script.
$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
function Fail([string]$message) { Write-Host $message -ForegroundColor Red; exit 1 }
if ($env:PROCESSOR_ARCHITECTURE -ne 'AMD64') { Fail 'A 64-bit Intel/AMD Windows PC is required.' }
$principal = [Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()
if ($principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) { Fail 'Run this in a normal PowerShell window, not as administrator.' }
$system = Join-Path $env:SystemRoot 'System32'
$curl = Join-Path $system 'curl.exe'
$tar = Join-Path $system 'tar.exe'
$verifierSha256 = 'f2652d226f699fc60f783575b9f499cd7f42b55cf2276dc4a6317531ad41d959'
$verifierBytes = 429056
$unwindSha256 = 'f170c420d203cfb6640b3f90ca46373fd886670adad08ce5f2fdde054db27976'
$unwindBytes = 183808
$stage = Join-Path $env:TEMP ('photofindry-cli-' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $stage | Out-Null
# Testing only: PHOTOFINDRY_SETUP_FILES names a local folder holding the same
# files this script would download. Every pin, size, signature and checksum
# below still applies; nothing about trust changes.
function Fetch([string]$url, [string]$output, [long]$limit, [int]$seconds) {
    if ($env:PHOTOFINDRY_SETUP_FILES) {
        Copy-Item -LiteralPath (Join-Path $env:PHOTOFINDRY_SETUP_FILES ($url -split '/')[-1]) -Destination $output
        return
    }
    & $curl --disable --fail --location --show-error --silent --proto '=https' --proto-redir '=https' --connect-timeout 20 --max-time $seconds --max-filesize $limit --output $output $url
    if ($LASTEXITCODE -ne 0) { Fail "Could not download $url. Check your connection and retry. Nothing was installed." }
}
function Pinned([string]$path, [string]$sha, [long]$bytes) {
    if ((Get-Item -LiteralPath $path).Length -ne $bytes -or (Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash.ToLowerInvariant() -ne $sha) {
        Fail 'The trusted setup verifier download does not match its published checksum. Nothing was installed.'
    }
}
try {
    Write-Host 'Verifying the PhotoFindry publisher and checking the latest signed Windows release...'
    $verifier = Join-Path $stage 'CompanionReleaseVerify.exe'
    Fetch "https://photofindry.com/installers/sha256/$verifierSha256/CompanionReleaseVerify-windows-x86_64.exe" $verifier 4194304 60
    Pinned $verifier $verifierSha256 $verifierBytes
    Fetch "https://photofindry.com/installers/sha256/$unwindSha256/libunwind.dll" (Join-Path $stage 'libunwind.dll') 4194304 60
    Pinned (Join-Path $stage 'libunwind.dll') $unwindSha256 $unwindBytes
    $feed = Join-Path $stage 'cli-release.json'
    Fetch 'https://photofindry.com/updates/cli/windows-x86_64/release.json' $feed 131072 60
    $check = Start-Process -FilePath $verifier -RedirectStandardInput $feed -RedirectStandardOutput (Join-Path $stage 'verified.json') -NoNewWindow -Wait -PassThru
    if ($check.ExitCode -ne 0) { Fail 'The Windows release did not authenticate with the trusted PhotoFindry publisher. Nothing was installed. Get the complete setup command from https://photofindry.com/docs/install.html if this keeps happening.' }
    # Parse only authenticated metadata. No code, trust roots or commands come
    # from the feed. The exact repository, file naming and data contract stay fixed.
    $envelope = Get-Content -LiteralPath $feed -Raw | ConvertFrom-Json
    $body = [Text.Encoding]::UTF8.GetString([Convert]::FromBase64String($envelope.payload)) | ConvertFrom-Json
    $version = [string]$body.version
    $build = [long]$body.build
    if ($version -notmatch '^[0-9]{1,8}\.[0-9]{1,8}\.[0-9]{1,8}-beta\.[0-9]{1,8}$' -or $build -le 130 -or $build -gt 1000000000 -or
        [string]$body.archiveSHA256 -notmatch '^[a-f0-9]{64}$' -or [long]$body.archiveBytes -le 0 -or [long]$body.archiveBytes -gt 2147483648 -or
        $body.schema -ne 1 -or $body.kind -ne 'photofindry-cli-release' -or $body.platform -ne 'windows-x86_64' -or $body.dataContract -ne 'photofindry-library-compatible-v1') {
        Fail 'This signed release is not compatible with this setup. Nothing was installed.'
    }
    $archive = "PhotoFindry-$version-windows-x86_64-companion.zip"
    $archiveUrl = "https://github.com/sentrybottale/photofindryapp/releases/download/v$version/$archive"
    if ($body.archiveURL -ne $archiveUrl) { Fail 'The signed Windows archive location is not an official PhotoFindry release. Nothing was installed.' }
    Write-Host "Downloading PhotoFindry $version (build $build)..."
    $archivePath = Join-Path $stage $archive
    Fetch $archiveUrl $archivePath ([long]$body.archiveBytes) 900
    if ((Get-Item -LiteralPath $archivePath).Length -ne [long]$body.archiveBytes -or (Get-FileHash -LiteralPath $archivePath -Algorithm SHA256).Hash.ToLowerInvariant() -ne $body.archiveSHA256) {
        Fail 'The signed Windows archive download does not match its signed checksum. Nothing was installed.'
    }
    $package = Join-Path $stage 'package'
    New-Item -ItemType Directory -Path $package | Out-Null
    & $tar -xf $archivePath -C $package
    if ($LASTEXITCODE -ne 0) { Fail 'The signed Windows archive could not be extracted. Nothing was installed.' }
    Write-Host 'Installing the signed companion and its logon startup...'
    & (Join-Path $package 'bin\photofindry.exe') lifecycle install --release $feed | Out-Null
    if ($LASTEXITCODE -ne 0) { Fail 'Installation did not finish. Your library is kept in place. Retry this complete command.' }
    $installed = Join-Path $env:LOCALAPPDATA "PhotoFindry Companion\versions\$build\package\bin\photofindry.exe"
    & $installed open --launch-state (Join-Path $env:LOCALAPPDATA 'PhotoRecall\Launch') | Out-Null
    Write-Host "Ready. PhotoFindry $version starts when you sign in. Status: `"$installed`" lifecycle status"
} finally {
    Remove-Item -LiteralPath $stage -Recurse -Force -ErrorAction SilentlyContinue
}
