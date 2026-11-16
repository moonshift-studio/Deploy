<#
.SYNOPSIS
    Builds a signed sparse (manifest-only) identity MSIX for an app.

    The package grants Windows package identity (e.g. for the
    graphicsCaptureProgrammatic WGC capability) while the app binaries stay
    unpackaged in their normal install directory. Registered at install time
    with: AddPackageByUriAsync(package, ExternalLocationUri=<install dir>).

.PARAMETER Config
    Path to the app's app-pack.json (all paths inside resolve relative to it).

.EXAMPLE
    powershell -ExecutionPolicy Bypass -File <deploy>\scripts\make-identity-package.ps1 -Config <app>\packaging\app-pack.json
#>
param(
    [Parameter(Mandatory)] [string]$Config
)

$ErrorActionPreference = "Stop"
$cfg = Get-Content $Config -Raw | ConvertFrom-Json
$base = Split-Path -Parent (Resolve-Path $Config)

$versionSource = Join-Path $base $cfg.versionSource
$version = "$((Get-Content $versionSource -Raw | ConvertFrom-Json).version).0"
$outDir = Join-Path $base (Split-Path -Parent $cfg.msixPath)
New-Item -ItemType Directory -Force -Path $outDir | Out-Null

# ---------- Tooling (MakeAppx / SignTool from the Windows SDK) ----------
$kits = "C:\Program Files (x86)\Windows Kits\10\bin"
if (-not (Test-Path $kits)) { throw "Windows SDK not found at $kits" }
$sdkBin = Get-ChildItem $kits -Directory |
    Where-Object { Test-Path (Join-Path $_.FullName "x64\MakeAppx.exe") } |
    Sort-Object Name -Descending | Select-Object -First 1
if (-not $sdkBin) { throw "MakeAppx.exe not found in $kits" }
$makeappx = Join-Path $sdkBin.FullName "x64\MakeAppx.exe"
$signtool = Get-ChildItem $kits -Filter "signtool.exe" -Recurse |
    Where-Object { $_.FullName -match "\\x64\\" } |
    Sort-Object FullName -Descending | Select-Object -First 1
Write-Host "==> MakeAppx: $makeappx" -ForegroundColor DarkGray
Write-Host "==> SignTool: $($signtool.FullName)" -ForegroundColor DarkGray

# ---------- Certificate ----------
$subject = $cfg.identityPublisher
$cert = Get-ChildItem Cert:\CurrentUser\My |
    Where-Object { $_.Subject -eq $subject } |
    Sort-Object NotAfter -Descending | Select-Object -First 1
if (-not $cert) {
    throw "No code-signing certificate '$subject' found. Run deploy\scripts\setup-signing.ps1 -Config <this config> first."
}
$cerPath = Join-Path $outDir "$($cfg.appName).cer"
Export-Certificate -Cert $cert -FilePath $cerPath | Out-Null

# ---------- Capabilities (baseline for win32App identity, extendable) ----------
$capabilities = @("runFullTrust", "unvirtualizedResources")
if ($cfg.identityCapabilities) {
    $capabilities = @($cfg.identityCapabilities)
}
$capabilityXml = ($capabilities | ForEach-Object {
    "    <rescap:Capability Name=""$_"" />"
}) -join "`n"

# ---------- Manifest ----------
$stage = Join-Path $env:TEMP "deploy-identity-$($cfg.identityName)"
Remove-Item $stage -Recurse -Force -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Path $stage | Out-Null

$manifest = @"
<?xml version="1.0" encoding="utf-8"?>
<Package
  IgnorableNamespaces="uap uap10 rescap"
  xmlns="http://schemas.microsoft.com/appx/manifest/foundation/windows10"
  xmlns:uap="http://schemas.microsoft.com/appx/manifest/uap/windows10"
  xmlns:uap10="http://schemas.microsoft.com/appx/manifest/uap/windows10/10"
  xmlns:rescap="http://schemas.microsoft.com/appx/manifest/foundation/windows10/restrictedcapabilities">
  <Identity Name="$($cfg.identityName)" Publisher="$($cfg.identityPublisher)" Version="$version" ProcessorArchitecture="neutral" />
  <Properties>
    <DisplayName>$($cfg.appName)</DisplayName>
    <PublisherDisplayName>$($cfg.publisher)</PublisherDisplayName>
    <Logo>Assets\StoreLogo.png</Logo>
    <uap10:AllowExternalContent>true</uap10:AllowExternalContent>
  </Properties>
  <Resources>
    <Resource Language="en-us" />
  </Resources>
  <Dependencies>
    <TargetDeviceFamily Name="Windows.Desktop" MinVersion="10.0.22000.0" MaxVersionTested="10.0.26100.0" />
  </Dependencies>
  <Capabilities>
$capabilityXml
  </Capabilities>
  <Applications>
    <Application Id="$($cfg.appName)" Executable="$($cfg.launchExe)" uap10:TrustLevel="mediumIL" uap10:RuntimeBehavior="win32App">
      <uap:VisualElements
        AppListEntry="none"
        DisplayName="$($cfg.appName)"
        Description="$($cfg.description)"
        BackgroundColor="transparent"
        Square150x150Logo="Assets\Square150x150Logo.png"
        Square44x44Logo="Assets\Square44x44Logo.png" />
    </Application>
  </Applications>
</Package>
"@
$manifestPath = Join-Path $stage "AppxManifest.xml"
Set-Content -Path $manifestPath -Value $manifest -Encoding UTF8

# ---------- Pack and sign (/nv: skip payload validation, payload is external) ----------
$msix = Join-Path $outDir "$($cfg.appName).Identity.msix"
Write-Host "==> Packing $msix" -ForegroundColor Cyan
& $makeappx pack /nv /o /d $stage /p $msix
if ($LASTEXITCODE -ne 0) { throw "MakeAppx failed" }
Write-Host "==> Signing $msix" -ForegroundColor Cyan
# Pin the exact certificate by thumbprint: several certs may share the
# subject once a re-issued signing certificate exists.
& ($signtool.FullName) sign /fd SHA256 /sha1 $cert.Thumbprint $msix
if ($LASTEXITCODE -ne 0) { throw "SignTool failed" }

Write-Host ""
Write-Host "==> Identity package ready: $msix" -ForegroundColor Green
Write-Host "    Certificate:            $cerPath" -ForegroundColor Green
