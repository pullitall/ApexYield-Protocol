@echo off
setlocal enabledelayedexpansion

echo =================================================================
echo   APEXYIELD: Execute Live Devnet Prototype Engine
echo =================================================================
echo.

python "%~dp0scripts\live_devnet_demo.py"

echo.
pause
