# Builds the release zip: twinfm.exe with every license notice it needs.
#   powershell -File tools\package-release.ps1
# Writes dist\twinfm-<version>-x86_64-windows.zip (the version from Cargo.toml). Needs cargo-about
# (`cargo install cargo-about --locked --features cli`).
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root

$version = (Select-String -Path Cargo.toml -Pattern '^version = "(.+)"' | Select-Object -First 1).Matches[0].Groups[1].Value
$name = "twinfm-$version-x86_64-windows"
$stage = Join-Path $root "dist\$name"

cargo build --release
if ($LASTEXITCODE -ne 0) { throw "cargo build failed" }

if (Test-Path $stage) { Remove-Item -Recurse -Force $stage }
New-Item -ItemType Directory -Force "$stage\licenses" | Out-Null

cargo about generate about.hbs -o "$stage\THIRD-PARTY-LICENSES.html"
if ($LASTEXITCODE -ne 0) { throw "cargo about failed" }

Copy-Item target\release\twinfm.exe, LICENSE, README.md $stage
Copy-Item licenses\* "$stage\licenses"
Copy-Item src\ui\fonts\OFL.txt "$stage\licenses\Inter-OFL.txt"
Copy-Item src\ui\icons\LICENSE "$stage\licenses\Lucide-icons.txt"

$zip = Join-Path $root "dist\$name.zip"
if (Test-Path $zip) { Remove-Item -Force $zip }
Compress-Archive -Path $stage -DestinationPath $zip
Get-FileHash -Algorithm SHA256 $zip | ForEach-Object { "$($_.Hash.ToLower())  $name.zip" } | Set-Content -Encoding ascii "$zip.sha256"
Write-Host "Wrote $zip"
