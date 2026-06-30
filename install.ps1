param(
    [string]$Version = "",
    [string]$BinDir = "",
    [switch]$Force,
    [switch]$EasyMode,
    [switch]$NoVerify,
    [switch]$Quiet,
    [switch]$Help
)

<#
plsql-intelligence Windows installer

One-liner install with cache buster:
  irm "https://github.com/MuhDur/plsql-intelligence/releases/latest/download/install.ps1?$([DateTimeOffset]::UtcNow.ToUnixTimeSeconds())" | iex

Or from the development branch:
  irm "https://raw.githubusercontent.com/MuhDur/plsql-intelligence/main/install.ps1?$([DateTimeOffset]::UtcNow.ToUnixTimeSeconds())" | iex

Options:
  -Quiet              Suppress non-error output
  -Force              Reinstall even if the selected version is already installed
  -Version <v>        Install a specific release tag/version
  -BinDir <dir>       Install binaries into dir
  -EasyMode           Add the bin dir to the user PATH
  -NoVerify           Skip SHA256 verification
  -Help               Show this help
#>

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$Owner = "MuhDur"
$Repo = "plsql-intelligence"
$ProjectName = "plsql-intelligence"
$PinnedFallbackVersion = "v0.7.0"
$GitHubApiBase = "https://api.github.com/repos/$Owner/$Repo"
$GitHubReleasesUrl = "https://github.com/$Owner/$Repo/releases"
$ReleaseBins = @("plsql", "plsql-depgraph")

if (-not $BinDir) {
    if ($env:LOCALAPPDATA) {
        $BinDir = Join-Path $env:LOCALAPPDATA "Programs\plsql-intelligence\bin"
    } else {
        $BinDir = Join-Path $HOME ".local\bin"
    }
}

function Show-Usage {
    @"
plsql-intelligence Windows installer

Usage:
  irm "https://github.com/MuhDur/plsql-intelligence/releases/latest/download/install.ps1?$([DateTimeOffset]::UtcNow.ToUnixTimeSeconds())" | iex
  powershell -ExecutionPolicy Bypass -File install.ps1 [options]

Options:
  -Quiet              Suppress non-error output
  -Force              Reinstall even if the selected version is already installed
  -Version <v>        Install a specific release tag/version
  -BinDir <dir>       Install binaries into dir
  -EasyMode           Add the bin dir to the user PATH
  -NoVerify           Skip SHA256 verification
  -Help               Show this help
"@
}

function Write-Info([string]$Message) {
    if (-not $Quiet) { Write-Host "-> $Message" -ForegroundColor Cyan }
}

function Write-Ok([string]$Message) {
    if (-not $Quiet) { Write-Host "OK $Message" -ForegroundColor Green }
}

function Write-Warn([string]$Message) {
    if (-not $Quiet) { Write-Host "WARN $Message" -ForegroundColor Yellow }
}

function Write-Fail([string]$Message) {
    Write-Host "ERROR $Message" -ForegroundColor Red -ErrorAction Continue
}

function Resolve-Version {
    if ($Version) {
        return @{ Value = $Version; Source = "flag" }
    }

    try {
        $latest = Invoke-RestMethod -Uri "$GitHubApiBase/releases/latest" -Headers @{ "User-Agent" = "$ProjectName-installer" }
        if ($latest.tag_name) {
            return @{ Value = [string]$latest.tag_name; Source = "GitHub API" }
        }
    } catch {
        Write-Warn "GitHub API version lookup failed; trying pinned fallback"
    }

    return @{ Value = $PinnedFallbackVersion; Source = "pinned fallback" }
}

function Detect-Target {
    $arch = $env:PROCESSOR_ARCHITECTURE
    if ($arch -eq "AMD64" -or $arch -eq "x86_64") {
        return "x86_64-pc-windows-msvc"
    }
    throw "Unsupported Windows architecture: $arch. Build from source or use a release asset for this architecture."
}

function Get-MarkerPath {
    Join-Path $BinDir ".plsql-intelligence-install"
}

function Test-AlreadyInstalled([string]$ResolvedVersion, [string]$Target) {
    if ($Force) { return $false }

    foreach ($bin in $ReleaseBins) {
        $path = Join-Path $BinDir "$bin.exe"
        if (-not (Test-Path $path)) { return $false }
    }

    $marker = Get-MarkerPath
    if (-not (Test-Path $marker)) { return $false }
    $markerText = Get-Content $marker -Raw
    return ($markerText -match "version=$([regex]::Escape($ResolvedVersion))" -and
            $markerText -match "target=$([regex]::Escape($Target))")
}

function Write-InstallMarker([string]$ResolvedVersion, [string]$Target, [string]$Source) {
    $marker = Get-MarkerPath
    @(
        "version=$ResolvedVersion"
        "target=$Target"
        "source=$Source"
    ) | Set-Content -Path $marker -Encoding UTF8
}

function Invoke-Download([string]$Uri, [string]$OutFile) {
    Invoke-WebRequest -Uri $Uri -OutFile $OutFile -UseBasicParsing -Headers @{ "User-Agent" = "$ProjectName-installer" }
}

