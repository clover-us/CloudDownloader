; CloudDownloader NSIS installer hooks.
;
; aria2c.exe runs as a child of the app and keeps bin\aria2c.exe locked. Tauri's
; CheckIfAppIsRunning only closes the main CloudDownloader.exe (through the
; Restart Manager), and it runs after this hook, so an install / update can fail
; with "Error opening file for writing" on bin\aria2c.exe. Closing the app alone
; is not enough either: its supervisor starts a new engine straight away.
;
; So: close the app, stop the engine under $INSTDIR, clear stray file attributes,
; and wait until the file really is writable. Everything is logged to
; %TEMP%\clouddownloader-install.log so a failed install on someone else's
; machine can be explained afterwards.
;
; The engine kill is scoped to $INSTDIR: a user's own aria2c elsewhere is never
; touched. No /T on taskkill: during an auto-update this installer is a child of
; the app and must not kill itself.

!macro _CloudDownloaderLog text
  nsExec::Exec `cmd /c echo [CloudDownloader] ${text} >> "%TEMP%\clouddownloader-install.log"`
  Pop $0
!macroend

!macro _CloudDownloaderStopEngineBody
  !insertmacro _CloudDownloaderLog "install: starting engine cleanup"
  nsExec::ExecToLog `cmd /c tasklist /FI "IMAGENAME eq CloudDownloader.exe" /FO LIST >> "%TEMP%\clouddownloader-install.log" 2>&1`
  Pop $0
  nsExec::ExecToLog `cmd /c tasklist /FI "IMAGENAME eq aria2c.exe" /FO LIST >> "%TEMP%\clouddownloader-install.log" 2>&1`
  Pop $0

  DetailPrint "Closing CloudDownloader before installing..."
  nsExec::ExecToLog 'taskkill /IM CloudDownloader.exe /F'
  Pop $0
  Sleep 800

  DetailPrint "Stopping the download engine (aria2c) in $INSTDIR..."
  nsExec::ExecToLog `powershell -NoProfile -NonInteractive -ExecutionPolicy Bypass -Command "Get-Process -Name aria2c -ErrorAction SilentlyContinue | Where-Object { $$_.Path -like '$INSTDIR\*' } | Stop-Process -Force -ErrorAction SilentlyContinue"`
  Pop $0
  Sleep 800

  ; A read-only bit (some AV tools set it) looks exactly like a lock to the
  ; extractor, so clear it before deciding the file is still in use.
  SetFileAttributes "$INSTDIR\bin\aria2c.exe" NORMAL
!macroend

Function CloudDownloaderWaitForEngine
  StrCpy $1 0
  IfFileExists "$INSTDIR\bin\aria2c.exe" 0 wait_done
  wait_loop:
    ClearErrors
    FileOpen $0 "$INSTDIR\bin\aria2c.exe" a
    IfErrors 0 wait_ok
    IntOp $1 $1 + 1
    IntCmp $1 10 wait_give_up
    Sleep 500
    Goto wait_loop
  wait_ok:
    FileClose $0
    !insertmacro _CloudDownloaderLog "install: aria2c.exe is writable after $1 tries"
    Goto wait_done
  wait_give_up:
    !insertmacro _CloudDownloaderLog "install: aria2c.exe is STILL locked after $1 tries"
    MessageBox MB_RETRYCANCEL|MB_ICONEXCLAMATION \
      "无法写入 $\"$INSTDIR\bin\aria2c.exe$\"。$\r$\n$\r$\n请先在托盘退出 CloudDownloader（只关窗口不够），必要时在任务管理器结束 aria2c.exe，然后点「重试」。" \
      IDRETRY wait_loop
    !insertmacro _CloudDownloaderLog "install: aborted by the user"
    Abort
  wait_done:
    Sleep 200
FunctionEnd

Function CloudDownloaderStopEngine
  !insertmacro _CloudDownloaderStopEngineBody
  Call CloudDownloaderWaitForEngine
FunctionEnd

Function un.CloudDownloaderStopEngine
  !insertmacro _CloudDownloaderStopEngineBody
  Call un.CloudDownloaderWaitForEngine
FunctionEnd

Function un.CloudDownloaderWaitForEngine
  StrCpy $1 0
  IfFileExists "$INSTDIR\bin\aria2c.exe" 0 un_wait_done
  un_wait_loop:
    ClearErrors
    FileOpen $0 "$INSTDIR\bin\aria2c.exe" a
    IfErrors 0 un_wait_ok
    IntOp $1 $1 + 1
    IntCmp $1 10 un_wait_done
    Sleep 500
    Goto un_wait_loop
  un_wait_ok:
    FileClose $0
  un_wait_done:
    Sleep 200
FunctionEnd

!macro NSIS_HOOK_PREINSTALL
  Call CloudDownloaderStopEngine
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  Call un.CloudDownloaderStopEngine
  ; The app registers clouddownloader:// in HKCU on every start; leaving the key
  ; behind would point Explorer at a removed executable.
  DeleteRegKey HKCU "Software\Classes\clouddownloader"
!macroend