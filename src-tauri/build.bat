@echo off
set LIB=D:\Program Files\Microsoft Visual Studio\2022\Professional\VC\Tools\MSVC\14.29.30133\lib\x64
set PATH=D:\Program Files\Microsoft Visual Studio\2022\Professional\VC\Tools\MSVC\14.29.30133\bin\HostX64\x64;%PATH%
cargo build --release 2>&1
