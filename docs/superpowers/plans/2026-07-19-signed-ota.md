# Signed OTA Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Every GitHub-release OTA update is Ed25519-verified (manifest signature + SHA-256 + size + target) before the firmware boots it.

**Architecture:** Verify logic in `ac-core` (host-tested), shared canonical format with a `tools/sign-manifest` signer used by CI, streaming hash check in `update.rs`, public key embedded in firmware.

**Tech Stack:** `ed25519-compact` (no default features) + `sha2` in ac-core; stable-Rust signer crate; existing esp-idf HTTP/OTA plumbing.

**Spec:** `docs/superpowers/specs/2026-07-19-signed-ota-design.md` (canonical string, manifest fields, threat model). **Beads:** cndition-control-03o.

## Global Constraints

- Canonical string: `condition-control-ota-v1\n<version>\n<target>\n<size>\n<sha256>` — exact, no trailing newline.
- Target string: `m5stickc-plus2`. OTA slot cap: `0x300000` bytes.
- Manifest JSON keys: `version`, `target`, `size`, `sha256` (lowercase hex), `sig` (base64, 64-byte Ed25519 signature).
- ac-core stays ESP-free; `cargo +stable test` must pass.
- Fail closed: missing/invalid manifest aborts the auto-update with a readable state string.
- `/api/ota` unchanged (password-protected unsigned escape hatch).

---

### Task 1: ac-core — canonical string + manifest parse/verify (TDD)

**Files:** `ac-core/Cargo.toml` (add `ed25519-compact = { version = "2", default-features = false }`, `sha2 = { version = "0.10", default-features = false }`), `ac-core/src/lib.rs`, `ac-core/tests/ota.rs` (new).

**Produces:**
- `pub struct OtaManifest { pub version: String, pub target: String, pub size: usize, pub sha256: String }`
- `pub fn ota_canonical(version: &str, target: &str, size: usize, sha256: &str) -> String`
- `pub fn verify_manifest(json: &str, pubkey: &[u8; 32]) -> Result<OtaManifest, &'static str>` — errors: `"manifest: bad json"`, `"manifest: bad sig encoding"`, `"manifest: signature invalid"`.

Tests (write first, RED): golden canonical string; round-trip with `ed25519_compact::KeyPair::from_seed(Seed::new([7u8;32]))` (dev-dependency `ed25519-compact` with defaults for signing in tests); tampered sha256/size/target/version/sig each fail with `"manifest: signature invalid"`; junk base64 → `"manifest: bad sig encoding"`; missing field → `"manifest: bad json"`. Numeric `size` extraction: scan for `"size":` then parse digits (not a quoted string — don't reuse `value_after` for it).

Commit: `ac-core: Ed25519 OTA manifest verification`.

### Task 2: ac-core — gh_asset_url (TDD)

**Files:** `ac-core/src/lib.rs`, `ac-core/tests/ota.rs`.

`pub fn gh_asset_url(json: &str, suffix: &str) -> Option<String>` — loop over `"browser_download_url"` values (same technique as `gh_release_parse`), return first URL ending with `suffix`. Refactor `gh_release_parse` to use it (keep its signature; existing tests must stay green). Tests: finds `.bin` and `manifest.json` in a two-asset fixture; None when absent.

Commit: `ac-core: gh_asset_url for locating release assets`.

### Task 3: tools/sign-manifest crate

**Files:** `tools/sign-manifest/Cargo.toml` (bin crate, deps: `ac-core = { path = "../../firmware-stick-rs/ac-core" }`, `ed25519-compact = "2"`, `sha2 = "0.10"`), `tools/sign-manifest/src/main.rs`, plus `gen-key` subcommand.

CLI:
- `sign-manifest gen-key` → prints hex secret key (64 bytes) to stdout and hex public key to stderr (so the secret can be piped to `gh secret set` without touching disk).
- `sign-manifest sign <bin> <version> <target>` → reads `OTA_SIGNING_KEY` (hex sk) from env, hashes the file, prints `manifest.json` to stdout.

Integration test in the crate: gen a seed keypair, sign a temp file, `ac_core::verify_manifest` accepts it and rejects a flipped byte.

Commit: `tools/sign-manifest: Ed25519 release signer sharing ac-core canonical format`.

### Task 4: key generation + GitHub secret + embedded pubkey

- `cargo run -p sign-manifest -- gen-key` locally; pipe secret to `gh secret set OTA_SIGNING_KEY --repo mnaza/condition-control`; NEVER print the secret into logs/chat/files.
- Public key → `firmware-stick-rs/src/update.rs`: `const OTA_PUBKEY: [u8; 32] = [...];` (hex-decoded bytes, with the hex string in a comment).

Commit: `Update: embed OTA release public key`.

### Task 5: verified update flow in update.rs

Replace the post-version-check section of `run()`:
1. `gh_asset_url(&body, "manifest.json")` → `bail!("unsigned release (no manifest)")` if None; `.bin` URL via existing parse.
2. Download manifest (cap 4 KB) → `verify_manifest(&text, &OTA_PUBKEY).map_err(|e| anyhow!(e))?`.
3. Checks: `m.target == "m5stickc-plus2"`, `m.version` equals tag (strip `v`), `m.size > 0 && m.size <= 0x30_0000` — each with its own bail message.
4. Stream loop: add `let mut hasher = sha2::Sha256::new();` + `hasher.update(&chunk[..n]);` (needs `sha2` in firmware `Cargo.toml` and `use sha2::Digest;`).
5. Before `update.complete()`: `total == m.size` and `format!("{:x}", hasher.finalize()) == m.sha256`, else `update.abort()` + bail (states: `"size mismatch"` / `"sha256 mismatch"`).
Set states: `"verifying manifest"`, `"verified vX.Y.Z"` on success path.

Gate: `cargo build --release`. Commit: `Update: verify Ed25519 manifest, size and sha256 before booting OTA image`.

### Task 6: CI signing + release + docs + version

- `ci.yml` release step: after `espflash save-image`, add
  `cargo +stable run --release --manifest-path ../tools/sign-manifest/Cargo.toml -- sign condition-control.bin "$V" m5stickc-plus2 > manifest.json` with `OTA_SIGNING_KEY: ${{ secrets.OTA_SIGNING_KEY }}` in env (install stable via `dtolnay/rust-toolchain@stable` pinned by SHA, or reuse existing stable if the runner has one), then `gh release create "v$V" condition-control.bin manifest.json ...`.
- Also run sign-manifest crate tests in the host-test CI job.
- README: signed-OTA paragraph (what's verified, escape hatch, key-loss recovery).
- Version → 0.3.11. Full gates: firmware build, all host tests.

Commit: `CI: sign releases with OTA manifest (v0.3.11)`.

### Task 7: rollout + e2e verification

1. Push → CI → release v0.3.11 (first signed release; device on 0.3.10 installs it via the old path).
2. Confirm release has BOTH assets; `curl` the manifest and locally cross-check sha256 of the released bin.
3. User updates device → 0.3.11.
4. Real verified-path proof: next release (any future version) must show `verifying manifest` → success. A negative test (tampered release) is impractical on GitHub prod; covered by host tests.
5. `bd close cndition-control-03o`; push both remotes.
