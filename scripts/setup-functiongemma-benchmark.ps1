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
$maxDownloadAttempts = 3

$resolvedRoot = [System.IO.Path]::GetFullPath($ExperimentRoot)
$runtimeDirectory = Join-Path $resolvedRoot "llama-$runtimeVersion"
$archivePath = Join-Path $resolvedRoot $runtimeArchiveName
$modelDirectory = Join-Path $resolvedRoot 'models'
$modelPath = Join-Path $modelDirectory $modelName
$serverPath = Join-Path $runtimeDirectory 'llama-server.exe'

New-Item -ItemType Directory -Force -Path $resolvedRoot, $modelDirectory | Out-Null

$hfTokenRequiredMessage = @"
FunctionGemma access requires you to accept Google's Gemma license at:
https://huggingface.co/google/functiongemma-270m-it

After accepting it, set HF_TOKEN in this PowerShell session and rerun this script.
The token is used only as an Authorization header and is never written by this script.
"@

if (-not (Test-Path -LiteralPath $modelPath) -and -not $env:HF_TOKEN) {
    throw $hfTokenRequiredMessage
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

function Remove-IncompleteDownload {
    param(
        [Parameter(Mandatory)]
        [string]$Path
    )

    if (Test-Path -LiteralPath $Path) {
        Remove-Item -LiteralPath $Path -Force -ErrorAction Stop
    }
}

function Get-VerifiedRemoteFile {
    param(
        [Parameter(Mandatory)]
        [string]$Uri,
        [Parameter(Mandatory)]
        [string]$OutFile,
        [Parameter(Mandatory)]
        [string]$ExpectedSha256,
        [hashtable]$Headers = @{},
        [int]$MaxAttempts = 3
    )

    $lastError = $null
    for ($attempt = 1; $attempt -le $MaxAttempts; $attempt++) {
        try {
            Remove-IncompleteDownload -Path $OutFile
            $request = @{
                Uri = $Uri
                OutFile = $OutFile
                UseBasicParsing = $true
            }
            if ($Headers.Count -gt 0) {
                $request.Headers = $Headers
            }
            Invoke-WebRequest @request
            Confirm-Hash -Path $OutFile -Expected $ExpectedSha256
            return
        } catch {
            $lastError = $_
            Remove-IncompleteDownload -Path $OutFile
            if ($attempt -ge $MaxAttempts) {
                break
            }
            $delaySeconds = [Math]::Min(2 * $attempt, 10)
            Write-Warning (
                "Download attempt $attempt/$MaxAttempts failed for $OutFile. " +
                "Removed incomplete file and retrying in ${delaySeconds}s. $_"
            )
            Start-Sleep -Seconds $delaySeconds
        }
    }

    throw "Failed to download a checksum-verified file to $OutFile after $MaxAttempts attempts. $lastError"
}

function Install-VerifiedFile {
    param(
        [Parameter(Mandatory)]
        [string]$Path,
        [Parameter(Mandatory)]
        [string]$Uri,
        [Parameter(Mandatory)]
        [string]$ExpectedSha256,
        [hashtable]$Headers = @{}
    )

    if (Test-Path -LiteralPath $Path) {
        try {
            Confirm-Hash -Path $Path -Expected $ExpectedSha256
            return
        } catch {
            Write-Warning "Existing file failed checksum verification; removing and re-downloading. $_"
            Remove-IncompleteDownload -Path $Path
        }
    }

    Get-VerifiedRemoteFile `
        -Uri $Uri `
        -OutFile $Path `
        -ExpectedSha256 $ExpectedSha256 `
        -Headers $Headers `
        -MaxAttempts $maxDownloadAttempts
}

if (-not (Test-Path -LiteralPath $serverPath)) {
    Install-VerifiedFile `
        -Path $archivePath `
        -Uri $runtimeUrl `
        -ExpectedSha256 $runtimeSha256
    Expand-Archive -LiteralPath $archivePath -DestinationPath $runtimeDirectory -Force
}

$modelReady = $false
if (Test-Path -LiteralPath $modelPath) {
    try {
        Confirm-Hash -Path $modelPath -Expected $modelSha256
        $modelReady = $true
    } catch {
        Write-Warning "Existing model failed checksum verification; removing incomplete download. $_"
        Remove-IncompleteDownload -Path $modelPath
    }
}

if (-not $modelReady) {
    if (-not $env:HF_TOKEN) {
        throw $hfTokenRequiredMessage
    }

    Get-VerifiedRemoteFile `
        -Uri $modelUrl `
        -OutFile $modelPath `
        -ExpectedSha256 $modelSha256 `
        -Headers @{ Authorization = "Bearer $env:HF_TOKEN" } `
        -MaxAttempts $maxDownloadAttempts
}

[pscustomobject]@{
    RuntimeVersion = $runtimeVersion
    ServerPath = $serverPath
    ModelPath = $modelPath
    ModelBytes = (Get-Item -LiteralPath $modelPath).Length
}
