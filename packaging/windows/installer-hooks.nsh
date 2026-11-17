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
; "Open with Atlas" for text and source files, and for folders (#499). Per user (HKCU\Software\Classes, whatever the
; install mode), and never the default: an extension only gets an `OpenWithProgids` value, so Atlas shows up in
; Explorer's Open with list while the user's own editor stays the default. No extension key is created and no default
; value is written (Tauri's `fileAssociations` would do exactly that, so tauri.windows.conf.json leaves it empty). The
; uninstaller removes every key and value written here.
;
; The extension list is one macro called with the macro to run on each, so install and uninstall cannot drift apart.
!macro ATLAS_EACH_EXT M
  !insertmacro ${M} ".md"
  !insertmacro ${M} ".markdown"
  !insertmacro ${M} ".txt"
  !insertmacro ${M} ".text"
  !insertmacro ${M} ".log"
  !insertmacro ${M} ".json"
  !insertmacro ${M} ".jsonc"
  !insertmacro ${M} ".json5"
  !insertmacro ${M} ".yaml"
  !insertmacro ${M} ".yml"
  !insertmacro ${M} ".toml"
  !insertmacro ${M} ".ini"
  !insertmacro ${M} ".cfg"
  !insertmacro ${M} ".conf"
  !insertmacro ${M} ".env"
  !insertmacro ${M} ".xml"
  !insertmacro ${M} ".html"
  !insertmacro ${M} ".htm"
  !insertmacro ${M} ".css"
  !insertmacro ${M} ".scss"
  !insertmacro ${M} ".less"
  !insertmacro ${M} ".js"
  !insertmacro ${M} ".mjs"
  !insertmacro ${M} ".cjs"
  !insertmacro ${M} ".jsx"
  !insertmacro ${M} ".ts"
  !insertmacro ${M} ".tsx"
  !insertmacro ${M} ".vue"
  !insertmacro ${M} ".svelte"
  !insertmacro ${M} ".py"
  !insertmacro ${M} ".rb"
  !insertmacro ${M} ".go"
  !insertmacro ${M} ".rs"
  !insertmacro ${M} ".java"
  !insertmacro ${M} ".kt"
  !insertmacro ${M} ".swift"
  !insertmacro ${M} ".c"
  !insertmacro ${M} ".h"
  !insertmacro ${M} ".cpp"
  !insertmacro ${M} ".hpp"
  !insertmacro ${M} ".cs"
  !insertmacro ${M} ".php"
  !insertmacro ${M} ".sh"
  !insertmacro ${M} ".bash"
  !insertmacro ${M} ".zsh"
  !insertmacro ${M} ".ps1"
  !insertmacro ${M} ".bat"
  !insertmacro ${M} ".cmd"
  !insertmacro ${M} ".sql"
  !insertmacro ${M} ".lua"
  !insertmacro ${M} ".dart"
  !insertmacro ${M} ".zig"
  !insertmacro ${M} ".tf"
  !insertmacro ${M} ".csv"
  !insertmacro ${M} ".diff"
  !insertmacro ${M} ".patch"
  !insertmacro ${M} ".gitignore"
  !insertmacro ${M} ".editorconfig"
  !insertmacro ${M} ".tex"
  !insertmacro ${M} ".rst"
!macroend

!macro ATLAS_EXT_REGISTER EXT
  WriteRegStr HKCU "Software\Classes\Applications\${MAINBINARYNAME}.exe\SupportedTypes" "${EXT}" ""
  WriteRegStr HKCU "Software\Classes\${EXT}\OpenWithProgids" "AtlasDesktop.File" ""
!macroend

!macro ATLAS_EXT_UNREGISTER EXT
  DeleteRegValue HKCU "Software\Classes\${EXT}\OpenWithProgids" "AtlasDesktop.File"
!macroend

!macro ATLAS_OPENWITH_REGISTER
  WriteRegStr HKCU "Software\Classes\Applications\${MAINBINARYNAME}.exe" "FriendlyAppName" "Atlas"
  WriteRegStr HKCU "Software\Classes\Applications\${MAINBINARYNAME}.exe\shell\open\command" "" "$\"$INSTDIR\${MAINBINARYNAME}.exe$\" $\"%1$\""
  WriteRegStr HKCU "Software\Classes\AtlasDesktop.File" "" "Atlas document"
  WriteRegStr HKCU "Software\Classes\AtlasDesktop.File\DefaultIcon" "" "$\"$INSTDIR\${MAINBINARYNAME}.exe$\",0"
  WriteRegStr HKCU "Software\Classes\AtlasDesktop.File\shell\open\command" "" "$\"$INSTDIR\${MAINBINARYNAME}.exe$\" $\"%1$\""
  !insertmacro ATLAS_EACH_EXT ATLAS_EXT_REGISTER
  WriteRegStr HKCU "Software\Classes\Directory\shell\AtlasDesktop" "" "Open with Atlas"
  WriteRegStr HKCU "Software\Classes\Directory\shell\AtlasDesktop" "Icon" "$\"$INSTDIR\${MAINBINARYNAME}.exe$\""
  WriteRegStr HKCU "Software\Classes\Directory\shell\AtlasDesktop\command" "" "$\"$INSTDIR\${MAINBINARYNAME}.exe$\" $\"%1$\""
  WriteRegStr HKCU "Software\Classes\Directory\Background\shell\AtlasDesktop" "" "Open with Atlas"
  WriteRegStr HKCU "Software\Classes\Directory\Background\shell\AtlasDesktop" "Icon" "$\"$INSTDIR\${MAINBINARYNAME}.exe$\""
  WriteRegStr HKCU "Software\Classes\Directory\Background\shell\AtlasDesktop\command" "" "$\"$INSTDIR\${MAINBINARYNAME}.exe$\" $\"%V$\""
  ; Tell Explorer the associations changed (SHCNE_ASSOCCHANGED).
  System::Call 'shell32::SHChangeNotify(i 0x08000000, i 0, p 0, p 0)'
!macroend

!macro ATLAS_OPENWITH_UNREGISTER
  !insertmacro ATLAS_EACH_EXT ATLAS_EXT_UNREGISTER
  DeleteRegKey HKCU "Software\Classes\Applications\${MAINBINARYNAME}.exe"
  DeleteRegKey HKCU "Software\Classes\AtlasDesktop.File"
  DeleteRegKey HKCU "Software\Classes\Directory\shell\AtlasDesktop"
  DeleteRegKey HKCU "Software\Classes\Directory\Background\shell\AtlasDesktop"
  System::Call 'shell32::SHChangeNotify(i 0x08000000, i 0, p 0, p 0)'
!macroend

!macro NSIS_HOOK_POSTINSTALL
  IfFileExists "$PROFILE\.atlas\bin\atlas.exe" 0 atlas_cli_first
  IfFileExists "$INSTDIR\atlas.exe" 0 atlas_cli_done
  nsExec::Exec '"$INSTDIR\atlas.exe" install --upgrade'
  Pop $0
  Goto atlas_cli_done
  atlas_cli_first:
  ; SearchPath also looks in the current directory, which is $INSTDIR here: its own atlas.exe is not
  ; an "existing CLI" (a clean install found it and never installed the command; Windows run, 2026-11-11).
  SearchPath $1 "atlas.exe"
  StrCmp $1 "" atlas_cli_install
  StrCmp $1 "$INSTDIR\atlas.exe" atlas_cli_install atlas_cli_done
  atlas_cli_install:
  IfFileExists "$INSTDIR\atlas.exe" 0 atlas_cli_done
  nsExec::Exec '"$INSTDIR\atlas.exe" install'
  Pop $0
  atlas_cli_done:

  WriteRegStr SHCTX "${UNINSTKEY}" "QuietUninstallString" "$\"$INSTDIR\uninstall.exe$\" /S"
  IfFileExists "$INSTDIR\Atlas Uninstall.exe" 0 atlas_uninstall_done
  WriteRegStr SHCTX "${UNINSTKEY}" "UninstallString" "$\"$INSTDIR\Atlas Uninstall.exe$\" --uninstall"
  WriteRegStr SHCTX "${UNINSTKEY}" "DisplayIcon" "$\"$INSTDIR\Atlas Uninstall.exe$\""
  atlas_uninstall_done:
  !insertmacro ATLAS_OPENWITH_REGISTER
!macroend

; Atlas-Setup.exe's own copy goes with the app. When the uninstall was started from that copy
; (Settings > Apps) this Delete fails because the image is running; Atlas-Setup.exe then schedules
; the deletion of the copy and the folder for after it exits (a running program cannot delete
; itself).
!macro NSIS_HOOK_POSTUNINSTALL
  Delete "$INSTDIR\Atlas Uninstall.exe"
  !insertmacro ATLAS_OPENWITH_UNREGISTER
!macroend
