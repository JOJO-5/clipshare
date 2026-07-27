# webview2-com-sys
This crate implements unsafe Rust bindings for the [WebView2](https://aka.ms/webview2) COM APIs using the [Windows](https://github.com/microsoft/windows-rs) crate.

ClipShare vendors the 0.38.2 generated bindings and replaces only the x64 static
loader with `WebView2LoaderStatic.lib` from the official Microsoft WebView2 SDK
1.0.1054.31. That loader does not statically import `EventSetInformation`, which
is absent on some Windows 7 installations. Its SHA-256 is
`76314119685BBF4C2B2423A44E81B57BEADC914C943D0E772FD6BC78C8E6B0E8`.
`WEBVIEW2_LOADER_LICENSE.txt` contains Microsoft's redistribution terms.

## Getting Started
This crate has a friendlier wrapper in [webview2-com](https://crates.io/crates/webview2-com).
