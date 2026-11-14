@echo off
setlocal
rem Always run from this script's folder (the project root)
cd /d "%~dp0"

echo ===========================================
echo   Stage Ncy release  (installer + latest.json)
echo ===========================================
echo.
echo Manual fallback for the GitHub "Release" workflow, which normally does this.
echo Takes the NSIS installer that build.bat just produced, renames it to the
echo stable asset name, and writes latest.json with the matching signature
echo already pasted in. Run build.bat first.
echo.

rem ---- Version: package.json, Cargo.toml and tauri.conf.json must agree ------
set "VER="
for /f "usebackq delims=" %%v in (`node scripts\release.mjs version`) do set "VER=%%v"
if not defined VER (
    echo ERROR: could not read a consistent version, see the message above.
    echo.
    pause
    exit /b 1
)
echo Version: %VER%
echo.

set "BUNDLE=target\release\bundle\nsis"
set "SETUP=%BUNDLE%\Ncy_%VER%_x64-setup.exe"
set "SIG=%SETUP%.sig"

if not exist "%SETUP%" (
    echo ERROR: installer not found at "%SETUP%".
    echo Run build.bat first, and check the version matches.
    echo.
    pause
    exit /b 1
)
if not exist "%SIG%" (
    echo ERROR: signature not found at "%SIG%".
    echo createUpdaterArtifacts must be on and the build must have been signed.
    echo.
    pause
    exit /b 1
)

rem ---- Release notes: one line, shown in the updater's latest.json -----------
set "NOTES="
set /p "NOTES=Release notes (one line, blank for a default): "

if not exist "release" mkdir "release"
copy /y "%SETUP%" "release\Ncy-windows-x64-setup.exe" >nul

node scripts\release.mjs manifest "%SIG%" "release\latest.json" "%NOTES%"
if not "%ERRORLEVEL%"=="0" (
    echo.
    echo FAILED to write release\latest.json
    echo.
    pause
    exit /b 1
)
node scripts\release.mjs notes "release\notes.md" >nul

echo.
echo STAGED OK  (v%VER%).
echo.
echo   release\Ncy-windows-x64-setup.exe
echo   release\latest.json
echo   release\notes.md   (release body, from CHANGELOG.md)
echo.
echo   Next: create the GitHub release v%VER% on 0x89-y/ncy-desktop and attach
echo   BOTH files under exactly these names. latest.json points at
echo   releases/download/v%VER%/Ncy-windows-x64-setup.exe, so the tag must match.
echo   Existing installs pick up the update on their next check (startup, then
echo   every 3 hours), or immediately via Settings ^> About ^> Check now.
echo.

start "" "release"

pause
