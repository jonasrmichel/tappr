@echo off
REM tappr installer for Windows CMD
REM Usage: curl -fsSL https://raw.githubusercontent.com/jonasrmichel/tappr/main/scripts/install.cmd -o install.cmd && install.cmd && del install.cmd

setlocal EnableDelayedExpansion

set "REPO=jonasrmichel/tappr"
set "BINARY_NAME=tappr"

echo.
echo   tappr installer
echo   ===============
echo.

REM Detect architecture
set "ARCH=x86_64"
if "%PROCESSOR_ARCHITECTURE%"=="ARM64" set "ARCH=aarch64"
if "%PROCESSOR_ARCHITEW6432%"=="ARM64" set "ARCH=aarch64"

set "TARGET=%ARCH%-pc-windows-gnu"

echo [info] Detected architecture: %ARCH%
echo [info] Target: %TARGET%

REM Get latest version using PowerShell (available on all modern Windows)
echo [info] Fetching latest release...
for /f "delims=" %%i in ('powershell -NoProfile -Command "(Invoke-RestMethod -Uri 'https://api.github.com/repos/%REPO%/releases/latest').tag_name"') do set "VERSION=%%i"

if "%VERSION%"=="" (
    echo [error] Failed to fetch latest version
    exit /b 1
)

echo [info] Latest version: %VERSION%

REM Setup paths
set "DOWNLOAD_URL=https://github.com/%REPO%/releases/download/%VERSION%/%BINARY_NAME%-%TARGET%.zip"
set "INSTALL_DIR=%LOCALAPPDATA%\tappr"
set "TMP_DIR=%TEMP%\tappr-install-%RANDOM%"

REM Create directories
if not exist "%INSTALL_DIR%" mkdir "%INSTALL_DIR%"
if not exist "%TMP_DIR%" mkdir "%TMP_DIR%"

REM Download
echo [info] Downloading %BINARY_NAME% %VERSION%...
curl -fsSL "%DOWNLOAD_URL%" -o "%TMP_DIR%\%BINARY_NAME%.zip"
if errorlevel 1 (
    echo [error] Failed to download from: %DOWNLOAD_URL%
    rmdir /s /q "%TMP_DIR%" 2>nul
    exit /b 1
)

REM Extract using PowerShell
echo [info] Extracting...
powershell -NoProfile -Command "Expand-Archive -Path '%TMP_DIR%\%BINARY_NAME%.zip' -DestinationPath '%TMP_DIR%' -Force"
if errorlevel 1 (
    echo [error] Failed to extract archive
    rmdir /s /q "%TMP_DIR%" 2>nul
    exit /b 1
)

REM Install
echo [info] Installing to %INSTALL_DIR%...
move /y "%TMP_DIR%\%BINARY_NAME%.exe" "%INSTALL_DIR%\%BINARY_NAME%.exe" >nul
if errorlevel 1 (
    echo [error] Failed to install binary
    rmdir /s /q "%TMP_DIR%" 2>nul
    exit /b 1
)

echo [success] Installed %BINARY_NAME% %VERSION% to %INSTALL_DIR%\%BINARY_NAME%.exe

REM Check if already in PATH
echo %PATH% | findstr /i /c:"%INSTALL_DIR%" >nul
if errorlevel 1 (
    echo [info] Adding %INSTALL_DIR% to PATH...

    REM Add to user PATH using PowerShell
    powershell -NoProfile -Command "$oldPath = [Environment]::GetEnvironmentVariable('PATH', 'User'); if ($oldPath -notlike '*%INSTALL_DIR%*') { [Environment]::SetEnvironmentVariable('PATH', \"$oldPath;%INSTALL_DIR%\", 'User') }"

    REM Also add to current session
    set "PATH=%PATH%;%INSTALL_DIR%"

    echo [success] Added to PATH
)

REM Cleanup
rmdir /s /q "%TMP_DIR%" 2>nul

echo.
echo [success] Installation complete!
echo.

REM Check for ffmpeg
where ffmpeg >nul 2>&1
if errorlevel 1 (
    echo [warn] ffmpeg not found - tappr requires ffmpeg for audio decoding
    echo.
    echo Install ffmpeg using one of these methods:
    echo.
    echo   winget install ffmpeg
    echo   choco install ffmpeg
    echo   scoop install ffmpeg
    echo.
    echo Or download from: https://ffmpeg.org/download.html
    echo.
) else (
    for /f "delims=" %%i in ('where ffmpeg') do echo [success] ffmpeg found at %%i
)

echo.

REM Verify tappr is accessible in current session
where %BINARY_NAME% >nul 2>&1
if errorlevel 1 (
    echo [info] Run directly:
    echo.
    echo   "%INSTALL_DIR%\%BINARY_NAME%.exe" --help
    echo.
    echo Or open a new terminal window, then run '%BINARY_NAME% --help'
) else (
    echo [success] You can now run '%BINARY_NAME% --help' to get started.
)
echo.

endlocal
