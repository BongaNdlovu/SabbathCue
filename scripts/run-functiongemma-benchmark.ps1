[CmdletBinding()]
param(
    [int]$Port = 18089,
    [string]$ShadowInput
)

$ErrorActionPreference = 'Stop'
$repoRoot = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$assets = & (Join-Path $PSScriptRoot 'setup-functiongemma-benchmark.ps1')
$logDirectory = Join-Path $repoRoot '.tmp\functiongemma-prototype\logs'
$stdoutPath = Join-Path $logDirectory 'llama-server.stdout.log'
$stderrPath = Join-Path $logDirectory 'llama-server.stderr.log'
New-Item -ItemType Directory -Force -Path $logDirectory | Out-Null
$quotedModelPath = '"{0}"' -f $assets.ModelPath

$serverArguments = @(
    '--model', $quotedModelPath,
    '--alias', 'functiongemma',
    '--host', '127.0.0.1',
    '--port', $Port,
    '--threads', '2',
    '--threads-batch', '2',
    '--ctx-size', '512',
    '--jinja',
    '--no-webui'
)

$startupTimer = [System.Diagnostics.Stopwatch]::StartNew()
$server = Start-Process `
    -FilePath $assets.ServerPath `
    -ArgumentList $serverArguments `
    -WorkingDirectory $repoRoot `
    -WindowStyle Hidden `
    -RedirectStandardOutput $stdoutPath `
    -RedirectStandardError $stderrPath `
    -PassThru

try {
    $healthUrl = "http://127.0.0.1:$Port/health"
    $ready = $false
    for ($attempt = 0; $attempt -lt 120; $attempt++) {
        if ($server.HasExited) {
            throw "llama-server exited before becoming ready. See $stderrPath."
        }
        try {
            Invoke-RestMethod -Uri $healthUrl -TimeoutSec 1 | Out-Null
            $ready = $true
            break
        } catch {
            Start-Sleep -Milliseconds 500
        }
    }
    if (-not $ready) {
        throw "llama-server did not become ready within 60 seconds. See $stderrPath."
    }
    $startupTimer.Stop()
    $startupMilliseconds = $startupTimer.Elapsed.TotalMilliseconds.ToString(
        [System.Globalization.CultureInfo]::InvariantCulture
    )

    $arguments = @(
        'run',
        '--manifest-path', 'src-tauri/Cargo.toml',
        '-p', 'rhema-detection',
        '--features', 'onnx',
        '--release',
        '--bin', 'command_benchmark',
        '--',
        '--gemma-url', "http://127.0.0.1:$Port/v1/chat/completions",
        '--gemma-model', 'functiongemma',
        '--gemma-model-path', $assets.ModelPath,
        '--gemma-pid', $server.Id,
        '--gemma-startup-ms', $startupMilliseconds
    )
    if ($ShadowInput) {
        $arguments += @('--shadow-input', $ShadowInput)
    }

    Push-Location $repoRoot
    try {
        & cargo @arguments
        if ($LASTEXITCODE -ne 0) {
            throw "The command benchmark exited with code $LASTEXITCODE."
        }
    } finally {
        Pop-Location
    }
} finally {
    if (-not $server.HasExited) {
        Stop-Process -Id $server.Id
        $server.WaitForExit()
    }
}
