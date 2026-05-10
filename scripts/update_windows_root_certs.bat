@echo off
title DeepSeek Balance Monitor - Update Windows Root Certificates

echo =====================================================
echo   DeepSeek Balance Monitor - Root Certificate Update
echo =====================================================
echo.
echo This script updates the Windows Trusted Root store by
echo downloading Microsoft's current root certificate list.
echo No certificate is bundled with this script.
echo.
echo For Windows 7/8.1 compatibility, this batch file uses
echo ASCII-only output. UTF-8 Chinese text may break old cmd.exe.
echo.

fltmc >nul 2>&1
if %errorlevel% neq 0 (
    echo [ERROR] Administrator permission is required.
    echo         Right-click this file and choose "Run as administrator".
    pause
    exit /b 1
)

certutil -? >nul 2>&1
if %errorlevel% neq 0 (
    echo [ERROR] certutil.exe was not found on this system.
    pause
    exit /b 1
)

powershell -NoProfile -ExecutionPolicy Bypass -Command "$PSVersionTable.PSVersion.Major" >nul 2>&1
if %errorlevel% neq 0 (
    echo [ERROR] Windows PowerShell was not found or cannot run.
    echo         Windows 7/8.1 normally includes PowerShell.
    pause
    exit /b 1
)

set "ROOTS_FILE=%TEMP%\dsmon_roots_%RANDOM%.sst"
set "BACKUP_FILE=%TEMP%\dsmon_roots_backup_%RANDOM%.txt"

echo [*] Creating a snapshot of the current root store...
powershell -NoProfile -ExecutionPolicy Bypass -Command "$ErrorActionPreference = 'Stop'; Get-ChildItem -Path Cert:\LocalMachine\Root | ForEach-Object { $_.Thumbprint } | Set-Content -Encoding ASCII '%BACKUP_FILE%'" >nul 2>&1
if %errorlevel% neq 0 (
    echo [ERROR] Failed to create a root store snapshot.
    echo         No certificate has been changed.
    call :cleanup_files
    pause
    exit /b 1
)
echo [OK] Snapshot created.
echo.

echo [*] Clearing local certificate URL cache...
certutil -urlcache * delete >nul 2>&1
echo [OK] Cache cleanup finished.
echo.

echo [*] Downloading root certificates from Windows Update...
certutil -generateSSTFromWU "%ROOTS_FILE%"
if %errorlevel% neq 0 (
    echo.
    echo [ERROR] Failed to download root certificates.
    echo         No certificate was imported, so the system root store is unchanged.
    echo.
    echo Possible causes:
    echo   - Windows 7/8.1 is missing TLS 1.2 or Windows Update patches.
    echo   - Windows Update is unavailable.
    echo   - Network, proxy, firewall, or security software blocked the request.
    echo   - Cryptographic Services is stopped or broken.
    echo.
    echo Recommended checks:
    echo   - Enable TLS 1.2 in Internet Options.
    echo   - Start Cryptographic Services.
    echo   - Make sure Windows Update can connect.
    call :cleanup_files
    pause
    exit /b 1
)
echo.

echo [*] Importing root certificates into the system root store...
certutil -addstore -f root "%ROOTS_FILE%"
if %errorlevel% neq 0 (
    echo.
    echo [ERROR] Failed to import root certificates.
    echo         Restoring the root store from the pre-import snapshot...
    call :restore_roots
    call :cleanup_files
    pause
    exit /b 1
)

call :cleanup_files

echo.
echo =====================================================
echo   Root certificate update completed.
echo   Restart Windows, then start dsmon again.
echo =====================================================
pause
exit /b 0

:restore_roots
powershell -NoProfile -ExecutionPolicy Bypass -Command "$ErrorActionPreference = 'Stop'; $before = @{}; Get-Content '%BACKUP_FILE%' | ForEach-Object { $t = $_.Trim().ToUpperInvariant(); if ($t) { $before[$t] = $true } }; Get-ChildItem -Path Cert:\LocalMachine\Root | Where-Object { $_.Thumbprint -and -not $before.ContainsKey($_.Thumbprint.ToUpperInvariant()) } | ForEach-Object { Remove-Item -LiteralPath $_.PSPath -Force }" >nul 2>&1
if %errorlevel% neq 0 (
    echo [WARNING] Automatic restore failed.
    echo           Please inspect the system root store manually.
) else (
    echo [OK] Root store restored to the pre-import snapshot.
)
exit /b 0

:cleanup_files
if exist "%ROOTS_FILE%" del /f /q "%ROOTS_FILE%" >nul 2>&1
if exist "%BACKUP_FILE%" del /f /q "%BACKUP_FILE%" >nul 2>&1
exit /b 0
