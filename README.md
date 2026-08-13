# Pombo Correio

Pombo Correio is a local-network file sender for Windows, Linux, and Android. This repository contains the first runnable Tauri 2 vertical slice.

## What works now

- Windows/Linux tray menu and close-to-tray behavior
- Drag files into the send window
- Discover other running instances with mDNS
- Offer one or more files to one or more nearby devices
- Require approval for a device's first incoming transfer
- Remember an accepted sender for subsequent transfers
- Stream files into `Downloads/Pombo Correio`
- Avoid overwriting files with the same name

The receiver application must currently be running. Android background receiving, transfer progress, resume, cryptographic device identities, and encrypted transport are later milestones.

## Security status

This is a LAN prototype, not a security-complete release. Trust records currently use generated device IDs and transfers use plain HTTP. Use it only on a network you trust. Before public distribution, device IDs must be replaced with cryptographic identities and the transport must authenticate and encrypt both peers.

## Desktop development

Requirements: Node.js, Rust, and the platform dependencies listed by Tauri.

```sh
npm install
npm run tauri dev
```

Checks:

```sh
npm run build
cargo test --manifest-path src-tauri/Cargo.toml
```

To test discovery and transfer locally, run the app on two machines on the same LAN. Some routers isolate wireless clients, and host firewalls may require allowing the application on private networks.

## Android setup

Install Android Studio, an Android SDK, NDK, and the Rust Android targets required by Tauri. Set `ANDROID_HOME` to the SDK directory, then initialize the generated Android project:

```sh
npm run tauri android init
npm run tauri android dev
```

The current Tauri/Rust core is mobile-capable, but the Android target has not been initialized in this checkout because `ANDROID_HOME` is not configured in the development environment.

## Protocol sketch

Instances advertise `_pombocorreio._tcp.local.` with a device ID and display name. A sender posts a transfer offer, polls until it is accepted or declined, then streams each accepted file. The protocol is intentionally small so that a future native Android receiver or another compatible client can implement it.
