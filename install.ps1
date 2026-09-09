<#
    vibe-control - Windows command-line installer.

    Downloads the latest installer (.msi preferred, else -setup.exe) from the
    GitHub Releases page and installs it silently.

    Usage (PowerShell):
        irm https://raw.githubusercontent.com/reiple/vibe-control/main/install.ps1 | iex

    Optional: pin a version
        $env:VC_VERSION = 'v0.1.0'; irm https://raw.githubusercontent.com/reiple/vibe-control/main/install.ps1 | iex
#>

$ErrorActionPreference = 'Stop'
$Repo = 'reiple/vibe-control'

function Write-Step($msg) { Write-Host "==> $msg" -ForegroundColor Green }
function Die($msg) { Write-Host "error: $msg" -ForegroundColor Red; exit 1 }

# GitHub API needs TLS 1.2 on older PowerShell (Windows PowerShell 5.1).
try { [Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12 } catch {}

$headers = @{ 'Accept' = 'application/vnd.github+json'; 'User-Agent' = 'vibe-control-installer' }
if ($env:VC_VERSION) {
    $apiUrl = "https://api.github.com/repos/$Repo/releases/tags/$($env:VC_VERSION)"
} else {
    $apiUrl = "https://api.github.com/repos/$Repo/releases/latest"
}

Write-Step "Looking up release from $Repo ..."
try {
    $release = Invoke-RestMethod -Uri $apiUrl -Headers $headers
} catch {
    Die "could not reach GitHub Releases (is the repo public and a release published?)."
}

# Prefer the NSIS -setup.exe (supports a clean silent /S install), else the MSI.
$asset = $release.assets | Where-Object { $_.name -like '*-setup.exe' } | Select-Object -First 1
$isMsi = $false
if (-not $asset) {
    $asset = $release.assets | Where-Object { $_.name -like '*.msi' } | Select-Object -First 1
    $isMsi = $true
}
if (-not $asset) { Die "no .msi or -setup.exe asset found in the release." }

$dest = Join-Path $env:TEMP $asset.name
Write-Step "Downloading $($asset.name) ..."
try {
    Invoke-WebRequest -Uri $asset.browser_download_url -OutFile $dest -Headers $headers
} catch {
    Die "download failed."
}

Write-Step "Installing (silent) ..."
if ($isMsi) {
    $p = Start-Process -FilePath 'msiexec.exe' -ArgumentList @('/i', "`"$dest`"", '/qn', '/norestart') -Wait -PassThru
} else {
    # NSIS silent install
    $p = Start-Process -FilePath $dest -ArgumentList '/S' -Wait -PassThru
}
if ($p.ExitCode -ne 0) { Die "installer exited with code $($p.ExitCode)." }

Remove-Item $dest -Force -ErrorAction SilentlyContinue
Write-Step "Installed. Launch vibe-control from the Start menu."
Write-Host "    (Unsigned build: Windows SmartScreen may warn 'Unknown publisher' - choose More info > Run anyway.)"
