[CmdletBinding()]
param(
    [string]$ExperimentRoot
)

$ErrorActionPreference = 'Stop'
if (-not $ExperimentRoot) {
    $ExperimentRoot = Join-Path $PSScriptRoot '..\.tmp\functiongemma-prototype'
}

$runtimeVersion = 'b10107'
$runtimeArchiveName = "llama-$runtimeVersion-bin-win-cpu-x64.zip"
$runtimeUrl = "https://github.com/ggml-org/llama.cpp/releases/download/$runtimeVersion/$runtimeArchiveName"
$runtimeSha256 = '52133a0a5a8f6035b1bdd2f89c3425ea8b742413d9bdb9a2dee30e3a1681b18c'
$modelName = 'functiongemma-270m-it-q8_0.gguf'
$modelUrl = "https://huggingface.co/ggml-org/functiongemma-270m-it-GGUF/resolve/main/$modelName"
$modelSha256 = '83940d4dd9676710856f43523bed096164a595a96f6b34771610a03937de5270'

$resolvedRoot = [System.IO.Path]::GetFullPath($ExperimentRoot)
$runtimeDirectory = Join-Path $resolvedRoot "llama-$runtimeVersion"
$archivePath = Join-Path $resolvedRoot $runtimeArchiveName
$modelDirectory = Join-Path $resolvedRoot 'models'
$modelPath = Join-Path $modelDirectory $modelName
$serverPath = Join-Path $runtimeDirectory 'llama-server.exe'

New-Item -ItemType Directory -Force -Path $resolvedRoot, $modelDirectory | Out-Null

if (-not (Test-Path -LiteralPath $modelPath) -and -not $env:HF_TOKEN) {
    throw @"
FunctionGemma access requires you to accept Google's Gemma license at:
https://huggingface.co/google/functiongemma-270m-it

After accepting it, set HF_TOKEN in this PowerShell session and rerun this script.
The token is used only as an Authorization header and is never written by this script.
"@
}

function Confirm-Hash {
    param(
        [Parameter(Mandatory)]
        [string]$Path,
        [Parameter(Mandatory)]
        [string]$Expected
    )

    $actual = (Get-FileHash -Algorithm SHA256 -LiteralPath $Path).Hash.ToLowerInvariant()
    if ($actual -ne $Expected) {
        throw "Checksum mismatch for $Path. Expected $Expected, received $actual."
    }
}

if (-not (Test-Path -LiteralPath $serverPath)) {
    if (-not (Test-Path -LiteralPath $archivePath)) {
        Invoke-WebRequest -Uri $runtimeUrl -OutFile $archivePath -UseBasicParsing
    }
    Confirm-Hash -Path $archivePath -Expected $runtimeSha256
    Expand-Archive -LiteralPath $archivePath -DestinationPath $runtimeDirectory -Force
}

if (-not (Test-Path -LiteralPath $modelPath)) {
    Invoke-WebRequest `
        -Uri $modelUrl `
        -Headers @{ Authorization = "Bearer $env:HF_TOKEN" } `
        -OutFile $modelPath `
        -UseBasicParsing
}

Confirm-Hash -Path $modelPath -Expected $modelSha256

[pscustomobject]@{
    RuntimeVersion = $runtimeVersion
    ServerPath = $serverPath
    ModelPath = $modelPath
    ModelBytes = (Get-Item -LiteralPath $modelPath).Length
}
