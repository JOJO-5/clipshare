@echo off
set VCTools=D:\Program Files\Microsoft Visual Studio\2022\Professional\VC\Tools\MSVC\14.44.35207
set SDK=C:\Program Files (x86)\Windows Kits\10
set INCLUDE=%VCTools%\include
set LIB=%VCTools%\lib\x64
set PATH=%SDK%\bin\10.0.17134.0\x64;%PATH%
cd /d %~dp0
call %VCTools%\..\..\..\Common7\IDE\..\VC\Auxiliary\Build\vcvarsall.bat x64
npm run tauri build
