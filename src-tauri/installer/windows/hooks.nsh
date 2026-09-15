; Registers/removes a per-user Explorer context menu entry, "Download with
; Pully", on Windows Internet Shortcut (.url) files — the only filesystem
; object that actually represents a "link" a user can right-click. Written
; to HKCU so it applies without elevation regardless of whether Pully itself
; was installed per-machine or per-user. See docs/WINDOWS_INTEGRATION.md.
;
; Referenced from tauri.conf.json as bundle.windows.nsis.installerHooks.
; Tauri's NSIS template calls these macros if defined.

!macro Pully_WriteContextMenuEntry base
  WriteRegStr HKCU "${base}" "" "Download with Pully"
  WriteRegStr HKCU "${base}" "Icon" "$INSTDIR\Pully.exe"
  WriteRegStr HKCU "${base}\command" "" '"$INSTDIR\Pully.exe" "%1"'
!macroend

!macro NSIS_HOOK_POSTINSTALL
  !insertmacro Pully_WriteContextMenuEntry "Software\Classes\InternetShortcut\shell\Pully.DownloadWithPully"
  !insertmacro Pully_WriteContextMenuEntry "Software\Classes\SystemFileAssociations\.url\shell\Pully.DownloadWithPully"
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  DeleteRegKey HKCU "Software\Classes\InternetShortcut\shell\Pully.DownloadWithPully"
  DeleteRegKey HKCU "Software\Classes\SystemFileAssociations\.url\shell\Pully.DownloadWithPully"
!macroend
