//! The Windows installer's NSIS script. makensis (NSIS 3, on the Windows
//! runner) compiles it; this only writes it.
//!
//! It installs where the Tauri NSIS installer did (per user,
//! `%LOCALAPPDATA%\Screepub`) under the same uninstall key
//! (`HKCU\...\Uninstall\Screepub`) and the same executable name, so
//! running it over a Tauri install upgrades that install in place: Apps &
//! Features shows one Screepub, the Start-menu shortcut keeps working, and
//! the old engine sidecar (`screepub-engine.exe`) is removed. Settings and
//! the library are the engine's (`%APPDATA%\Screepub`,
//! `Documents\Screepub`) and are never touched, by install or uninstall.
//!
//! Unsigned, as the Tauri installer was: SmartScreen warns until a
//! code-signing certificate exists (a decision for the owner; signtool
//! would sign this file and the window binary in release.yml).
//!
//! `/S` is NSIS's own silent switch; `/R` (ours) starts Screepub when done,
//! which is how an in-app update would hand over on Windows
//! (`sp_update::install::nsis_update_args`).

use crate::names::{PRODUCT, PUBLISHER, WINDOW_EXE};

/// The script, for `version` (numeric dotted, as VIProductVersion needs),
/// packing the files in `src_dir` (the window, the three LICENSE- files,
/// THIRD-PARTY-NOTICES.md, icon.ico) into `out_file`.
pub fn script(version: &str, src_dir: &str, out_file: &str) -> String {
    let numeric: String = version
        .split(['-', '+'])
        .next()
        .unwrap_or("0.0.0")
        .to_string();
    format!(
        r#"; Screepub's Windows installer. Written by `sp-release nsis`; do not edit.
Unicode true
ManifestDPIAware true
SetCompressor /SOLID lzma
RequestExecutionLevel user

!include "FileFunc.nsh"
!include "LogicLib.nsh"

!define PRODUCTNAME "{PRODUCT}"
!define VERSION "{version}"
!define MAINBINARYNAME "{WINDOW_EXE}"
!define MANUFACTURER "{PUBLISHER}"
!define UNINSTKEY "Software\Microsoft\Windows\CurrentVersion\Uninstall\${{PRODUCTNAME}}"
!define MANUPRODUCTKEY "Software\${{MANUFACTURER}}\${{PRODUCTNAME}}"
!define SRC "{src_dir}"

Name "${{PRODUCTNAME}}"
OutFile "{out_file}"
InstallDir "$LOCALAPPDATA\${{PRODUCTNAME}}"
Icon "${{SRC}}\icon.ico"
UninstallIcon "${{SRC}}\icon.ico"
VIProductVersion "{numeric}.0"
VIAddVersionKey "ProductName" "${{PRODUCTNAME}}"
VIAddVersionKey "FileDescription" "${{PRODUCTNAME}}"
VIAddVersionKey "CompanyName" "${{MANUFACTURER}}"
VIAddVersionKey "LegalCopyright" "${{MANUFACTURER}}"
VIAddVersionKey "FileVersion" "${{VERSION}}"
VIAddVersionKey "ProductVersion" "${{VERSION}}"

Page directory
Page instfiles
UninstPage uninstConfirm
UninstPage instfiles

Section "Install"
  SetOutPath "$INSTDIR"
  ; A window still closing (an update hands over from it) holds its exe for
  ; a moment: wait up to ten seconds for it to let go.
  StrCpy $0 0
  ${{Do}}
    ClearErrors
    Delete "$INSTDIR\${{MAINBINARYNAME}}.exe"
    ${{IfNot}} ${{Errors}}
      ${{Break}}
    ${{EndIf}}
    IntOp $0 $0 + 1
    ${{If}} $0 > 100
      MessageBox MB_OK|MB_ICONSTOP "Screepub is still running. Quit it, then run this installer again." /SD IDOK
      Abort
    ${{EndIf}}
    Sleep 100
  ${{Loop}}
  ; The Tauri window's engine, which the Rust app does not have.
  Delete "$INSTDIR\screepub-engine.exe"
  ; The AGPL licence file of releases up to 0.12.1.
  Delete "$INSTDIR\LICENSE"
  File "${{SRC}}\${{MAINBINARYNAME}}.exe"
  File "${{SRC}}\LICENSE-MIT"
  File "${{SRC}}\LICENSE-APACHE"
  File "${{SRC}}\LICENSE-GPL-3.0"
  File "${{SRC}}\THIRD-PARTY-NOTICES.md"
  WriteUninstaller "$INSTDIR\uninstall.exe"

  CreateShortcut "$SMPROGRAMS\${{PRODUCTNAME}}.lnk" "$INSTDIR\${{MAINBINARYNAME}}.exe"

  WriteRegStr HKCU "${{MANUPRODUCTKEY}}" "" "$INSTDIR"
  WriteRegStr HKCU "${{UNINSTKEY}}" "DisplayName" "${{PRODUCTNAME}}"
  WriteRegStr HKCU "${{UNINSTKEY}}" "DisplayIcon" "$\"$INSTDIR\${{MAINBINARYNAME}}.exe$\""
  WriteRegStr HKCU "${{UNINSTKEY}}" "DisplayVersion" "${{VERSION}}"
  WriteRegStr HKCU "${{UNINSTKEY}}" "Publisher" "${{MANUFACTURER}}"
  WriteRegStr HKCU "${{UNINSTKEY}}" "InstallLocation" "$\"$INSTDIR$\""
  WriteRegStr HKCU "${{UNINSTKEY}}" "UninstallString" "$\"$INSTDIR\uninstall.exe$\""
  WriteRegStr HKCU "${{UNINSTKEY}}" "MainBinaryName" "${{MAINBINARYNAME}}.exe"
  WriteRegDWORD HKCU "${{UNINSTKEY}}" "NoModify" 1
  WriteRegDWORD HKCU "${{UNINSTKEY}}" "NoRepair" 1
  ${{GetSize}} "$INSTDIR" "/S=0K" $0 $1 $2
  WriteRegDWORD HKCU "${{UNINSTKEY}}" "EstimatedSize" "$0"

  ${{GetParameters}} $1
  ClearErrors
  ${{GetOptions}} $1 "/R" $2
  ${{IfNot}} ${{Errors}}
    Exec '"$INSTDIR\${{MAINBINARYNAME}}.exe"'
  ${{EndIf}}
SectionEnd

Section "Uninstall"
  Delete "$INSTDIR\${{MAINBINARYNAME}}.exe"
  Delete "$INSTDIR\screepub-engine.exe"
  Delete "$INSTDIR\LICENSE"
  Delete "$INSTDIR\LICENSE-MIT"
  Delete "$INSTDIR\LICENSE-APACHE"
  Delete "$INSTDIR\LICENSE-GPL-3.0"
  Delete "$INSTDIR\THIRD-PARTY-NOTICES.md"
  Delete "$INSTDIR\uninstall.exe"
  RMDir "$INSTDIR"
  Delete "$SMPROGRAMS\${{PRODUCTNAME}}.lnk"
  Delete "$DESKTOP\${{PRODUCTNAME}}.lnk"
  DeleteRegKey HKCU "${{UNINSTKEY}}"
  DeleteRegKey HKCU "${{MANUPRODUCTKEY}}"
SectionEnd
"#
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_installs_where_the_tauri_installer_did() {
        let s = script(
            "0.12.0",
            r"C:\stage",
            r"C:\out\Screepub-windows-x64-setup.exe",
        );
        assert!(s.contains("InstallDir \"$LOCALAPPDATA\\${PRODUCTNAME}\""));
        assert!(s.contains("!define UNINSTKEY \"Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\${PRODUCTNAME}\""));
        assert!(s.contains("!define MAINBINARYNAME \"screepub-desktop\""));
        assert!(s.contains("Delete \"$INSTDIR\\screepub-engine.exe\""));
        assert!(s.contains("VIProductVersion \"0.12.0.0\""));
        assert!(s.contains("OutFile \"C:\\out\\Screepub-windows-x64-setup.exe\""));
    }

    #[test]
    fn a_prerelease_still_gets_a_numeric_file_version() {
        assert!(script("1.0.0-rc1", "s", "o").contains("VIProductVersion \"1.0.0.0\""));
    }
}
