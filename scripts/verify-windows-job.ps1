# Exercise abrupt parent exit, which skips Rust Drop implementations.
$ErrorActionPreference = 'Stop'
$probe = Join-Path $PSScriptRoot '../src-tauri/target/debug/sandbox_probe.exe'
$output = & $probe --job-kill-parent
if ($LASTEXITCODE -ne 0) {
    throw 'AppContainer parent probe failed'
}
$childId = 0
if (-not [int]::TryParse(($output | Out-String).Trim(), [ref]$childId) -or $childId -le 0) {
    throw 'Parent probe did not report a child process ID'
}
$child = Get-Process -Id $childId -ErrorAction SilentlyContinue
if ($null -ne $child) {
    try {
        if (-not $child.WaitForExit(5000)) {
            throw "AppContainer child $childId survived launcher exit"
        }
    } finally {
        $child.Dispose()
    }
}
Write-Host 'PROCESS PASS: AppContainer child terminated after abrupt launcher exit'
