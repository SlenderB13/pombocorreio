# Releasing Pombo Correio

Desktop releases are built and published by GitHub Actions when a version tag is pushed.

## Create a release

1. Update every project version at once:

   ```bash
   npm run release:version -- 0.2.0
   ```

2. Review and verify the release:

   ```bash
   git diff
   npm run format:check
   npm run build
   cargo test --manifest-path src-tauri/Cargo.toml
   ```

3. Commit and push the version:

   ```bash
   git add .
   git commit -m "Release v0.2.0"
   git push origin main
   ```

4. Create and push the matching tag:

   ```bash
   git tag -a v0.2.0 -m "Pombo Correio 0.2.0"
   git push origin v0.2.0
   ```

The release workflow builds Debian, RPM, Arch Linux, MSI, and NSIS installers. Once every build
succeeds, it creates the GitHub Release, generates release notes, and uploads all installers plus
`SHA256SUMS.txt`.

If any version differs from the tag, the workflow stops before building or publishing anything.
