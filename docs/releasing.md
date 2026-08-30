# Release guide

Stable releases are created by pushing a `vMAJOR.MINOR.PATCH` tag. The unified
GitHub Actions workflow verifies both platforms, builds all artifacts, and
publishes one GitHub Release only after the macOS and Android jobs both pass.

## Release artifacts

Each release contains:

- `Trackpad-Companion-VERSION-macos.dmg`: Developer ID signed, notarized, and
  stapled drag-to-Applications installer;
- `Trackpad-Companion-VERSION-macos.zip`: the same signed and stapled app bundle;
- `Trackpad-Companion-VERSION-android.apk`: directly installable signed APK;
- `Trackpad-Companion-VERSION-android.aab`: signed bundle for Google Play;
- `SHA256SUMS`: SHA-256 digests for every binary artifact.

GitHub adds the source ZIP and tarball automatically. The workflow never creates
a partial release: the publish job depends on both platform build jobs.

## Version gate

Before tagging, keep these fields synchronized:

- root `Cargo.toml` package version;
- `crates/touchpad-proto/Cargo.toml` version;
- Android `versionName` in `android/app/build.gradle.kts`;
- `CFBundleShortVersionString` in `packaging/macos/Info.plist`.

Increment Android `versionCode` for every Android release. The tag must be the
shared version prefixed with `v`, for example `v0.1.0`.

## GitHub Actions secrets

Configure these repository or protected release-environment secrets. Never
commit certificates, keystores, passwords, or exported secret values.

### Android

| Secret | Value |
| --- | --- |
| `ANDROID_KEYSTORE_BASE64` | Base64-encoded JKS/PKCS12 upload keystore |
| `ANDROID_KEYSTORE_PASSWORD` | Keystore password |
| `ANDROID_KEY_ALIAS` | Upload-key alias |
| `ANDROID_KEY_PASSWORD` | Upload-key password |

Generate a long-lived upload key once and back it up securely:

```sh
keytool -genkeypair -v \
  -keystore trackpad-companion-upload.jks \
  -alias trackpad-companion \
  -keyalg RSA -keysize 4096 -validity 10000
```

Encode the keystore as a single-line secret value:

```sh
base64 < trackpad-companion-upload.jks | tr -d '\n'
```

Use the same upload key for every update. Google Play releases should enable
Play App Signing and use this keystore as the upload key.

### macOS

| Secret | Value |
| --- | --- |
| `MACOS_CERTIFICATE_P12` | Base64-encoded Developer ID Application certificate |
| `MACOS_CERTIFICATE_PASSWORD` | Exported P12 password |
| `MACOS_KEYCHAIN_PASSWORD` | Random password for the temporary Actions keychain |
| `MACOS_CODESIGN_IDENTITY` | Full `Developer ID Application: ...` identity |
| `APPLE_ID` | Apple developer account email |
| `APPLE_TEAM_ID` | Apple Developer team identifier |
| `APPLE_APP_SPECIFIC_PASSWORD` | App-specific password used by `notarytool` |

The workflow imports the certificate into an ephemeral keychain, signs with the
hardened runtime, notarizes the app archive, staples the app, rebuilds the ZIP,
then notarizes and staples the DMG. Temporary certificate and keychain files are
removed even when the job fails.

## Publish sequence

1. Commit and push the release candidate to `main`.
2. Wait for the normal CI workflow to pass.
3. Confirm all version fields and Android `versionCode` are correct.
4. Create and push the annotated tag:

   ```sh
   git tag -a v0.1.0 -m "Trackpad Companion v0.1.0"
   git push origin v0.1.0
   ```

5. Wait for the Release workflow. After it completes, download all five assets
   and verify `SHA256SUMS` before announcing the release.

Do not move or recreate an already published tag. Fix the problem, increment the
version, and publish a new tag instead.