function Download-ReleaseFile([string]$ResolvedVersion, [string]$AssetName, [string]$OutFile) {
    $versioned = "$GitHubReleasesUrl/download/$ResolvedVersion/$AssetName"
    try {
        Invoke-Download -Uri $versioned -OutFile $OutFile
        return $versioned
    } catch {
        $latest = "$GitHubReleasesUrl/latest/download/$AssetName"
        Invoke-Download -Uri $latest -OutFile $OutFile
        return $latest
    }
}

function Get-ExpectedSha([string]$ShaFile, [string]$AssetName) {
    $line = Get-Content $ShaFile | Where-Object {
        $_ -match "\s+$([regex]::Escape($AssetName))$"
    } | Select-Object -First 1

    if (-not $line) {
        throw "SHA256SUMS has no entry for $AssetName"
    }
    return (($line -split "\s+")[0]).ToLowerInvariant()
}

function Verify-Sha256([string]$File, [string]$ShaFile, [string]$AssetName) {
    if ($NoVerify) {
        Write-Warn "SHA256 verification disabled"
        return
    }

    $expected = Get-ExpectedSha -ShaFile $ShaFile -AssetName $AssetName
    $actual = (Get-FileHash -Algorithm SHA256 -Path $File).Hash.ToLowerInvariant()
    if ($actual -ne $expected) {
        throw "Checksum mismatch for $AssetName. Expected $expected, got $actual"
    }
    Write-Ok "SHA256 verified: $AssetName"
}

function Install-Binary([string]$Source, [string]$BinName) {
    New-Item -ItemType Directory -Force -Path $BinDir | Out-Null
    $destination = Join-Path $BinDir "$BinName.exe"
    Copy-Item -Force -Path $Source -Destination $destination
    Write-Ok "Installed $BinName -> $destination"
}

function Maybe-AddPath {
    $current = [Environment]::GetEnvironmentVariable("Path", "User")
    if (-not $current) { $current = "" }
    $parts = $current -split ";"
    if ($parts -contains $BinDir) {
        Write-Info "$BinDir is already on the user PATH"
        return
    }

    if ($EasyMode) {
        $next = if ($current) { "$current;$BinDir" } else { $BinDir }
        [Environment]::SetEnvironmentVariable("Path", $next, "User")
        Write-Ok "Added $BinDir to the user PATH"
    } else {
        Write-Warn "$BinDir is not on PATH; rerun with -EasyMode or add it to the user PATH"
    }
}

function Show-Summary([string]$ResolvedVersion, [string]$Target) {
    if ($Quiet) { return }
    Write-Host ""
    Write-Host "plsql-intelligence installed" -ForegroundColor Green
    Write-Host "  Version: $ResolvedVersion"
    Write-Host "  Target:  $Target"
    Write-Host "  Bin dir: $BinDir"
    foreach ($bin in $ReleaseBins) {
        $path = Join-Path $BinDir "$bin.exe"
        if (Test-Path $path) {
            $versionLine = & $path --version 2>$null
            Write-Host "  ${bin}: $versionLine"
        }
    }
    Write-Host "  Uninstall: remove plsql.exe, plsql-depgraph.exe, and .plsql-intelligence-install from the bin dir"
}

function Main {
    if ($Help) {
        Show-Usage
        return
    }

    $target = Detect-Target
    $resolved = Resolve-Version
    $resolvedVersion = [string]$resolved.Value
    $versionSource = [string]$resolved.Source

    Write-Info "Repository: $Owner/$Repo"
    Write-Info "Install dir: $BinDir"
    Write-Info "Target: $target"
    Write-Info "Version: $resolvedVersion ($versionSource)"

    if (Test-AlreadyInstalled -ResolvedVersion $resolvedVersion -Target $target) {
        Write-Ok "Requested release already installed in $BinDir"
        Maybe-AddPath
        Show-Summary -ResolvedVersion $resolvedVersion -Target $target
        return
    }

    $temp = Join-Path ([IO.Path]::GetTempPath()) ([IO.Path]::GetRandomFileName())
    New-Item -ItemType Directory -Force -Path $temp | Out-Null
    try {
        $shaFile = Join-Path $temp "SHA256SUMS"
        Download-ReleaseFile -ResolvedVersion $resolvedVersion -AssetName "SHA256SUMS" -OutFile $shaFile | Out-Null

        foreach ($bin in $ReleaseBins) {
            $asset = "$bin-$target.exe"
            $downloaded = Join-Path $temp $asset
            Download-ReleaseFile -ResolvedVersion $resolvedVersion -AssetName $asset -OutFile $downloaded | Out-Null
            Verify-Sha256 -File $downloaded -ShaFile $shaFile -AssetName $asset
            Install-Binary -Source $downloaded -BinName $bin
        }

        Write-InstallMarker -ResolvedVersion $resolvedVersion -Target $target -Source $versionSource
        Maybe-AddPath
        Show-Summary -ResolvedVersion $resolvedVersion -Target $target
        Write-Ok "Installation complete"
    } finally {
        if (Test-Path $temp) {
            Remove-Item -Recurse -Force $temp
        }
    }
}

try {
    Main
} catch {
    Write-Fail $_.Exception.Message
    exit 1
}
