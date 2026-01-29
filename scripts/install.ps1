# tappr installer for Windows PowerShell
# Usage: irm https://raw.githubusercontent.com/jonasrmichel/tappr/main/scripts/install.ps1 | iex

$ErrorActionPreference = "Stop"

$Repo = "jonasrmichel/tappr"
$BinaryName = "tappr"

function Write-Info { param($Message) Write-Host "[info] " -ForegroundColor Blue -NoNewline; Write-Host $Message }
function Write-Success { param($Message) Write-Host "[success] " -ForegroundColor Green -NoNewline; Write-Host $Message }
function Write-Warn { param($Message) Write-Host "[warn] " -ForegroundColor Yellow -NoNewline; Write-Host $Message }
function Write-Error { param($Message) Write-Host "[error] " -ForegroundColor Red -NoNewline; Write-Host $Message; exit 1 }

function Get-Architecture {
    $arch = [System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture
    switch ($arch) {
        "X64" { return "x86_64" }
        "Arm64" { return "aarch64" }
        default { Write-Error "Unsupported architecture: $arch" }
    }
}

function Get-LatestVersion {
    try {
        $release = Invoke-RestMethod -Uri "https://api.github.com/repos/$Repo/releases/latest" -UseBasicParsing
        return $release.tag_name
    } catch {
        Write-Error "Failed to fetch latest version: $_"
    }
}

function Install-Tappr {
    Write-Host ""
    Write-Host "  tappr installer" -ForegroundColor Cyan
    Write-Host "  ===============" -ForegroundColor Cyan
    Write-Host ""

    $arch = Get-Architecture
    $target = "$arch-pc-windows-msvc"

    Write-Info "Detected architecture: $arch"
    Write-Info "Target: $target"

    # Get latest version
    Write-Info "Fetching latest release..."
    $version = Get-LatestVersion
    Write-Info "Latest version: $version"

    # Construct download URL
    $downloadUrl = "https://github.com/$Repo/releases/download/$version/$BinaryName-$target.zip"

    # Create temp directory
    $tmpDir = Join-Path $env:TEMP "tappr-install-$(Get-Random)"
    New-Item -ItemType Directory -Path $tmpDir -Force | Out-Null

    try {
        # Download
        Write-Info "Downloading $BinaryName $version..."
        $zipPath = Join-Path $tmpDir "$BinaryName.zip"

        try {
            Invoke-WebRequest -Uri $downloadUrl -OutFile $zipPath -UseBasicParsing
        } catch {
            Write-Error "Failed to download from: $downloadUrl`n$_"
        }

        # Extract
        Write-Info "Extracting..."
        Expand-Archive -Path $zipPath -DestinationPath $tmpDir -Force

        # Determine install location
        $installDir = Join-Path $env:LOCALAPPDATA "tappr"
        if (-not (Test-Path $installDir)) {
            New-Item -ItemType Directory -Path $installDir -Force | Out-Null
        }

        # Install binary
        Write-Info "Installing to $installDir..."
        $binaryPath = Join-Path $tmpDir "$BinaryName.exe"
        $destPath = Join-Path $installDir "$BinaryName.exe"
        Move-Item -Path $binaryPath -Destination $destPath -Force

        Write-Success "Installed $BinaryName $version to $destPath"

        # Add to PATH if not already there
        $userPath = [Environment]::GetEnvironmentVariable("PATH", "User")
        if ($userPath -notlike "*$installDir*") {
            Write-Info "Adding $installDir to PATH..."
            $newPath = "$userPath;$installDir"
            [Environment]::SetEnvironmentVariable("PATH", $newPath, "User")
            $env:PATH = "$env:PATH;$installDir"
            Write-Success "Added to PATH"
        }

        Write-Host ""
        Write-Success "Installation complete!"
        Write-Host ""

        # Refresh PATH in current session
        $env:PATH = [Environment]::GetEnvironmentVariable("PATH", "Machine") + ";" + [Environment]::GetEnvironmentVariable("PATH", "User")

        # Check for ffmpeg
        if (-not (Get-Command ffmpeg -ErrorAction SilentlyContinue)) {
            Write-Warn "ffmpeg not found - tappr requires ffmpeg for audio decoding"
            Write-Host ""
            Write-Host "Install ffmpeg using one of these methods:"
            Write-Host ""
            Write-Host "  winget install ffmpeg" -ForegroundColor Cyan
            Write-Host "  choco install ffmpeg" -ForegroundColor Cyan
            Write-Host "  scoop install ffmpeg" -ForegroundColor Cyan
            Write-Host ""
            Write-Host "Or download from: https://ffmpeg.org/download.html"
            Write-Host ""
        } else {
            Write-Success "ffmpeg found at $((Get-Command ffmpeg).Source)"
        }

        # Verify tappr is accessible
        Write-Host ""
        if (Get-Command $BinaryName -ErrorAction SilentlyContinue) {
            Write-Success "You can now run '$BinaryName --help' to get started."
        } else {
            Write-Host "Run directly:" -ForegroundColor Yellow
            Write-Host ""
            Write-Host "  $destPath --help" -ForegroundColor Cyan
            Write-Host ""
            Write-Host "Or restart your terminal, then run '$BinaryName --help'" -ForegroundColor Yellow
        }
        Write-Host ""

    } finally {
        # Cleanup
        if (Test-Path $tmpDir) {
            Remove-Item -Path $tmpDir -Recurse -Force -ErrorAction SilentlyContinue
        }
    }
}

# Run installer
Install-Tappr
