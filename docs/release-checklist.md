# Release Checklist

## Pre-Release
- [ ] All tests pass
- [ ] Benchmark regression check (docs/benchmarks.md)
- [ ] Manual test matrix (docs/test-matrix.md)
- [ ] Fault injection scenarios (docs/fault-injection.md)

## Build Release

```bash
./scripts/build-release.sh
```

Or manually:
```bash
cd rust-core && cargo build --release --no-default-features && cd ..
xcodebuild -scheme UltraMeeting -configuration Release clean build
```

## Signing & Notarization

### Entitlements (UltraMeeting.entitlements)

```xml
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
	<key>com.apple.security.device.audio-input</key>
	<true/>
	<key>com.apple.security.cs.hardened-runtime</key>
	<true/>
</dict>
</plist>
```

Note: Screen capture does not use a separate entitlement; it uses TCC and NSScreenCaptureUsageDescription.

### Code Sign

```bash
codesign --deep --force --options runtime --sign "Developer ID Application: YOUR_NAME" UltraMeeting.app
```

### Notarize

```bash
# Create notarization credentials (one-time)
xcrun notarytool store-credentials notary-profile --apple-id YOUR_EMAIL --team-id TEAM_ID --password APP_SPECIFIC_PASSWORD

# Submit
xcrun notarytool submit UltraMeeting.dmg --keychain-profile notary-profile --wait

# Staple
xcrun stapler staple UltraMeeting.app
```

## Distribution
- [ ] DMG created
- [ ] README with install instructions
- [ ] Permission setup guide
