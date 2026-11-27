# Apply authored numeric performance settings without evaluating .env text.
[CmdletBinding()]
param(
    [ValidateSet('default', 'low', 'high', '8gb', '12gb', '16gb')]
    [string]$Preset = 'default',
    [string]$VictoryExe,
    [string[]]$VictoryArgs,
    [switch]$ShowSettings
)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
if (-not $VictoryExe) {
    $VictoryExe = Join-Path $root 'target\release\victory.exe'
}

$names = @(
    'VICTORY_WINDOW_KB',
    'VICTORY_SCRATCH_SPARE_MB',
    'VICTORY_BUFFERED_TITLE_READS',
    'VICTORY_VRAM_TARGET_MB'
)

function Read-PerformanceEnv([string]$Path) {
    $settings = @{}
    $lineNumber = 0
    foreach ($line in Get-Content -LiteralPath $Path -Encoding UTF8) {
        $lineNumber++
        $line = $line.Trim()
        if (-not $line -or $line.StartsWith('#')) { continue }
        if ($line -cnotmatch '^(VICTORY_[A-Z0-9_]+)\s*=\s*([0-9]+)$') {
            throw "Invalid numeric setting at $($Path):$lineNumber"
        }
        $name = $Matches[1]
        $value = $Matches[2]
        if ($names -cnotcontains $name) {
            throw "Unknown performance setting $name at $($Path):$lineNumber"
        }
        if ($settings.ContainsKey($name)) {
            throw "Duplicate performance setting $name at $($Path):$lineNumber"
        }
        switch ($name) {
            'VICTORY_WINDOW_KB' {
                if (@('64','128','256','512','1024','2048','4096') -cnotcontains $value) {
                    throw "$name must be a power of two from 64 through 4096"
                }
            }
            'VICTORY_SCRATCH_SPARE_MB' {
                $mb = 0
                if (-not [int]::TryParse($value, [ref]$mb) -or $mb -gt 512) {
                    throw "$name must be between 0 and 512"
                }
                $value = $mb.ToString()
            }
            'VICTORY_VRAM_TARGET_MB' {
                $mb = 0
                if (-not [int]::TryParse($value, [ref]$mb) -or ($mb -ne 0 -and ($mb -lt 1024 -or $mb -gt 65536))) {
                    throw "$name must be 0 (automatic) or between 1024 and 65536"
                }
                $value = $mb.ToString()
            }
            'VICTORY_BUFFERED_TITLE_READS' {
                if (@('0','1') -cnotcontains $value) {
                    throw "$name must be 0 or 1"
                }
            }
        }
        $settings[$name] = $value
    }
    return $settings
}

$profilePath = Join-Path $root ('presets\' + $Preset.ToLowerInvariant() + '.env')
$settings = Read-PerformanceEnv $profilePath
foreach ($name in $names) {
    if (-not $settings.ContainsKey($name)) {
        throw "Incomplete preset: $name is missing from $profilePath"
    }
}
$localPath = Join-Path $root '.env'
if (Test-Path -LiteralPath $localPath -PathType Leaf) {
    $local = Read-PerformanceEnv $localPath
    foreach ($name in $local.Keys) {
        $settings[$name] = $local[$name]
    }
}

Write-Host "VIctory performance preset: $Preset"
foreach ($name in $names) {
    Write-Host "$name=$($settings[$name])"
}
if ($ShowSettings) { return }

if (-not $VictoryArgs -or $VictoryArgs.Count -eq 0) {
    throw 'Specify -VictoryArgs, e.g. @("run", "D:\game\title.exe", "--execute")'
}
$exe = (Get-Item -LiteralPath $VictoryExe).FullName
if (-not (Test-Path -LiteralPath $exe -PathType Leaf)) {
    throw "VIctory executable not found: $exe"
}

# Save and restore only the managed variables, including absent values.
$previous = @{}
foreach ($name in $names) {
    $previous[$name] = [Environment]::GetEnvironmentVariable($name, 'Process')
}
try {
    foreach ($name in $names) {
        [Environment]::SetEnvironmentVariable($name, $settings[$name], 'Process')
    }
    & $exe @VictoryArgs
    $code = $LASTEXITCODE
}
finally {
    foreach ($name in $names) {
        [Environment]::SetEnvironmentVariable($name, $previous[$name], 'Process')
    }
}
exit $code
