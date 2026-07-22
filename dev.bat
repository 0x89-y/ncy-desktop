@echo off
setlocal
cd /d "%~dp0"

echo ===========================================
echo   Starting Ncy Desktop development mode
echo ===========================================
echo.

call npm run tauri dev
set "ERR=%ERRORLEVEL%"

echo.
if not "%ERR%"=="0" echo Ncy Desktop development mode stopped with exit code %ERR%.
if "%ERR%"=="0" echo Ncy Desktop development mode stopped.
pause
exit /b %ERR%
