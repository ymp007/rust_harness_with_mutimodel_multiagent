@echo off
pushd "%~dp0"
cargo build --release -p y-cli --bin ycode --locked
if errorlevel 1 (
    popd
    exit /b 1
)
popd
set "PATH=%~dp0target\release;%PATH%"
echo Run ycode in your project. Set OPENROUTER_API_KEY first.
ycode --version
