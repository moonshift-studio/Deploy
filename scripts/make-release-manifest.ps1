<#
.SYNOPSIS
    Generates the tauri-plugin-updater manifest (latest.json) for a deploy
    release. The signature comes from `npx tauri signer sign` output (.sig).

.EXAMPLE
    powershell -ExecutionPolicy Bypass -File <deploy>\scripts\make-release-manifest.ps1 `
        -Config <app>\packaging\app-pack.json `
        -Artifact <app>\packaging\dist\App_0.1.0_x64-setup.exe `
        -Signature <app>\packaging\dist\App_0.1.0_x64-setup.exe.sig `
        -Version 0.1.0 `
        -Repo owner/repo
#>
param(
    [Parameter(Mandatory)] [string]$Config,
    [Parameter(Mandatory)] [string]$Artifact,
    [Parameter(Mandatory)] [string]$Signature,
    [Parameter(Mandatory)] [string]$Version,
    [Parameter(Mandatory)] [string]$Repo,
    [string]$Notes = ""
)

$ErrorActionPreference = "Stop"
$cfg = Get-Content $Config -Raw | ConvertFrom-Json

$assetName = "$($cfg.appName)_$($Version)_x64-setup.exe"
$signature = (Get-Content $Signature -Raw).Trim()

$manifest = [ordered]@{
    version  = $Version
    pub_date = (Get-Date).ToUniversalTime().ToString("yyyy-MM-ddTHH:mm:ssZ")
    notes    = $Notes
    platforms = [ordered]@{
        "windows-x86_64" = [ordered]@{
            signature = $signature
            url       = "https://github.com/$Repo/releases/download/v$Version/$assetName"
        }
    }
}

$outFile = Join-Path (Split-Path -Parent $Artifact) "latest.json"
$manifest | ConvertTo-Json -Depth 5 | Set-Content -Path $outFile -Encoding UTF8
Write-Host "==> Manifest ready: $outFile"
Write-Host "    Asset name: $assetName"
Get-Content $outFile
