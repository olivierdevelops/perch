# perch installer for Windows.
#
# Usage:
#   irm https://raw.githubusercontent.com/olivierdevelops/perch/main/scripts/install.ps1 | iex
#
# Environment:
#   $env:PERCH_VERSION       version tag to install (default: latest)
#   $env:PERCH_INSTALL_DIR   install destination (default: $env:LOCALAPPDATA\Programs\perch)

$ErrorActionPreference = "Stop"

$Repo = "olivierdevelops/perch"

# ── OS / arch ───────────────────────────────────────────────────────────────
$Os = "windows"
$Arch = switch ($env:PROCESSOR_ARCHITECTURE) {
    "AMD64" { "amd64" }
    "ARM64" { "arm64" }
    default { throw "Unsupported architecture: $($env:PROCESSOR_ARCHITECTURE)" }
}

# ── version ─────────────────────────────────────────────────────────────────
$Version = if ($env:PERCH_VERSION) { $env:PERCH_VERSION } else { "latest" }
if ($Version -eq "latest") {
    $resp = Invoke-RestMethod "https://api.github.com/repos/$Repo/releases/latest"
    $Version = $resp.tag_name
    if (-not $Version) { throw "Could not resolve latest version" }
}

# ── install dir ─────────────────────────────────────────────────────────────
$DestDir = if ($env:PERCH_INSTALL_DIR) {
    $env:PERCH_INSTALL_DIR
} else {
    Join-Path $env:LOCALAPPDATA "Programs\perch"
}
New-Item -ItemType Directory -Force -Path $DestDir | Out-Null

# ── download ───────────────────────────────────────────────────────────────
$BinName = "perch-$Os-$Arch.exe"
$Url = "https://github.com/$Repo/releases/download/$Version/$BinName"
$SumUrl = "$Url.sha256"

$Tmp = New-Item -ItemType Directory -Path (Join-Path $env:TEMP "perch-install-$([System.Guid]::NewGuid())")
try {
    $BinPath = Join-Path $Tmp $BinName
    Write-Host "→ downloading $BinName ($Version)"
    Invoke-WebRequest -Uri $Url -OutFile $BinPath -UseBasicParsing

    try {
        $SumPath = Join-Path $Tmp "$BinName.sha256"
        Invoke-WebRequest -Uri $SumUrl -OutFile $SumPath -UseBasicParsing -ErrorAction SilentlyContinue
        if (Test-Path $SumPath) {
            $Expected = (Get-Content $SumPath).Split(' ')[0]
            $Actual   = (Get-FileHash $BinPath -Algorithm SHA256).Hash.ToLower()
            if ($Expected -ne $Actual) {
                throw "Checksum mismatch (expected $Expected, got $Actual)"
            }
            Write-Host "→ checksum verified"
        }
    } catch { }

    Move-Item -Force $BinPath (Join-Path $DestDir "perch.exe")
} finally {
    Remove-Item -Recurse -Force $Tmp -ErrorAction SilentlyContinue
}

Write-Host ""
Write-Host "✓ installed $(& "$DestDir\perch.exe" --version)"
Write-Host "  path: $DestDir\perch.exe"

$pathEntries = $env:PATH -split ';'
if ($pathEntries -notcontains $DestDir) {
    Write-Host ""
    Write-Host "  ⚠  $DestDir is not on your PATH. Add it with:"
    Write-Host "      [Environment]::SetEnvironmentVariable('Path', `"`$env:Path;$DestDir`", 'User')"
}
