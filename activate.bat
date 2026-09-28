@echo off
setlocal
echo =========================================================================
echo             Y CODE - ULTRA-LOW-OVERHEAD AGENT HARNESS
echo =========================================================================

REM Add Cargo to PATH if needed
where cargo >nul 2>nul
if %errorlevel% neq 0 (
    set "PATH=%USERPROFILE%\.cargo\bin;%PATH%"
)

set DAEMON_EXE=%~dp0target\release\y-daemon.exe
set CLI_EXE=%~dp0target\release\y-cli.exe

if not exist "%DAEMON_EXE%" (
    echo Building release binaries...
    cargo build --release --bin y-daemon --bin y-cli
)

REM Check if y-daemon is already running
tasklist /fi "imagename eq y-daemon.exe" | findstr /i "y-daemon.exe" >nul
if %errorlevel% neq 0 (
    echo Launching resident background daemon...
    start "" /b "%DAEMON_EXE%"
    timeout /t 1 /nobreak >nul
) else (
    echo Resident y-daemon is already running.
)

echo.
echo Launching UI Dashboard at http://127.0.0.1:4040/ ...
start http://127.0.0.1:4040/

echo.
echo [Y CODE READY] You can now dispatch turns:
echo    "%CLI_EXE%" "your prompt here"
echo.
