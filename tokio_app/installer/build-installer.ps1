#Requires -Version 5.1
<#
.SYNOPSIS
  Builds release .exe and Inno Setup installer (optional VC++ + Prolific driver).

.PARAMETER SkipBuild
  Skip cargo; use existing target\release\com-port-plotter.exe

.PARAMETER WithVcRedist
  Download VC_redist.x64.exe into installer\redist\

.PARAMETER RequireProlific
  Fail if Prolific driver installer is missing from drivers\prolific\

.PARAMETER IsccPath
  Full path to ISCC.exe

.EXAMPLE
  .\installer\build-installer.ps1 -WithVcRedist -RequireProlific
#>
[CmdletBinding()]
param(
    [switch]$SkipBuild,
    [switch]$WithVcRedist,
    [switch]$RequireProlific,
    [string]$IsccPath = ""
)

$ErrorActionPreference = "Stop"

$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$CrateDir = Resolve-Path (Join-Path $ScriptDir "..")
$IssFile = Join-Path $ScriptDir "com-port-plotter.iss"
$OutputDir = Join-Path $ScriptDir "output"
$RedistDir = Join-Path $ScriptDir "redist"
$ProlificDir = Join-Path $ScriptDir "drivers\prolific"
$ProlificCanonical = Join-Path $ProlificDir "PL23XX_Prolific_DriverInstaller.exe"
$ExePath = Join-Path $CrateDir "target\release\com-port-plotter.exe"
$ProlificDownloadPage = "https://www.prolific.com.tw/US/ShowProduct.aspx?p_id=225&pcid=41"

Write-Host "==> Crate: $CrateDir" -ForegroundColor Cyan

# --- 1. Release build ---
if (-not $SkipBuild) {
    Write-Host "==> cargo build --release" -ForegroundColor Cyan
    Push-Location $CrateDir
    try {
        cargo build --release
        if ($LASTEXITCODE -ne 0) {
            throw "cargo build --release failed (exit $LASTEXITCODE)"
        }
    }
    finally {
        Pop-Location
    }
}

if (-not (Test-Path $ExePath)) {
    throw "Missing $ExePath. Build release first or omit -SkipBuild."
}

$exeInfo = Get-Item $ExePath
$sizeMb = [math]::Round($exeInfo.Length / 1MB, 1)
Write-Host "==> EXE: $($exeInfo.FullName) ($sizeMb MB)" -ForegroundColor Green

# --- 2. Optional VC++ redist ---
$VcUrl = "https://aka.ms/vs/17/release/vc_redist.x64.exe"
$VcLocal = Join-Path $RedistDir "VC_redist.x64.exe"

if ($WithVcRedist) {
    New-Item -ItemType Directory -Force -Path $RedistDir | Out-Null
    if (-not (Test-Path $VcLocal)) {
        Write-Host "==> Downloading VC++ Redistributable..." -ForegroundColor Cyan
        Invoke-WebRequest -Uri $VcUrl -OutFile $VcLocal -UseBasicParsing
    }
    else {
        Write-Host "==> VC++ redist already present: $VcLocal" -ForegroundColor DarkGray
    }
}
elseif (-not (Test-Path $VcLocal)) {
    Write-Host "==> VC++ redist not included (use -WithVcRedist if needed)" -ForegroundColor DarkGray
}

# --- 3. Prolific driver installer ---
New-Item -ItemType Directory -Force -Path $ProlificDir | Out-Null

function Resolve-ProlificInstaller {
    if (Test-Path $ProlificCanonical) {
        return (Get-Item $ProlificCanonical)
    }
    $candidates = @()
    $candidates += Get-ChildItem -Path $ProlificDir -Filter "PL23XX*.exe" -File -ErrorAction SilentlyContinue
    $candidates += Get-ChildItem -Path $ProlificDir -Filter "*Prolific*Driver*.exe" -File -ErrorAction SilentlyContinue
    $candidates += Get-ChildItem -Path $ProlificDir -Filter "*DriverInstaller*.exe" -File -ErrorAction SilentlyContinue
    $picked = $candidates | Sort-Object LastWriteTime -Descending | Select-Object -First 1
    if ($picked) {
        if ($picked.FullName -ne $ProlificCanonical) {
            Write-Host "==> Using Prolific installer: $($picked.Name)" -ForegroundColor Cyan
            Copy-Item -Force $picked.FullName $ProlificCanonical
        }
        return (Get-Item $ProlificCanonical)
    }
    return $null
}

$prolific = Resolve-ProlificInstaller
if ($prolific) {
    $pMb = [math]::Round($prolific.Length / 1MB, 1)
    Write-Host "==> Prolific driver: $($prolific.FullName) ($pMb MB)" -ForegroundColor Green
}
else {
    $msg = @"
==> Prolific driver installer NOT FOUND in:
    $ProlificDir

Download the official Windows installer from Prolific:
    $ProlificDownloadPage
Then place/rename it as:
    $ProlificCanonical

Without this file the setup will still build, but will not install the COM driver.
"@
    if ($RequireProlific) {
        throw $msg
    }
    Write-Host $msg -ForegroundColor Yellow
}

# --- 4. Find Inno Setup compiler ---
function Find-Iscc {
    param([string]$Explicit)

    if ($Explicit -and (Test-Path $Explicit)) {
        return (Resolve-Path $Explicit).Path
    }

    $cmd = Get-Command ISCC.exe -ErrorAction SilentlyContinue
    if ($cmd) {
        return $cmd.Source
    }

    $candidates = @(
        "${env:ProgramFiles(x86)}\Inno Setup 6\ISCC.exe",
        "${env:ProgramFiles}\Inno Setup 6\ISCC.exe",
        "${env:LocalAppData}\Programs\Inno Setup 6\ISCC.exe"
    )
    foreach ($c in $candidates) {
        if (Test-Path $c) {
            return $c
        }
    }
    return $null
}

$iscc = Find-Iscc -Explicit $IsccPath
if (-not $iscc) {
    Write-Host ""
    Write-Host "ISCC.exe (Inno Setup Compiler) not found." -ForegroundColor Yellow
    Write-Host "Install Inno Setup 6: https://jrsoftware.org/isinfo.php"
    Write-Host "Or pass: -IsccPath 'C:\Path\To\ISCC.exe'"
    Write-Host ""
    Write-Host "Release EXE is ready:"
    Write-Host "  $ExePath"
    exit 2
}

Write-Host "==> ISCC: $iscc" -ForegroundColor Cyan
New-Item -ItemType Directory -Force -Path $OutputDir | Out-Null

# --- 5. Compile installer ---
Write-Host "==> Compiling installer..." -ForegroundColor Cyan
& $iscc $IssFile
if ($LASTEXITCODE -ne 0) {
    throw "ISCC failed (exit $LASTEXITCODE)"
}

$setup = Get-ChildItem -Path $OutputDir -Filter "COMPortPlotter-Setup-*.exe" |
    Sort-Object LastWriteTime -Descending |
    Select-Object -First 1

if ($setup) {
    $setupMb = [math]::Round($setup.Length / 1MB, 1)
    Write-Host "==> Done: $($setup.FullName) ($setupMb MB)" -ForegroundColor Green
}
else {
    Write-Host "==> ISCC finished; check folder: $OutputDir" -ForegroundColor Yellow
}
