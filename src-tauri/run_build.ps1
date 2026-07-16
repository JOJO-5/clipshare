$env:LIB = "D:\Program Files\Microsoft Visual Studio\2022\Professional\VC\Tools\MSVC\14.44.35207\lib\x64"
$env:PATH = "D:\Program Files\Microsoft Visual Studio\2022\Professional\VC\Tools\MSVC\14.44.35207\bin\HostX64\x64;" + $env:PATH
cargo build --release 2>&1
