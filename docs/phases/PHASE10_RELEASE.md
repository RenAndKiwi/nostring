# Phase 10: Build & Tag Release Binaries

**Goal:** Ship signed, notarized release binaries for macOS (arm64 + x64), Windows (x64), and Linux (x64) via GitHub Releases, with a clear path to auto-update.

**Status:** Planning

---

## Table of Contents

1. [Current State Assessment](#1-current-state-assessment)
2. [Release Workflow Fixes](#2-release-workflow-fixes)
3. [Code Signing & Notarization](#3-code-signing--notarization)
4. [v0.2.0 Release Plan](#4-v020-release-plan)
5. [Auto-Update Mechanism](#5-auto-update-mechanism)
6. [Security Review](#6-security-review)
7. [Implementation Roadmap](#7-implementation-roadmap)

---

## 1. Current State Assessment

### What Exists

**Release workflow** (`.github/workflows/release.yml`):
- Triggers on `v*` tags and `workflow_dispatch`
- Builds 4 targets: macOS arm64, macOS x64, Linux x64, Windows x64
- Packages raw binaries (tar.gz / zip) — NOT Tauri bundles
- Creates SHA256SUMS.txt
- Creates a **draft** GitHub Release

**CI workflow** (`.github/workflows/ci.yml`):
- Lint, test, security audit, cargo-deny on push/PR to main
- Excludes `nostring-app` from builds (no webkit deps on CI runner)

### Critical Issues

| Issue | Severity | Detail |
|-------|----------|--------|
| **Not using Tauri bundler** | 🔴 High | Workflow runs `cargo build` directly, producing a bare binary instead of `.app`/`.dmg`/`.msi`/`.AppImage`. Users get a raw executable with no icons, no installer, no OS integration. |
| **Bundle is disabled** | 🔴 High | `tauri.conf.json` has `"bundle": { "active": false }`. Must be enabled. |
| **No icons configured** | 🟡 Medium | `"icon": []` in tauri.conf.json. Tauri requires icons for bundling. |
| **No code signing** | 🟡 Medium | macOS will show "unidentified developer" warning. Windows SmartScreen will block. |
| **No notarization** | 🟡 Medium | macOS Gatekeeper will quarantine the app. |
| **Binary name mismatch** | 🟡 Medium | Workflow assumes `nostring-app` binary name, but Tauri may produce `NoString` (from `productName`). |
| **No frontend build step** | 🔴 High | Workflow skips the frontend entirely. Tauri needs the frontend built into `frontendDist`. |
| **Version is 0.1.0** | 🟢 Low | Workspace and tauri.conf.json both say 0.1.0. Need to bump for release. |

### Verdict

The current release.yml **will not produce working distributable binaries**. It needs a near-complete rewrite to use `tauri-apps/tauri-action` or `cargo tauri build` properly.

---

## 2. Release Workflow Fixes

### 2.1 Switch to Tauri Action

Replace the manual `cargo build` + packaging with the official `tauri-apps/tauri-action@v0`, which handles:
- Frontend build
- Tauri bundling (`.app`, `.dmg`, `.msi`, `.deb`, `.AppImage`)
- Code signing (when env vars are set)
- Notarization (when Apple credentials are provided)
- Artifact upload
- GitHub Release creation

### 2.2 Proposed Workflow Structure

```yaml
name: Release

on:
  push:
    tags: ['v*']
  workflow_dispatch:

jobs:
  build-macos:
    runs-on: macos-latest
    strategy:
      matrix:
        include:
          - args: '--target aarch64-apple-darwin'
            artifact: nostring-macos-arm64
          - args: '--target x86_64-apple-darwin'
            artifact: nostring-macos-x64
    steps:
      - uses: actions/checkout@v4
      - name: Install Rust
        uses: dtolnay/rust-toolchain@stable
        with:
          targets: aarch64-apple-darwin,x86_64-apple-darwin
      - name: Import Apple Certificate  # ONLY when signing is set up
        # ... certificate import steps ...
      - uses: tauri-apps/tauri-action@v0
        env:
          GITHUB_TOKEN: ${{ secrets.GITHUB_TOKEN }}
          # Apple signing env vars (when ready)
        with:
          tagName: v__VERSION__
          releaseName: 'NoString v__VERSION__'
          releaseBody: 'See CHANGELOG.md for details.'
          releaseDraft: true
          args: ${{ matrix.args }}

  build-linux:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - name: Install dependencies
        run: |
          sudo apt-get update
          sudo apt-get install -y libwebkit2gtk-4.1-dev libappindicator3-dev librsvg2-dev patchelf
      - name: Install Rust
        uses: dtolnay/rust-toolchain@stable
      - uses: tauri-apps/tauri-action@v0
        env:
          GITHUB_TOKEN: ${{ secrets.GITHUB_TOKEN }}
        with:
          tagName: v__VERSION__
          releaseName: 'NoString v__VERSION__'
          releaseDraft: true

  build-windows:
    runs-on: windows-latest
    steps:
      - uses: actions/checkout@v4
      - name: Install Rust
        uses: dtolnay/rust-toolchain@stable
      - name: Import Windows Certificate  # ONLY when signing is set up
        # ... certificate import steps ...
      - uses: tauri-apps/tauri-action@v0
        env:
          GITHUB_TOKEN: ${{ secrets.GITHUB_TOKEN }}
          # Windows signing env vars (when ready)
        with:
          tagName: v__VERSION__
          releaseName: 'NoString v__VERSION__'
          releaseDraft: true

  checksums:
    needs: [build-macos, build-linux, build-windows]
    runs-on: ubuntu-latest
    steps:
      - name: Download all artifacts
        uses: actions/download-artifact@v4
      - name: Create SHA256SUMS
        run: find . -type f \( -name "*.dmg" -o -name "*.app.tar.gz" -o -name "*.AppImage" -o -name "*.deb" -o -name "*.msi" -o -name "*.exe" \) -exec sha256sum {} \; > SHA256SUMS.txt
      - name: Upload checksums
        # Attach SHA256SUMS.txt to the draft release
```

### 2.3 Required tauri.conf.json Changes

```json
{
  "bundle": {
    "active": true,
    "targets": ["dmg", "app", "appimage", "deb", "msi", "nsis"],
    "icon": [
      "icons/32x32.png",
      "icons/128x128.png",
      "icons/128x128@2x.png",
      "icons/icon.icns",
      "icons/icon.ico"
    ]
  }
}
```

### 2.4 Pre-Release Checklist

- [ ] Generate proper application icons (all required sizes)
- [ ] Enable bundling in `tauri.conf.json`
- [ ] Ensure frontend builds (the `frontendDist` directory is populated)
- [ ] Test `cargo tauri build` locally on macOS
- [ ] Run `workflow_dispatch` to test CI before tagging

---

## 3. Code Signing & Notarization

### 3.1 macOS: Code Signing + Notarization

**Why it matters:**
- Without signing: macOS shows "app is damaged and can't be opened" (Gatekeeper quarantine)
- Without notarization: Even signed apps get a scary "unidentified developer" warning
- Users must right-click → Open to bypass — terrible UX for a security app

**Requirements:**
- Apple Developer Program membership ($99/year)
- "Developer ID Application" certificate (for distribution outside App Store)
- App-specific password or App Store Connect API key (for notarization)

**GitHub Secrets needed:**
| Secret | Description |
|--------|-------------|
| `APPLE_CERTIFICATE` | Base64-encoded .p12 certificate |
| `APPLE_CERTIFICATE_PASSWORD` | Export password for the .p12 |
| `APPLE_SIGNING_IDENTITY` | Certificate common name (e.g., "Developer ID Application: Name (TEAMID)") |
| `APPLE_ID` | Apple ID email |
| `APPLE_PASSWORD` | App-specific password (NOT Apple ID password) |
| `APPLE_TEAM_ID` | 10-character team identifier |
| `KEYCHAIN_PASSWORD` | Arbitrary password for CI keychain |

**Or use App Store Connect API (preferred for CI):**
| Secret | Description |
|--------|-------------|
| `APPLE_API_ISSUER` | Issuer ID from App Store Connect |
| `APPLE_API_KEY` | Key ID |
| `APPLE_API_KEY_PATH` | Path to the .p8 private key file |

**Decision needed:** Whether to invest $99/year now or ship unsigned initially.

**Recommendation:** Ship v0.2.0 **unsigned** with clear README instructions for bypassing Gatekeeper. Invest in signing for v0.3.0 when approaching beta/public launch. Rationale:
- Alpha users (technical Bitcoiners) can handle right-click → Open
- Saves time and cost during rapid iteration
- Code signing doesn't add security for the _user's data_ (that's our crypto layer)
- But plan for it — don't make architectural choices that block signing later

### 3.2 Windows: Code Signing

**Why it matters:**
- Without signing: SmartScreen shows "Windows protected your PC" warning
- Users must click "More info" → "Run anyway" — bad UX
- SmartScreen reputation builds over time with an EV certificate

**Certificate types:**
| Type | Cost | SmartScreen | Notes |
|------|------|-------------|-------|
| OV (Organization Validated) | ~$200-400/year | Gradual reputation | Cheaper, software-based |
| EV (Extended Validation) | ~$400-700/year | Immediate trust | Requires hardware token (USB) or Azure Key Vault |

**GitHub Secrets needed (OV):**
| Secret | Description |
|--------|-------------|
| `WINDOWS_CERTIFICATE` | Base64-encoded .pfx certificate |
| `WINDOWS_CERTIFICATE_PASSWORD` | PFX export password |

**For Azure Key Vault (EV):**
| Secret | Description |
|--------|-------------|
| `AZURE_KEY_VAULT_URI` | Key vault URL |
| `AZURE_CLIENT_ID` | Service principal client ID |
| `AZURE_CLIENT_SECRET` | Service principal secret |
| `AZURE_TENANT_ID` | Azure AD tenant ID |

**tauri.conf.json additions (when ready):**
```json
{
  "bundle": {
    "windows": {
      "certificateThumbprint": "...",
      "digestAlgorithm": "sha256",
      "timestampUrl": "http://timestamp.sectigo.com"
    }
  }
}
```

**Recommendation:** Same as macOS — ship unsigned for v0.2.0. Add OV certificate for v0.3.0. Consider EV for v1.0.

### 3.3 Linux: No Signing Required

Linux does not have a centralized code signing requirement. Users install via:
- `.AppImage` (download and run)
- `.deb` (dpkg install)

Optional: GPG-sign the release artifacts for users who want to verify. This is cheap and easy:
```bash
gpg --detach-sign --armor nostring-linux-x64.AppImage
```

**Recommendation:** Include GPG signatures from v0.2.0. Generate a project GPG key and publish the public key in the README.

---

## 4. v0.2.0 Release Plan

### 4.1 Version Bump

Update version from `0.1.0` → `0.2.0` in:
- `Cargo.toml` (workspace `[workspace.package]` version)
- `tauri-app/src-tauri/tauri.conf.json`

**Versioning scheme:** Semantic Versioning
- `0.x.y` — Pre-1.0 (breaking changes allowed on minor bumps)
- `0.2.0` — First distributable release (all phases 0-9 complete)
- `0.3.0` — Code signing + auto-update
- `1.0.0` — Security audited, production-ready

### 4.2 Release Notes Template

```markdown
# NoString v0.2.0 — First Public Alpha

**Bitcoin inheritance without trusted third parties.**

## What's New
- 🔑 Unified seed: BIP-39 mnemonic derives both Nostr and Bitcoin keys
- ⏱️ Timelock inheritance: Miniscript policies with cascade timelocks
- 🔐 Shamir backup: SLIP-39 and Codex32 (BIP-93) share splitting
- 👥 Multi-heir support: Spouse → children → executor cascades
- 📡 Watch-only mode: Import xpub, never expose keys
- 🔔 Notifications: Nostr DM + email check-in reminders
- 📊 Spend detection: Know if an heir claimed funds
- 🖥️ Desktop app: Tauri-based UI for macOS, Windows, Linux
- 🖧 Self-hosting: nostring-server daemon (Docker)

## ⚠️ Alpha Software
This is pre-release software. DO NOT use with mainnet funds you cannot
afford to lose. Test on testnet/signet first.

## ⚠️ Unsigned Binaries
These binaries are NOT code-signed. You may see security warnings:
- **macOS:** Right-click the app → Open → Open (bypasses Gatekeeper)
- **Windows:** Click "More info" → "Run anyway" (bypasses SmartScreen)
- **Linux:** `chmod +x NoString.AppImage && ./NoString.AppImage`

Verify integrity via SHA256SUMS.txt before running.

## Downloads
| Platform | File | SHA256 |
|----------|------|--------|
| macOS (Apple Silicon) | `NoString_0.2.0_aarch64.dmg` | `...` |
| macOS (Intel) | `NoString_0.2.0_x64.dmg` | `...` |
| Windows | `NoString_0.2.0_x64-setup.exe` | `...` |
| Linux (AppImage) | `NoString_0.2.0_amd64.AppImage` | `...` |
| Linux (deb) | `NoString_0.2.0_amd64.deb` | `...` |

## Verification
sha256sum -c SHA256SUMS.txt

## Test Coverage
232 tests passing across 11 crates.
```

### 4.3 Tag & Release Process

```bash
# 1. Ensure main is clean and CI passes
git checkout main
git pull

# 2. Bump versions
# Edit Cargo.toml workspace version → "0.2.0"
# Edit tauri.conf.json version → "0.2.0"

# 3. Update CHANGELOG.md (create if not exists)

# 4. Commit version bump
git add -A
git commit -m "chore: bump version to 0.2.0"
git push

# 5. Tag and push (triggers release.yml)
git tag -a v0.2.0 -m "NoString v0.2.0 — First Public Alpha"
git push origin v0.2.0

# 6. Wait for CI to complete
# 7. Review draft release on GitHub
# 8. Edit release notes, attach SHA256SUMS
# 9. Publish release (un-draft)
```

---

## 5. Auto-Update Mechanism

### 5.1 Tauri Updater Plugin Overview

Tauri v2 includes `@tauri-apps/plugin-updater` which supports:
- **Signature verification** — mandatory, uses Ed25519 keypair
- **Static JSON endpoint** — host a `latest.json` on GitHub Releases or S3
- **Dynamic server** — API that checks version and returns update info
- **Platform-aware** — `{{target}}`, `{{arch}}`, `{{current_version}}` template variables

### 5.2 Update Signing (Separate from Code Signing)

Tauri update signing is **independent** of OS code signing. It uses a dedicated keypair:

```bash
# Generate update signing keys
cargo tauri signer generate -w ~/.tauri/nostring.key
# Outputs: nostring.key (private) and nostring.key.pub (public)
```

- **Public key** → embedded in `tauri.conf.json` (shipped with every build)
- **Private key** → stored as GitHub Secret (`TAURI_SIGNING_PRIVATE_KEY`)
- This ensures users only install updates signed by us, even if GitHub is compromised

### 5.3 Update Endpoint Strategy

**Option A: GitHub Releases (simplest, recommended for alpha/beta)**
```json
{
  "plugins": {
    "updater": {
      "pubkey": "dW50cnVzdGVkIGNvbW1lbnQ...",
      "endpoints": [
        "https://github.com/nostring/nostring/releases/latest/download/latest.json"
      ]
    }
  }
}
```

The CI workflow generates a `latest.json` file and attaches it to each release:
```json
{
  "version": "0.3.0",
  "notes": "Bug fixes and performance improvements",
  "pub_date": "2026-02-15T00:00:00Z",
  "platforms": {
    "darwin-aarch64": {
      "signature": "...",
      "url": "https://github.com/nostring/nostring/releases/download/v0.3.0/NoString.app.tar.gz"
    },
    "darwin-x86_64": { ... },
    "linux-x86_64": { ... },
    "windows-x86_64": { ... }
  }
}
```

**Option B: Self-hosted endpoint (future, for privacy)**
- Host on nostring.dev or the user's own nostring-server instance
- Allows checking update without hitting GitHub (privacy)
- More complex, defer to post-v1.0

### 5.4 User Experience

- On app launch, check for updates silently
- Show non-intrusive banner: "NoString v0.3.0 is available. [Update Now] [Later]"
- User clicks "Update Now" → download + verify signature → restart
- **Never auto-install without user consent** — this is a security-critical app

### 5.5 Implementation Timeline

| Version | Auto-Update Status |
|---------|-------------------|
| v0.2.0 | No auto-update. Manual download from GitHub Releases. |
| v0.3.0 | Add updater plugin with GitHub Releases endpoint. |
| v0.4.0+ | Refine UX, add self-hosted endpoint option. |

---

## 6. Security Review

### 6.1 Supply Chain Risks

| Risk | Severity | Mitigation |
|------|----------|------------|
| Compromised CI runner | 🔴 High | Use GitHub-hosted runners only. Pin action versions with SHA hashes. |
| Dependency injection | 🔴 High | `cargo audit` in CI. `Cargo.lock` committed. Pin crate versions. |
| Malicious GitHub Action | 🟡 Medium | Use only official actions (`actions/*`, `tauri-apps/*`). Pin to commit SHA. |
| Compromised signing key | 🔴 High | Store in GitHub Secrets (encrypted). Rotate annually. Backup offline. |
| Tag manipulation | 🟡 Medium | Use signed tags (`git tag -s`). Require branch protection on main. |
| Binary tampering post-build | 🟡 Medium | SHA256SUMS.txt in release. GPG-sign checksums file. |

### 6.2 Code Signing Security Considerations

**For a Bitcoin security tool, unsigned binaries are a meaningful risk:**
- Users downloading from GitHub could get a MITM'd binary
- SHA256 verification helps but most users won't do it
- Code signing provides an additional verification layer

**However:**
- Code signing proves *identity* (who built it), not *safety* (what it does)
- For an open-source project, reproducible builds are arguably more valuable
- Our real security is the cryptographic layer (Argon2id, AES-256-GCM, miniscript)

### 6.3 Reproducible Builds (Future Goal)

For a Bitcoin security tool, reproducible builds are the gold standard:
- Anyone can verify the binary matches the source code
- Eliminates trust in the build environment
- Used by Bitcoin Core, Sparrow Wallet, etc.

**Approach (post-v1.0):**
1. Nix-based build environment for determinism
2. Document build reproduction steps
3. Multiple independent builders verify checksums match
4. Publish Gitian/Guix build signatures

### 6.4 Update Signing vs Code Signing

| Aspect | Code Signing (OS) | Update Signing (Tauri) |
|--------|-------------------|------------------------|
| Purpose | OS trusts the binary | App trusts the update |
| Authority | Apple / Microsoft CA | Our Ed25519 keypair |
| Compromise impact | Attacker can sign any app | Attacker can push malicious updates |
| Rotation | Tied to developer account | We control the keypair |
| Required? | No (but UX suffers) | Yes (Tauri enforces it) |

**Key insight:** Tauri's update signing is more important than OS code signing for our threat model. A compromised update key could push malicious code to all users. Store the private key with extreme care.

### 6.5 Secret Management for CI

**Required GitHub Secrets (v0.2.0 — unsigned):**
| Secret | Purpose |
|--------|---------|
| `GITHUB_TOKEN` | Auto-provided, creates releases |

**Required GitHub Secrets (v0.3.0 — signed + updater):**
| Secret | Purpose |
|--------|---------|
| `APPLE_CERTIFICATE` | macOS code signing |
| `APPLE_CERTIFICATE_PASSWORD` | macOS code signing |
| `APPLE_ID` | macOS notarization |
| `APPLE_PASSWORD` | macOS notarization (app-specific password) |
| `APPLE_TEAM_ID` | macOS notarization |
| `KEYCHAIN_PASSWORD` | CI keychain for macOS |
| `WINDOWS_CERTIFICATE` | Windows code signing |
| `WINDOWS_CERTIFICATE_PASSWORD` | Windows code signing |
| `TAURI_SIGNING_PRIVATE_KEY` | Update signature key |
| `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | Update signature password |

### 6.6 Action Pinning

Pin all GitHub Actions to full commit SHAs, not version tags:
```yaml
# Good
- uses: actions/checkout@b4ffde65f46336ab88eb53be808477a3936bae11 # v4.1.1

# Bad (tag can be moved)
- uses: actions/checkout@v4
```

---

## 7. Implementation Roadmap

### Step 1: Fix Tauri Bundling Prerequisites (v0.2.0-prep)
- [ ] Generate proper icons (all sizes: 32x32, 128x128, 128x128@2x, .icns, .ico)
- [ ] Set `"bundle": { "active": true }` in tauri.conf.json
- [ ] Configure bundle targets: `["dmg", "app", "appimage", "deb", "nsis"]`
- [ ] Set icon paths in tauri.conf.json
- [ ] Verify `cargo tauri build` works locally on macOS
- [ ] Verify frontend is built and accessible at `frontendDist` path

### Step 2: Rewrite release.yml (v0.2.0-prep)
- [ ] Replace manual cargo build with `tauri-apps/tauri-action@v0`
- [ ] Split into per-platform jobs (macOS, Linux, Windows)
- [ ] Add SHA256SUMS generation
- [ ] Add `workflow_dispatch` for testing without tagging
- [ ] Pin all action versions to commit SHAs
- [ ] Test with `workflow_dispatch` before tagging

### Step 3: Version Bump & Tag (v0.2.0)
- [ ] Bump workspace version to 0.2.0
- [ ] Bump tauri.conf.json version to 0.2.0
- [ ] Create CHANGELOG.md
- [ ] Draft release notes
- [ ] Create signed tag: `git tag -s v0.2.0`
- [ ] Push tag, verify CI produces working binaries
- [ ] Download and test binaries on each platform
- [ ] Publish release (un-draft)

### Step 4: Code Signing Setup (v0.3.0)
- [ ] Enroll in Apple Developer Program ($99/year)
- [ ] Generate "Developer ID Application" certificate
- [ ] Export .p12, configure GitHub Secrets
- [ ] Acquire OV code signing certificate for Windows (~$200-400/year)
- [ ] Configure Windows signing in tauri.conf.json
- [ ] Generate GPG key for Linux artifact signing
- [ ] Update release.yml with signing steps
- [ ] Test signed builds on all platforms

### Step 5: Auto-Update (v0.3.0)
- [ ] Generate Tauri update signing keypair
- [ ] Store private key as GitHub Secret
- [ ] Add public key to tauri.conf.json
- [ ] Install `@tauri-apps/plugin-updater` in Tauri app
- [ ] Configure `createUpdaterArtifacts: true`
- [ ] Set endpoint to GitHub Releases `latest.json`
- [ ] Add CI step to generate/upload `latest.json`
- [ ] Implement update check UI (banner, not modal)
- [ ] Test update flow: v0.3.0 → v0.3.1

### Step 6: Hardening (v0.4.0+)
- [ ] Pin GitHub Actions to commit SHAs
- [ ] Add SBOM (Software Bill of Materials) generation
- [ ] Investigate reproducible builds (Nix)
- [ ] Add binary transparency log
- [ ] Consider self-hosted update endpoint

---

## Cost Summary

| Item | Cost | When |
|------|------|------|
| GitHub Actions | Free (public repo) | v0.2.0 |
| Apple Developer Program | $99/year | v0.3.0 |
| OV Code Signing Cert (Windows) | ~$200-400/year | v0.3.0 |
| GPG Key | Free | v0.2.0 |
| Domain (nostring.dev) | ~$12/year | v0.2.0 |
| **Total Year 1** | **~$310-510** | |

---

## Decision Log

| Decision | Choice | Rationale |
|----------|--------|-----------|
| v0.2.0 signing | Unsigned | Alpha users are technical; save cost during rapid iteration |
| Bundle format | DMG + AppImage + NSIS | Standard for each platform, good UX |
| Update mechanism | Tauri updater plugin | Built-in, signature-verified, well-tested |
| Update endpoint | GitHub Releases static JSON | Simplest, free, reliable for alpha/beta |
| Windows cert type | OV (initially) | EV too expensive for early stage; OV builds reputation over time |
| macOS distribution | Outside App Store | App Store review process too restrictive for Bitcoin tools |

---

*Created: 2026-02-03*
*Phase 10 planning complete. Implementation blocked on Step 1 (icon generation + tauri.conf.json fixes).*
