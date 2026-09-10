@echo off
setlocal enabledelayedexpansion

echo ============================================================
echo  TeleStash - Thumbnail Diagnostics Run
echo ============================================================
echo.
echo PENTING: Tutup dulu TeleStash yang terpasang (versi dari
echo Start Menu / updater), karena versi debug ini memakai
echo AppData dan port yang sama (14201). Kalau dua-duanya jalan,
echo streaming server bentrok dan session bisa rusak.
echo.
pause
echo.

set "RUST_LOG=info,telestash=debug"
set "WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9222"
set "TELESTASH_LOG_FILE=C:\Telestash\stable-debug.log"
set "EXE_PATH=C:\Telestash\app\src-tauri\target\debug\telestash.exe"

if not exist "%EXE_PATH%" (
    echo [ERROR] Executable tidak ditemukan: %EXE_PATH%
    pause
    exit /b 1
)

if not exist "C:\Telestash\app\src-tauri\target\debug\mpv.exe" (
    echo [WARNING] mpv.exe tidak ada di sebelah exe - generate thumbnail pasti gagal.
    pause
)

if exist "%TELESTASH_LOG_FILE%" del "%TELESTASH_LOG_FILE%"

echo Menjalankan: %EXE_PATH%
echo Remote debug port: 9222 (buka chrome://inspect jika perlu DevTools)
echo Log: %TELESTASH_LOG_FILE%
echo.
echo LANGKAH YANG DIMINTA:
echo   1. Login seperti biasa.
echo   2. Buka folder yang berisi VIDEO (yang thumbnail-nya kosong).
echo   3. Tunggu 20-30 detik - biar antrean generate jalan.
echo   4. Kalau perlu, scroll sedikit supaya card masuk viewport.
echo   5. Tutup aplikasi lewat tray -^> Exit.
echo.

"%EXE_PATH%"

echo.
echo ============================================================
echo TeleStash ditutup. Log: %TELESTASH_LOG_FILE%
echo.
echo Baris diagnostik penting:
echo ------------------------------------------------------------
findstr /I "thumb thumbgen thumb-serve MPV frame gen server streaming" "%TELESTASH_LOG_FILE%" 2>nul
echo ------------------------------------------------------------
echo.
echo Kirim file log lengkap ke asisten.
pause