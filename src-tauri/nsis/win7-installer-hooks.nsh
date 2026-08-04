; Prevent an upgrade from overwriting ClipShare's fixed WebView2 runtime while
; the previous app instance (or one of its child processes) still holds it.
!macro NSIS_HOOK_PREINSTALL
  ; Tauri's own check shows a localized prompt and can close the old app.
  !insertmacro CheckIfAppIsRunning "clipshare.exe" "ClipShare"

win7_runtime_check:
  IfFileExists "$INSTDIR\webview2-runtime\msedge_elf.dll" 0 win7_runtime_available

  ; Opening the exact runtime DLL for writing is a scoped lock check. Do not
  ; terminate every msedgewebview2.exe because other applications may use it.
  ClearErrors
  FileOpen $0 "$INSTDIR\webview2-runtime\msedge_elf.dll" a
  IfErrors win7_runtime_locked win7_runtime_available_open

win7_runtime_available_open:
  FileClose $0
  Goto win7_runtime_available

win7_runtime_locked:
  MessageBox MB_RETRYCANCEL|MB_ICONEXCLAMATION \
    "ClipShare or its WebView2 background process is still running.$\r$\n$\r$\nPlease exit ClipShare and end the ClipShare-related msedgewebview2.exe process in Task Manager, then click Retry.$\r$\n$\r$\nClipShare 或其 WebView2 后台进程仍在运行。请退出 ClipShare，并在任务管理器中结束 ClipShare 相关的 msedgewebview2.exe，然后点击“重试”。" \
    IDRETRY win7_runtime_check IDCANCEL win7_runtime_abort

win7_runtime_abort:
  Abort "ClipShare installation was cancelled because its WebView2 runtime is still in use."

win7_runtime_available:
!macroend
