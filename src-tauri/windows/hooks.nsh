; CloudDownloader NSIS installer hooks.
;
; aria2c.exe runs as a child of the app and keeps bin\aria2c.exe locked. Tauri's
; CheckIfAppIsRunning only stops the main CloudDownloader.exe (via the Restart
; Manager), so an install / update / uninstall would otherwise fail with
; "Error opening file for writing" on bin\aria2c.exe. Stop our engine first.
;
; The kill is scoped to $INSTDIR so a user's own aria2c running elsewhere is never
; touched, and the main app is left to Tauri's running-app handling. No /T flag:
; the installer itself is a child of the app and must not be killed.

!macro _CloudDownloaderStopEngine
  DetailPrint "Stopping CloudDownloader download engine (aria2c)..."
  nsExec::ExecToLog `powershell -NoProfile -NonInteractive -ExecutionPolicy Bypass -Command "Get-Process -Name aria2c -ErrorAction SilentlyContinue | Where-Object { $$_.Path -like '$INSTDIR\*' } | Stop-Process -Force -ErrorAction SilentlyContinue"`
  Pop $0
  Sleep 500
!macroend

!macro NSIS_HOOK_PREINSTALL
  !insertmacro _CloudDownloaderStopEngine
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  !insertmacro _CloudDownloaderStopEngine
!macroend
