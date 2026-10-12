# Automatic updates

Installed clients check for a signed release once a day. A newer release downloads in the
background. The home screen shows when it is ready; quitting normally starts installation.
Turn off **Automatic Updates** in **Settings → General** to stop downloads and installation.
The saved choice takes effect immediately. Interrupted downloads restart safely on the next launch.

Updates require a release manifest signed by a key trusted by the installed client. The client
and its separate Rust helper check the signature, version, platform, byte count, and SHA-256.
The helper has no network client. Invalid or expired releases are never installed.

- **macOS:** the helper replaces the whole signed app bundle from the release DMG. It preserves
  the bundle's signature and stapled notarization ticket. The app's parent folder must be writable.
- **Windows:** Windows Installer upgrades the MSI installation after the client exits. Windows
  asks for administrator consent. Cancelling leaves the installation unchanged. In-use files may
  require a later restart; the updater never reboots the machine automatically.
- **Linux:** the helper replaces the running AppImage's source file. Its parent folder must be
  writable. Unpacked installations, including `install.sh` installations, use manual updates.

The previous AppImage or app bundle stays beside the installation with a `.previous` suffix.
Windows keeps a recovery copy in the user's update directory. These copies are removed only
when the new version completes its first client frame. A failed file or bundle swap restores
the old version; Windows Installer performs its own transaction rollback. A later application
crash does not automatically restore the old version.

Downloads, the startup receipt, and `apply.log` live in the install layout's per-user `update`
directory. If installation fails, this log records the reason. Game worlds and account data
are outside the replaced application payload.
