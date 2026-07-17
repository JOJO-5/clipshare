@echo off
setlocal
call "D:\Program Files\Microsoft Visual Studio\2022\Professional\VC\Auxiliary\Build\vcvars64.bat"
if errorlevel 1 exit /b %errorlevel%
set "PATH=D:\nodejs;%PATH%"
cd /d "%~dp0src-tauri"
cargo build --release
