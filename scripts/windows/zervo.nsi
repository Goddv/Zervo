; NSIS installer for Zervo.
;
; Built by scripts/package-windows.ps1, which passes VERSION, ARCH, STAGE and
; OUTFILE in. NSIS 3.10 is preinstalled on both GitHub Windows runner images,
; so there is nothing to fetch.
;
; This is a PER-USER install: into %LOCALAPPDATA%\Programs\Zervo, registered
; under HKCU, with RequestExecutionLevel user. That is deliberate. The build is
; unsigned, and asking an unsigned installer for administrator rights is the
; combination Windows objects to most loudly — and there is nothing in Zervo
; that needs to be outside the user's own profile.

Unicode true
ManifestDPIAware true

!ifndef VERSION
  !error "pass /DVERSION=x.y.z"
!endif
!ifndef STAGE
  !error "pass /DSTAGE=<staging directory>"
!endif
!ifndef ARCH
  !define ARCH "x64"
!endif
!ifndef OUTFILE
  !define OUTFILE "Zervo-${VERSION}-windows-${ARCH}-setup.exe"
!endif

!define APPNAME    "Zervo"
!define PUBLISHER  "Goddv"
!define HOMEPAGE   "https://github.com/Goddv/Zervo"
!define REGKEY     "Software\Microsoft\Windows\CurrentVersion\Uninstall\Zervo"

Name "${APPNAME} ${VERSION}"
OutFile "${OUTFILE}"
InstallDir "$LOCALAPPDATA\Programs\Zervo"
InstallDirRegKey HKCU "Software\Zervo" "InstallDir"
RequestExecutionLevel user
SetCompressor /SOLID lzma

VIProductVersion "${VERSION}.0"
VIAddVersionKey "ProductName"     "${APPNAME}"
VIAddVersionKey "FileDescription" "Zervo installer"
VIAddVersionKey "FileVersion"     "${VERSION}"
VIAddVersionKey "ProductVersion"  "${VERSION}"
VIAddVersionKey "CompanyName"     "${PUBLISHER}"
VIAddVersionKey "LegalCopyright"  "Mozilla Public License 2.0"

!include "MUI2.nsh"
!include "FileFunc.nsh"
!insertmacro GetSize

!define MUI_ICON   "${__FILEDIR__}\..\..\assets\icon\zervo.ico"
!define MUI_UNICON "${__FILEDIR__}\..\..\assets\icon\zervo.ico"
!define MUI_ABORTWARNING

!insertmacro MUI_PAGE_LICENSE "${__FILEDIR__}\..\..\LICENSE"
!insertmacro MUI_PAGE_COMPONENTS
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES
!define MUI_FINISHPAGE_RUN "$INSTDIR\Zervo.exe"
!insertmacro MUI_PAGE_FINISH

!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES

!insertmacro MUI_LANGUAGE "English"

