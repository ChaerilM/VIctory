# Optional local build. Run in an x64 Visual Studio Developer PowerShell.
[CmdletBinding()]
param(
    [Parameter(Mandatory=$true)][string]$SdkDir,
    [Parameter(Mandatory=$true)][string]$OutputDir
)
$ErrorActionPreference = 'Stop'
$repo = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$sdk = (Resolve-Path -LiteralPath $SdkDir).Path
$out = [IO.Path]::GetFullPath($OutputDir)
foreach ($path in @($sdk, $out)) {
    if ($path.TrimEnd('\') -ieq $repo.TrimEnd('\') -or $path.StartsWith($repo.TrimEnd('\') + '\', [StringComparison]::OrdinalIgnoreCase)) {
        throw 'SDK and output must be outside the repository'
    }
}
if ($env:VSCMD_ARG_TGT_ARCH -ne 'x64') { throw 'Use an x64 Visual Studio Developer PowerShell' }
$null = Get-Command cl.exe -ErrorAction Stop
$include = Join-Path $sdk 'include'
$lib = Join-Path $sdk 'lib\Windows_x86_64\x64\nvsdk_ngx_d.lib'
$dll = Join-Path $sdk 'lib\Windows_x86_64\rel\nvngx_dlss.dll'
foreach ($file in @((Join-Path $include 'nvsdk_ngx_helpers.h'), $lib, $dll)) {
    if (-not (Test-Path -LiteralPath $file -PathType Leaf)) { throw "Missing SDK file: $file" }
}
New-Item -ItemType Directory -Force -Path $out | Out-Null
$src = Join-Path $repo 'native\ngx\bridge.cpp'
Push-Location -LiteralPath $out
try {
    & cl.exe /nologo /std:c++17 /EHsc /MD /O2 /LD "/I$include" $src /link $lib d3d12.lib dxgi.lib /OUT:victory_streamline_upscaler.dll
    if ($LASTEXITCODE -ne 0) { throw "NGX adapter build failed ($LASTEXITCODE)" }
    Copy-Item -LiteralPath $dll -Destination $out -Force
} finally {
    Pop-Location
}
Write-Host "Adapter built in $out. Set [paths].streamline_dir to this folder."
