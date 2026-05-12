@echo off
cd /d "%~dp0.."
powershell -ExecutionPolicy Bypass -File "scripts\build_rmskin.ps1"
pause
