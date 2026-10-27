; NSIS hooks of Atlas Desktop's Windows installer (tauri.windows.conf.json `installerHooks`).

; The installer carries `atlas.exe` next to the app. Someone who installs only the app (no
; install.ps1) had no `atlas` command in a terminal and nothing for `atlas update` to update.
; First install: let that `atlas.exe` install itself the way install.ps1 does (`atlas install`:
; %USERPROFILE%\.atlas\bin, user PATH, bundled skills). Never over an existing CLI: it may be a
; newer one or a linked dev build, and `atlas update` keeps it in step with the app.
!macro NSIS_HOOK_POSTINSTALL
  IfFileExists "$PROFILE\.atlas\bin\atlas.exe" atlas_cli_done 0
  IfFileExists "$INSTDIR\atlas.exe" 0 atlas_cli_done
  nsExec::Exec '"$INSTDIR\atlas.exe" install'
  Pop $0
  atlas_cli_done:
!macroend
