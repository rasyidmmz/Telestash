@echo off
setlocal enabledelayedexpansion

echo ============================================================
echo  TeleStash - Debug Launcher (local build)
echo ============================================================
echo.
echo PENTING: Tutup dulu TeleStash yang terpasang (versi dari
echo Start Menu / updater), karena versi debug ini memakai
echo AppData dan port yang sama (14201).
echo.
pause
echo.

set "RUST_LOG=info,telestash=debug"
set "WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9222"
set "TELESTASH_LOG_FILE=C:\Telestash\stable-debug.log"
set "EXE_PATH=C:\Telestash\app\src-tauri\target\debug\telestash.exe"

if not exist "%EXE_PATH%" (
    echo [ERROR] Executable tidak ditemukan: %EXE_PATH%
    echo Build dulu: npm run build ^&^& cargo build --features tauri/custom-protocol
    pause
    exit /b 1
)

if not exist "C:\Telestash\app\src-tauri\target\debug\mpv.exe" (
    echo [WARNING] mpv.exe tidak ada di sebelah exe - pemutaran video akan gagal.
    pause
)

if exist "%TELESTASH_LOG_FILE%" del "%TELESTASH_LOG_FILE%"

echo Build yang dijalankan (cek timestamp ini):
for %%F in ("%EXE_PATH%") do echo   %%F  --  %%~tF
echo.
echo Remote debug port: 9222 (buka chrome://inspect jika perlu DevTools)
echo Log: %TELESTASH_LOG_FILE%
echo.

"%EXE_PATH%"

echo.
echo ============================================================
echo TeleStash ditutup. Log: %TELESTASH_LOG_FILE%
echo ============================================================
pause