//! The Windows installer's NSIS script. makensis (NSIS 3, on the Windows
//! runner) compiles it; this only writes it.
//!
//! It installs per user, as the Tauri NSIS installer did, at
//! `%LOCALAPPDATA%\Method Reader` under the uninstall key
//! `HKCU\...\Uninstall\Method Reader`. Up to 0.12.x the app was called
//! Screepub and installed at `%LOCALAPPDATA%\Screepub` under
//! `...\Uninstall\Screepub` (the Tauri window's layout, kept by the Rust
//! 0.12 installer): running this over such an install removes it (its
//! executable, the old engine sidecar, its notices, its uninstaller, its
//! Start-menu shortcut and its two registry keys), so Apps & Features
//! shows one Method Reader and not two apps. Settings and the library are
//! the engine's (it moves the old `%APPDATA%\Screepub` and
//! `%APPDATA%\com.darkwell.screepub` into `%APPDATA%\com.methodreader.desktop`,
//! and `Documents\Screepub` to `Documents\Method Reader`, once itself)
//! and are never touched here, by install or uninstall.
//!
//! Unsigned, as the Tauri installer was: SmartScreen warns until a
//! code-signing certificate exists (a decision for the owner; signtool
//! would sign this file and the window binary in release.yml).
//!
//! `/S` is NSIS's own silent switch; `/R` (ours) starts Method Reader when done,
//! which is how an in-app update would hand over on Windows
//! (`sp_update::install::nsis_update_args`).

use crate::names::{COPYRIGHT, LEGACY_PRODUCT, LEGACY_WINDOW_EXE, PRODUCT, PUBLISHER, WINDOW_EXE};

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
        r#"; Method Reader's Windows installer. Written by `sp-release nsis`; do not edit.
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
!define COPYRIGHT "{COPYRIGHT}"
!define UNINSTKEY "Software\Microsoft\Windows\CurrentVersion\Uninstall\${{PRODUCTNAME}}"
!define MANUPRODUCTKEY "Software\${{MANUFACTURER}}\${{PRODUCTNAME}}"
!define SRC "{src_dir}"
; The 0.12.x install (the app was called Screepub), removed by this one.
!define LEGACYNAME "{LEGACY_PRODUCT}"
!define LEGACYBINARYNAME "{LEGACY_WINDOW_EXE}"
!define LEGACYMANUFACTURER "Darkwell Entertainment LLC"
!define LEGACYDIR "$LOCALAPPDATA\${{LEGACYNAME}}"
!define LEGACYUNINSTKEY "Software\Microsoft\Windows\CurrentVersion\Uninstall\${{LEGACYNAME}}"
!define LEGACYMANUPRODUCTKEY "Software\${{LEGACYMANUFACTURER}}\${{LEGACYNAME}}"

