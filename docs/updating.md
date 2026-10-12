# Updates

The launcher checks its selected release channel once a day and downloads verified updates
in the background. An interrupted transfer resumes from its digest-named partial file, or
restarts safely if the server does not support ranges.

When the download is ready, choose **Restart and update** or **Later**. Later defers installation
for this session. In **Settings → General**, you can turn off update checks, select a channel,
or enable **Install updates when I quit**. Installation on quit is off by default. The in-game
pause menu only shows a small ready badge; update actions remain in the launcher.

Beta and Nightly are available initially. Stable appears after a signed stable release exists.
Changing channels never silently downgrades: a slower channel waits for a newer app version
and build. **Switch now** explicitly permits the selected channel's older release. Numeric
prereleases use SemVer ordering, and successive builds use a signed monotonic build number.

The client checks the manifest's Ed25519 signature, channel, expiry, app version and build,
then the artifact's size and SHA-256. The existing trusted-key list supports key rotation.
A separate Rust helper waits for the exact client process to exit and verifies the stage again.
A new version preparing game assets reports progress, so only a start that stalls for five minutes
counts as failed; the helper then stops the client and its setup process before restoring files.

- **macOS:** the helper replaces the complete signed app bundle from the DMG and preserves its
  signature and notarization ticket. The previous bundle remains beside it until the new client
  completes startup. A failed new start restores the previous bundle.
- **Linux:** the same swap and startup rollback apply to AppImages. Unpacked installations use
  the manual download action.
- **Windows:** the helper runs the verified MSI passively with existing rights and relaunches
  the client. The previous downloaded MSI remains in the update directory for manual repair.

If the install location is not writable, the launcher offers **Download update**. It opens the
verified DMG or installer for you to finish; on Linux it opens the folder containing the AppImage.
The updater does not request elevated privileges or reboot the machine.

Each channel's manifest lives in a fixed GitHub release; `update_channels` in
`packaging/release-assets.json` names its tag and file for both the client and the package
workflow. Publication must sign the correct channel and a positive build number. The release signer accepts `-build-number`;
packaged builds use `CINNABAR_BUILD_NUMBER` or the package workflow's `GITHUB_RUN_NUMBER`.