Section "Zervo" SecMain
  SectionIn RO
  SetOutPath "$INSTDIR"
  ; Everything the packaging script staged: the exe, ANGLE, the CRT, the
  ; licence and the readme.
  File /r "${STAGE}\*.*"

  CreateShortCut "$SMPROGRAMS\Zervo.lnk" "$INSTDIR\Zervo.exe" "" "$INSTDIR\Zervo.exe" 0

  WriteRegStr HKCU "Software\Zervo" "InstallDir" "$INSTDIR"
  WriteRegStr HKCU "Software\Zervo" "Version"    "${VERSION}"

  ; Add/Remove Programs. EstimatedSize is in KiB and is what the list shows.
  WriteRegStr   HKCU "${REGKEY}" "DisplayName"     "${APPNAME}"
  WriteRegStr   HKCU "${REGKEY}" "DisplayVersion"  "${VERSION}"
  WriteRegStr   HKCU "${REGKEY}" "DisplayIcon"     "$INSTDIR\Zervo.exe"
  WriteRegStr   HKCU "${REGKEY}" "Publisher"       "${PUBLISHER}"
  WriteRegStr   HKCU "${REGKEY}" "URLInfoAbout"    "${HOMEPAGE}"
  WriteRegStr   HKCU "${REGKEY}" "InstallLocation" "$INSTDIR"
  WriteRegStr   HKCU "${REGKEY}" "UninstallString" '"$INSTDIR\uninstall.exe"'
  WriteRegDWORD HKCU "${REGKEY}" "NoModify" 1
  WriteRegDWORD HKCU "${REGKEY}" "NoRepair" 1

  ; What puts Zervo in Settings > Default apps. Registering the capability is
  ; all an application may do; Windows will not let an installer make itself
  ; the default browser, and should not.
  WriteRegStr HKCU "Software\Clients\StartMenuInternet\Zervo\Capabilities" "ApplicationName"        "${APPNAME}"
  WriteRegStr HKCU "Software\Clients\StartMenuInternet\Zervo\Capabilities" "ApplicationDescription" "A sidebar-first browser built on the Servo engine"
  WriteRegStr HKCU "Software\Clients\StartMenuInternet\Zervo\Capabilities" "ApplicationIcon"        "$INSTDIR\Zervo.exe,0"
  WriteRegStr HKCU "Software\Clients\StartMenuInternet\Zervo\Capabilities\URLAssociations" "http"   "ZervoURL"
  WriteRegStr HKCU "Software\Clients\StartMenuInternet\Zervo\Capabilities\URLAssociations" "https"  "ZervoURL"
  WriteRegStr HKCU "Software\Clients\StartMenuInternet\Zervo\Capabilities\FileAssociations" ".html" "ZervoHTML"
  WriteRegStr HKCU "Software\Clients\StartMenuInternet\Zervo\Capabilities\FileAssociations" ".htm"  "ZervoHTML"
  WriteRegStr HKCU "Software\Clients\StartMenuInternet\Zervo\DefaultIcon" "" "$INSTDIR\Zervo.exe,0"
  WriteRegStr HKCU "Software\Clients\StartMenuInternet\Zervo\shell\open\command" "" '"$INSTDIR\Zervo.exe"'
  WriteRegStr HKCU "Software\RegisteredApplications" "Zervo" "Software\Clients\StartMenuInternet\Zervo\Capabilities"

  WriteRegStr HKCU "Software\Classes\ZervoURL" "" "Zervo URL"
  WriteRegStr HKCU "Software\Classes\ZervoURL" "URL Protocol" ""
  WriteRegStr HKCU "Software\Classes\ZervoURL\DefaultIcon" "" "$INSTDIR\Zervo.exe,0"
  WriteRegStr HKCU "Software\Classes\ZervoURL\shell\open\command" "" '"$INSTDIR\Zervo.exe" "%1"'
  WriteRegStr HKCU "Software\Classes\ZervoHTML" "" "HTML Document"
  WriteRegStr HKCU "Software\Classes\ZervoHTML\DefaultIcon" "" "$INSTDIR\Zervo.exe,0"
  WriteRegStr HKCU "Software\Classes\ZervoHTML\shell\open\command" "" '"$INSTDIR\Zervo.exe" "%1"'

  WriteUninstaller "$INSTDIR\uninstall.exe"

  ; After the uninstaller, not before: this is what Add/Remove Programs shows,
  ; in KiB, and measuring the directory before writing a file into it reports a
  ; size that is short by the uninstaller.
  ${GetSize} "$INSTDIR" "/S=0K" $0 $1 $2
  WriteRegDWORD HKCU "${REGKEY}" "EstimatedSize" "$0"
SectionEnd

Section "Desktop shortcut" SecDesktop
  CreateShortCut "$DESKTOP\Zervo.lnk" "$INSTDIR\Zervo.exe" "" "$INSTDIR\Zervo.exe" 0
SectionEnd

!insertmacro MUI_FUNCTION_DESCRIPTION_BEGIN
  !insertmacro MUI_DESCRIPTION_TEXT ${SecMain}    "Zervo, its ANGLE libraries and the Visual C++ runtime it needs."
  !insertmacro MUI_DESCRIPTION_TEXT ${SecDesktop} "A shortcut on the desktop as well as in the Start menu."
!insertmacro MUI_FUNCTION_DESCRIPTION_END

Section "Uninstall"
  Delete "$DESKTOP\Zervo.lnk"
  Delete "$SMPROGRAMS\Zervo.lnk"

  ; RMDir /r on $INSTDIR only. Zervo's settings live in the user's profile and
  ; are left alone: an uninstall is not a request to throw away bookmarks.
  RMDir /r "$INSTDIR"

  DeleteRegValue HKCU "Software\RegisteredApplications" "Zervo"
  DeleteRegKey HKCU "Software\Clients\StartMenuInternet\Zervo"
  DeleteRegKey HKCU "Software\Classes\ZervoURL"
  DeleteRegKey HKCU "Software\Classes\ZervoHTML"
  DeleteRegKey HKCU "Software\Zervo"
  DeleteRegKey HKCU "${REGKEY}"
SectionEnd
