@echo off
setlocal
rem Always run from this script's folder (the project root)
cd /d "%~dp0"

echo ===========================================
echo   Building Ncy  (release, NSIS installer)
echo ===========================================
echo.

rem ---- Updater signing -------------------------------------------------------
rem createUpdaterArtifacts is on, so the build needs the private signing key.
rem The key is read from %USERPROFILE%\.ncy\ncy-updater.key (generate it with:
rem   npx tauri signer generate -w "%USERPROFILE%\.ncy\ncy-updater.key" ).
set "NCY_KEY=%USERPROFILE%\.ncy\ncy-updater.key"
if not exist "%NCY_KEY%" (
    echo ERROR: signing key not found at "%NCY_KEY%".
    echo Generate it once with:
    echo     npx tauri signer generate -w "%USERPROFILE%\.ncy\ncy-updater.key"
    echo.
    pause
    exit /b 1
)
set "TAURI_SIGNING_PRIVATE_KEY=%NCY_KEY%"
rem Prompt for the key password each run, masked (nothing is echoed), so it is
rem never stored in this file or shown on screen. Just press Enter if the key
rem has no password. An empty entry leaves the variable blank.
set "TAURI_SIGNING_PRIVATE_KEY_PASSWORD="
for /f "usebackq delims=" %%p in (`powershell -NoProfile -Command "$s = Read-Host 'Signing key password, blank if none' -AsSecureString; $b = [Runtime.InteropServices.Marshal]::SecureStringToBSTR($s); try { [Runtime.InteropServices.Marshal]::PtrToStringBSTR($b) } finally { [Runtime.InteropServices.Marshal]::ZeroFreeBSTR($b) }"`) do set "TAURI_SIGNING_PRIVATE_KEY_PASSWORD=%%p"
echo.

rem Tauri's build script caches the exe/window icon and misses icon-only
rem changes, so bump tauri.conf.json's timestamp to force the icon to re-embed.
powershell -NoProfile -Command "(Get-Item 'src-tauri\tauri.conf.json').LastWriteTime = Get-Date" >nul 2>&1

rem Rebuild the packaged Windows x64 Pear worker before Tauri collects its
rem runtime, bundle, and native addons as release resources.
call npm run pear:build
if not "%ERRORLEVEL%"=="0" (
    echo PEAR WORKER BUILD FAILED
    echo Fully exit Ncy Dev, including its tray process, before rebuilding.
    echo If dependencies are missing, run npm ci in ..\ncy-core\pear-sidecar.
    pause
    exit /b 1
)

call npm run tauri build -- --bundles nsis
set "ERR=%ERRORLEVEL%"

echo.
if not "%ERR%"=="0" (
    echo BUILD FAILED  [exit code %ERR%]
    echo.
    pause
    exit /b %ERR%
)

echo BUILD OK.
echo.
echo   Installer:   target\release\bundle\nsis\
echo   Signature:   target\release\bundle\nsis\*-setup.exe.sig
echo   Standalone:  target\release\ncy-desktop.exe
echo.
echo   To publish an update: run publish-release.bat, then upload the staged
echo   release\ folder to https://0x89-y.xyz/ncy/ .
echo.

rem Open the installer folder in Explorer
start "" "target\release"

pause
