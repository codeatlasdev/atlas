; NSIS hooks of Atlas Desktop's Windows installer (tauri.windows.conf.json `installerHooks`).

; The installer carries `atlas.exe` next to the app. Someone who installs only the app (no
; install.ps1) had no `atlas` command in a terminal and nothing for `atlas update` to update.
; First install: let that `atlas.exe` install itself the way install.ps1 does (`atlas install`:
; %USERPROFILE%\.atlas\bin, user PATH, bundled skills). Never over an existing CLI: it may be a
; newer one or a linked dev build. "Existing" means an `atlas.exe` in %USERPROFILE%\.atlas\bin or
; anywhere on PATH (a cargo install in %USERPROFILE%\.cargo\bin, a scoop shim): a second copy in
; %USERPROFILE%\.atlas\bin would shadow it.
;
; An existing %USERPROFILE%\.atlas\bin\atlas.exe is the one case that is brought up: `atlas install
; --upgrade` replaces it only when it is older than the bundled one (or unreadable), never a newer
; one and never a dev-build link. Without this a CLI from an older build stayed behind forever (the
; PATH `atlas` and the terminal host it starts ran the old version next to the new app) unless
; `atlas update` happened to replace it first.
;
; Settings > Apps: Atlas-Setup.exe (crates/atlas-setup) copies itself into $INSTDIR as
; "Atlas Uninstall.exe" after this installer ran, and its `--uninstall` window is what "Uninstall"
; should open. Tauri's installer rewrites the uninstall entry on every run (an `atlas update` runs
; it silently, without Atlas-Setup.exe; `atlas update` leaves the copy in place), so when that copy
; is there this hook points the entry back at it: UninstallString and DisplayIcon use the branded window, QuietUninstallString keeps the
; plain NSIS uninstaller for scripts (winget, Intune) that need `/S`.
!macro NSIS_HOOK_POSTINSTALL
  IfFileExists "$PROFILE\.atlas\bin\atlas.exe" 0 atlas_cli_first
  IfFileExists "$INSTDIR\atlas.exe" 0 atlas_cli_done
  nsExec::Exec '"$INSTDIR\atlas.exe" install --upgrade'
  Pop $0
  Goto atlas_cli_done
  atlas_cli_first:
  SearchPath $1 "atlas.exe"
  StrCmp $1 "" 0 atlas_cli_done
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

; Atlas-Setup.exe's own copy goes with the app. When the uninstall was started from that copy
; (Settings > Apps) this Delete fails because the image is running; Atlas-Setup.exe then schedules
; the deletion of the copy and the folder for after it exits (a running program cannot delete
; itself).
!macro NSIS_HOOK_POSTUNINSTALL
  Delete "$INSTDIR\Atlas Uninstall.exe"
!macroend
