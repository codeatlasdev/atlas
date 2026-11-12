; NSIS hooks of Atlas Desktop's Windows installer (tauri.windows.conf.json `installerHooks`).

; The installer carries `atlas.exe` next to the app. Someone who installs only the app (no
; install.ps1) had no `atlas` command in a terminal and nothing for `atlas update` to update.
; First install: let that `atlas.exe` install itself the way install.ps1 does (`atlas install`:
; %USERPROFILE%\.atlas\bin, user PATH, bundled skills). Never over an existing CLI: it may be a
; newer one or a linked dev build, and `atlas update` keeps it in step with the app.
;
; Settings > Apps: Atlas-Setup.exe (crates/atlas-setup) copies itself into $INSTDIR as
; "Atlas Uninstall.exe" after this installer ran, and its `--uninstall` window is what "Uninstall"
; should open. Tauri's installer rewrites the uninstall entry on every run (an `atlas update` runs
; it silently, without Atlas-Setup.exe), so when that copy is there this hook points the entry back
; at it: UninstallString and DisplayIcon use the branded window, QuietUninstallString keeps the
; plain NSIS uninstaller for scripts (winget, Intune) that need `/S`.
!macro NSIS_HOOK_POSTINSTALL
  IfFileExists "$PROFILE\.atlas\bin\atlas.exe" atlas_cli_done 0
  IfFileExists "$INSTDIR\atlas.exe" 0 atlas_cli_done
  nsExec::Exec '"$INSTDIR\atlas.exe" install'
  Pop $0
  atlas_cli_done:

  WriteRegStr SHCTX "${UNINSTKEY}" "QuietUninstallString" "$\"$INSTDIR\uninstall.exe$\" /S"
  IfFileExists "$INSTDIR\Atlas Uninstall.exe" 0 atlas_uninstall_done
  WriteRegStr SHCTX "${UNINSTKEY}" "UninstallString" "$\"$INSTDIR\Atlas Uninstall.exe$\" --uninstall"
  WriteRegStr SHCTX "${UNINSTKEY}" "DisplayIcon" "$\"$INSTDIR\Atlas Uninstall.exe$\""
  atlas_uninstall_done:
!macroend

; Atlas-Setup.exe's own copy goes with the app. (`uninstall.exe` itself is deleted by the
; uninstaller; the copy is ours, and Atlas-Setup.exe removes it after the uninstall too, since a
; running program cannot delete itself.)
!macro NSIS_HOOK_POSTUNINSTALL
  Delete "$INSTDIR\Atlas Uninstall.exe"
!macroend