Name "${{PRODUCTNAME}}"
OutFile "{out_file}"
InstallDir "$LOCALAPPDATA\${{PRODUCTNAME}}"
Icon "${{SRC}}\icon.ico"
UninstallIcon "${{SRC}}\icon.ico"
VIProductVersion "{numeric}.0"
VIAddVersionKey "ProductName" "${{PRODUCTNAME}}"
VIAddVersionKey "FileDescription" "${{PRODUCTNAME}}"
VIAddVersionKey "CompanyName" "${{MANUFACTURER}}"
VIAddVersionKey "LegalCopyright" "${{COPYRIGHT}}"
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
      MessageBox MB_OK|MB_ICONSTOP "Method Reader is still running. Quit it, then run this installer again." /SD IDOK
      Abort
    ${{EndIf}}
    Sleep 100
  ${{Loop}}

  ; An install from before the rename (Screepub 0.12.x or older): remove it,
  ; so there is one app, not two. Its settings and library are not here.
  ${{If}} ${{FileExists}} "${{LEGACYDIR}}\${{LEGACYBINARYNAME}}.exe"
    StrCpy $0 0
    ${{Do}}
      ClearErrors
      Delete "${{LEGACYDIR}}\${{LEGACYBINARYNAME}}.exe"
      ${{IfNot}} ${{Errors}}
        ${{Break}}
      ${{EndIf}}
      IntOp $0 $0 + 1
      ${{If}} $0 > 100
        MessageBox MB_OK|MB_ICONSTOP "Screepub, the older version of Method Reader, is still running. Quit it, then run this installer again." /SD IDOK
        Abort
      ${{EndIf}}
      Sleep 100
    ${{Loop}}
  ${{EndIf}}
  Delete "${{LEGACYDIR}}\screepub-engine.exe"
  Delete "${{LEGACYDIR}}\LICENSE"
  Delete "${{LEGACYDIR}}\LICENSE-MIT"
  Delete "${{LEGACYDIR}}\LICENSE-APACHE"
  Delete "${{LEGACYDIR}}\LICENSE-GPL-3.0"
  Delete "${{LEGACYDIR}}\THIRD-PARTY-NOTICES.md"
  Delete "${{LEGACYDIR}}\uninstall.exe"
  RMDir "${{LEGACYDIR}}"
  Delete "$SMPROGRAMS\${{LEGACYNAME}}.lnk"
  Delete "$DESKTOP\${{LEGACYNAME}}.lnk"
  DeleteRegKey HKCU "${{LEGACYUNINSTKEY}}"
  DeleteRegKey HKCU "${{LEGACYMANUPRODUCTKEY}}"
  ; Again, in case the folder chosen was the old one and went with it.
  SetOutPath "$INSTDIR"
  ; The AGPL licence file of releases up to 0.12.1 (an install chosen into
  ; the old folder by hand).
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
    fn it_installs_per_user_under_the_new_name() {
        let s = script(
            "0.13.0",
            r"C:\stage",
            r"C:\out\Method-Reader-windows-x64-setup.exe",
        );
        assert!(s.contains("!define PRODUCTNAME \"Method Reader\""));
        assert!(s.contains("InstallDir \"$LOCALAPPDATA\\${PRODUCTNAME}\""));
        assert!(s.contains("!define UNINSTKEY \"Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\${PRODUCTNAME}\""));
        assert!(s.contains("!define MAINBINARYNAME \"method-reader-app\""));
        assert!(s.contains(
            "!define MANUFACTURER \"Method Production Suite Team and the Method Reader contributors\""
        ));
        assert!(s.contains(
            "!define COPYRIGHT \"Copyright (c) 2026 Method Production Suite Team and the Method Reader contributors\""
        ));
        assert!(s.contains("VIAddVersionKey \"LegalCopyright\" \"${COPYRIGHT}\""));
        assert!(s.contains("VIProductVersion \"0.13.0.0\""));
        assert!(s.contains("OutFile \"C:\\out\\Method-Reader-windows-x64-setup.exe\""));
    }

    #[test]
    fn it_removes_a_screepub_install_so_there_are_not_two() {
        let s = script("0.13.0", "s", "o");
        // The 0.12.x layout: folder, executable, keys and shortcut.
        assert!(s.contains("!define LEGACYDIR \"$LOCALAPPDATA\\${LEGACYNAME}\""));
        assert!(s.contains("!define LEGACYNAME \"Screepub\""));
        assert!(s.contains("!define LEGACYBINARYNAME \"screepub-desktop\""));
        assert!(s.contains(
            "!define LEGACYUNINSTKEY \"Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\${LEGACYNAME}\""
        ));
        assert!(s.contains(
            "!define LEGACYMANUPRODUCTKEY \"Software\\${LEGACYMANUFACTURER}\\${LEGACYNAME}\""
        ));
        assert!(s.contains("!define LEGACYMANUFACTURER \"Darkwell Entertainment LLC\""));
        let install =
            &s[s.find("Section \"Install\"").unwrap()..s.find("Section \"Uninstall\"").unwrap()];
        for line in [
            "Delete \"${LEGACYDIR}\\${LEGACYBINARYNAME}.exe\"",
            "Delete \"${LEGACYDIR}\\screepub-engine.exe\"",
            "Delete \"${LEGACYDIR}\\uninstall.exe\"",
            "RMDir \"${LEGACYDIR}\"",
            "Delete \"$SMPROGRAMS\\${LEGACYNAME}.lnk\"",
            "DeleteRegKey HKCU \"${LEGACYUNINSTKEY}\"",
            "DeleteRegKey HKCU \"${LEGACYMANUPRODUCTKEY}\"",
        ] {
            assert!(install.contains(line), "{line}");
        }
        // Removed before the new files go in, and never recursively: the
        // folder only goes if it is empty.
        assert!(
            install.find("RMDir \"${LEGACYDIR}\"").unwrap()
                < install.find("File \"${SRC}").unwrap()
        );
        assert!(!s.contains("RMDir /r"));
        // Settings and the library are never touched.
        assert!(!s.contains("$APPDATA"));
        assert!(!s.contains("$DOCUMENTS"));
    }

    #[test]
    fn a_prerelease_still_gets_a_numeric_file_version() {
        assert!(script("1.0.0-rc1", "s", "o").contains("VIProductVersion \"1.0.0.0\""));
    }
}
