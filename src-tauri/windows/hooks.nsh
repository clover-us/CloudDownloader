; CloudDownloader NSIS installer hooks.
;
; aria2c.exe runs as a child of the app and keeps bin\aria2c.exe locked. Tauri's
; CheckIfAppIsRunning only closes the main CloudDownloader.exe (through the
; Restart Manager), so an install / update / uninstall can still fail with
; "Error opening file for writing" on bin\aria2c.exe -- and killing the engine
; while the app is alive is not enough, because the app's supervisor starts a
; new one straight away. Close the app first, then the engine, then wait until
; the file is actually writable.
;
; The engine kill is scoped to $INSTDIR so a user's own aria2c running elsewhere
; is never touched. No /T on taskkill: during an auto-update this installer is a
; child of the app and must not kill itself.

Function CloudDownloaderStopEngine
  DetailPrint "Closing CloudDownloader before installing..."
  nsExec::ExecToLog 'taskkill /IM CloudDownloader.exe /F'
  Pop $0
  Sleep 800
  DetailPrint "Stopping the download engine (aria2c) in $INSTDIR..."
  nsExec::ExecToLog `powershell -NoProfile -NonInteractive -ExecutionPolicy Bypass -Command "Get-Process -Name aria2c -ErrorAction SilentlyContinue | Where-Object { $$_.Path -like '$INSTDIR\*' } | Stop-Process -Force -ErrorAction SilentlyContinue"`
  Pop $0
  Sleep 800
  ; A scanner or a slow shutdown can hold the file for a moment longer; wait
  ; until opening it for append succeeds instead of failing the whole install.
  StrCpy $1 0
  IfFileExists "$INSTDIR\bin\aria2c.exe" 0 done
  wait:
    ClearErrors
    FileOpen $0 "$INSTDIR\bin\aria2c.exe" a
    IfErrors 0 done
    IntOp $1 $1 + 1
    IntCmp $1 10 done
    Sleep 500
    Goto wait
  done:
    Sleep 200
FunctionEnd

!macro NSIS_HOOK_PREINSTALL
  Call CloudDownloaderStopEngine
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  Call CloudDownloaderStopEngine
  ; The app registers clouddownloader:// in HKCU on every start; leaving the key
  ; behind would point Explorer at a removed executable.
  DeleteRegKey HKCU "Software\Classes\clouddownloader"
!macroend