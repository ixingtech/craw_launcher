!define CLAW_PROG_ID "OpenClawLauncher.claw"

!macro NSIS_HOOK_POSTINSTALL
  WriteRegStr HKCU "Software\Classes\.claw" "" "${CLAW_PROG_ID}"
  WriteRegStr HKCU "Software\Classes\${CLAW_PROG_ID}" "" "${PRODUCTNAME} Package"
  WriteRegStr HKCU "Software\Classes\${CLAW_PROG_ID}\DefaultIcon" "" "$INSTDIR\${MAINBINARYNAME}.exe,0"
  WriteRegStr HKCU "Software\Classes\${CLAW_PROG_ID}\shell" "" "open"
  WriteRegStr HKCU "Software\Classes\${CLAW_PROG_ID}\shell\open\command" "" '"$INSTDIR\${MAINBINARYNAME}.exe" "%1"'
  System::Call 'Shell32::SHChangeNotify(i, i, p, p) (0x08000000, 0, 0, 0)'
!macroend

!macro NSIS_HOOK_POSTUNINSTALL
  DeleteRegKey HKCU "Software\Classes\${CLAW_PROG_ID}"
  DeleteRegKey HKCU "Software\Classes\.claw"
  System::Call 'Shell32::SHChangeNotify(i, i, p, p) (0x08000000, 0, 0, 0)'
!macroend
