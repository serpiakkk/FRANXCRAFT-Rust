# FRANXCRAFT Rust

An independent Android project based on [Starling](https://github.com/starling-client/starling), formerly Horizon Android, and the upstream [Horizon](https://github.com/ludovic111/horizon) project. FRANXCRAFT Rust is unofficial and is not affiliated with or endorsed by Mojang or Microsoft.

This is an early development snapshot, not a released or tested APK. The original README, LICENSE, third-party notices and contributor credits remain in this repository. The upstream Starling development branch is `starling`; the older `android` branch is historical.

## Android source build

See the original [build instructions](README.md#build-from-source). Requirements include Rust 1.90 or newer with Android target and rust-src, cargo-ndk, Android SDK build-tools 35.0.0 and platform 36, NDK r28c, JDK 25 and Python 3.10 or newer. From the repository root:

```sh
python3 packaging/android/build_apk.py
```

The APK does not include Minecraft game files. Consult the upstream README for setup and ownership requirements. Changes to app identity, update endpoints and login integrations need careful testing before release.
