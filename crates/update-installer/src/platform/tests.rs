use super::*;

#[test]
fn an_upgrade_keeps_a_custom_install_folder_with_spaces() {
    let msi = Path::new(r"C:\Users\Jo Smith\AppData\Local\Cinnabar\update\update.msi");
    let target = Path::new(r"D:\My Games\Cinnabar");
    assert_eq!(
        msiexec_arguments(msi, target),
        [
            "/i",
            r#""C:\Users\Jo Smith\AppData\Local\Cinnabar\update\update.msi""#,
            "/passive",
            "/norestart",
            "REBOOT=ReallySuppress",
            r#"INSTALLFOLDER="D:\My Games\Cinnabar""#,
        ]
    );
}
