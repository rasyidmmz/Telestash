@echo off
setlocal enabledelayedexpansion

echo ========================================================
echo  TeleStash Stable v1.6.0 - Live Debug Launcher
echo ========================================================

set "RUST_LOG=info,telestash=debug"
set "WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9222"
set "LOG_FILE=C:\Telestash\stable-debug.log"

set "EXE_PATH=C:\Program Files\TeleStash\telestash.exe"
if not exist "%EXE_PATH%" (
    if exist "%LOCALAPPDATA%\Programs\TeleStash\telestash.exe" (
        set "EXE_PATH=%LOCALAPPDATA%\Programs\TeleStash\telestash.exe"
    ) else if exist "%LOCALAPPDATA%\TeleStash\telestash.exe" (
        set "EXE_PATH=%LOCALAPPDATA%\TeleStash\telestash.exe"
    ) else if exist "C:\Telestash\app\src-tauri\target\release\telestash.exe" (
        set "EXE_PATH=C:\Telestash\app\src-tauri\target\release\telestash.exe"
    )
)

if not exist "%EXE_PATH%" (
    echo [ERROR] TeleStash executable not found!
    echo Looked for: "%EXE_PATH%"
    pause
    exit /b 1
)

echo [1/3] Environment: RUST_LOG=%RUST_LOG%
echo [2/3] Remote Debug Port: 9222
echo [3/3] Target Binary: %EXE_PATH%
echo Logging output to: %LOG_FILE%
echo.
echo Starting TeleStash... (Console will keep running while TeleStash is active)

powershell -NoProfile -Command "$env:RUST_LOG = 'info,telestash=debug'; $env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS = '--remote-debugging-port=9222'; Write-Host '--- TeleStash Debug Session Started ---' ; & '%EXE_PATH%' --remote-debugging-port=9222 *>&1 | Tee-Object -FilePath '%LOG_FILE%'"

echo.
echo TeleStash has closed. Log saved to: %LOG_FILE%
pause
