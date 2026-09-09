# Build, stage, and push a release to itch.io.
#   .\tools\release.ps1                    # current Cargo.toml version -> windows channel
#   .\tools\release.ps1 -Channel osx       # same build, another channel
#   .\tools\release.ps1 -Version 0.2.0     # override the version stamp
#
# The game loads `assets/` relative to its working directory, so the staged folder
# is just the exe beside the assets tree; butler handles compression and diffs.
param([string]$Channel = "windows", [string]$Version = "")

$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
$manifest = Join-Path $root "Cargo.toml"
$exe = Join-Path $root "target\release\the-zone.exe"

if (-not $Version) {
    $cargo = Get-Content $manifest -Raw
    $m = [regex]::Match($cargo, '(?m)^version\s*=\s*"([^"]+)"')
    if ($m.Success) { $Version = $m.Groups[1].Value }
    else { $Version = (git -C $root describe --tags --always 2>$null).Trim() }
}
if (-not $Version) { throw "no version in Cargo.toml and no git tag" }

cargo build --release --manifest-path $manifest

$stage = Join-Path $root "target\dist\the-zone-$Version-$Channel"
Remove-Item $stage -Recurse -Force -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Path $stage | Out-Null
Copy-Item $exe $stage
Copy-Item (Join-Path $root "assets") $stage -Recurse

butler push $stage "tigwyk/the-zone:$Channel" --userversion $Version
