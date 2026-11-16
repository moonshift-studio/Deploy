<#
.SYNOPSIS
    One-time setup of the two signing materials deploy needs:

      1. A self-signed code-signing certificate (signs the identity MSIX and
         the installer exe). The .pfx is exported for CI; the .cer is embedded
         in installers and trusted per-user at install time (no admin).
      2. A minisign keypair for tauri-plugin-updater artifact signatures.
         The private key becomes the TAURI_SIGNING_PRIVATE_KEY secret; the
         public key goes into the app's tauri.conf.json updater config.

    Keys are stored in a `keys` folder next to the app's app-pack.json
    (gitignore that folder in the app repo).

.PARAMETER Config
    Path to the app's app-pack.json.

.EXAMPLE
    powershell -ExecutionPolicy Bypass -File <deploy>\scripts\setup-signing.ps1 -Config <app>\packaging\app-pack.json
#>
param(
    [Parameter(Mandatory)] [string]$Config,
    [string]$Password = ""
)

$ErrorActionPreference = "Stop"
$cfg = Get-Content $Config -Raw | ConvertFrom-Json
$base = Split-Path -Parent (Resolve-Path $Config)

$keysDir = Join-Path $base "keys"
New-Item -ItemType Directory -Force -Path $keysDir | Out-Null
if (-not $Password) {
    $guid = [guid]::NewGuid().ToString("N")
    $Password = "moonshift-$guid"
}

# ---------- Code-signing certificate ----------
$subject = $cfg.identityPublisher
$cert = Get-ChildItem Cert:\CurrentUser\My |
    Where-Object { $_.Subject -eq $subject -and $_.FriendlyName -like "*deploy*" } |
    Sort-Object NotAfter -Descending | Select-Object -First 1
if (-not $cert) {
    Write-Host "==> Creating self-signed code-signing certificate ($subject)" -ForegroundColor Cyan
    # The extensions are required for package signing: Code Signing EKU and
    # Basic Constraints CA=true (the empty {text} value) — without CA the
    # self-signed cert cannot serve as its own trusted root and sparse MSIX
    # registration fails with 0x800B0109 (untrusted root).
    $cert = New-SelfSignedCertificate `
        -Type Custom `
        -Subject $subject `
        -KeyUsage DigitalSignature `
        -KeyExportPolicy Exportable `
        -KeySpec Signature `
        -FriendlyName "$($cfg.appName) deploy signing" `
        -CertStoreLocation "Cert:\CurrentUser\My" `
        -NotAfter (Get-Date).AddYears(10) `
        -TextExtension @("2.5.29.37={text}1.3.6.1.5.5.7.3.3", "2.5.29.19={text}")
} else {
    Write-Host "==> Reusing existing certificate $($cert.Thumbprint)" -ForegroundColor Yellow
}

$pfxPath = Join-Path $keysDir "$($cfg.appName).pfx"
$cerPath = Join-Path $keysDir "$($cfg.appName).cer"
$pw = ConvertTo-SecureString -String $Password -AsPlainText -Force
Export-PfxCertificate -Cert $cert -FilePath $pfxPath -Password $pw | Out-Null
Export-Certificate -Cert $cert -FilePath $cerPath | Out-Null
Write-Host "==> PFX:  $pfxPath"
Write-Host "==> CER:  $cerPath"

# ---------- Minisign keypair for the updater ----------
$scriptDir = Split-Path -Parent $PSScriptRoot
$keyFile = Join-Path $keysDir "$($cfg.appName)-update.key"
if (-not (Test-Path $keyFile)) {
    Write-Host "==> Generating minisign updater keypair" -ForegroundColor Cyan
    Push-Location $scriptDir
    try {
        npx tauri signer generate -w $keyFile --password $Password
        if ($LASTEXITCODE -ne 0) { throw "tauri signer generate failed" }
    } finally {
        Pop-Location
    }
} else {
    Write-Host "==> Reusing existing minisign key $keyFile" -ForegroundColor Yellow
}

$pubKey = (Get-Content "$keyFile.pub" -Raw).Trim()

Write-Host ""
Write-Host "=================================================================" -ForegroundColor Cyan
Write-Host " GitHub Actions secrets to set:" -ForegroundColor Cyan
Write-Host "=================================================================" -ForegroundColor Cyan
Write-Host "  TAURI_SIGNING_PRIVATE_KEY           <contents of $keyFile>"
Write-Host "  TAURI_SIGNING_PRIVATE_KEY_PASSWORD  $Password"
Write-Host "  WINDOWS_CERT_PFX                    <contents of $pfxPath, base64>"
Write-Host "  WINDOWS_CERT_PASSWORD               $Password"
Write-Host ""
Write-Host "=================================================================" -ForegroundColor Cyan
Write-Host " Updater public key for the app's tauri.conf.json:" -ForegroundColor Cyan
Write-Host "=================================================================" -ForegroundColor Cyan
Write-Host $pubKey
Write-Host ""
Write-Host "Store the password somewhere safe - it is not recoverable." -ForegroundColor Yellow
