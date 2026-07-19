# Signed OTA — Design

**Date:** 2026-07-19 · **Beads:** cndition-control-03o

## Goal

The GitHub self-updater currently trusts whatever `condition-control.bin`
the latest release serves (TLS authenticates GitHub, nothing authenticates
*us*). Add an Ed25519-signed manifest so the firmware independently
verifies every over-the-air release before booting it.

## Manifest

Second release asset `manifest.json`:

```json
{"version":"0.3.11","target":"m5stickc-plus2","size":1234567,
 "sha256":"<hex of the .bin>","sig":"<base64 Ed25519 signature>"}
```

The signature covers the canonical string (JSON formatting can never
affect verification):

```
condition-control-ota-v1\n<version>\n<target>\n<size>\n<sha256>
```

## Verification (ac-core, host-tested)

New deps in `ac-core`: `ed25519-compact` (default features minus rand if
possible), `sha2`. New API:

- `pub struct OtaManifest { version, target, size: usize, sha256: String }`
- `pub fn ota_canonical(version, target, size, sha256) -> String`
- `pub fn verify_manifest(json: &str, pubkey: &[u8; 32]) -> Result<OtaManifest, &'static str>`
  — extracts fields (same string-scanning style as `gh_release_parse`),
  decodes `sig` with the existing `base64_decode`, verifies Ed25519 over
  the canonical string. Errors are static strs shown in the update UI.
- `pub fn gh_asset_url(json: &str, suffix: &str) -> Option<String>` —
  generalisation of the asset scan in `gh_release_parse`, used to find
  both `condition-control.bin` and `manifest.json`.
- Firmware-side streaming hash uses `sha2::Sha256` directly.

## Firmware flow (`update.rs`)

1. Fetch latest-release JSON (unchanged), get tag + both asset URLs.
   **No `manifest.json` asset ⇒ fail closed** ("unsigned release").
2. Version gate as today (`version_newer`) — this is the anti-rollback
   for the auto path; the manual `/api/ota` endpoint is the deliberate,
   password-protected override and stays unsigned (recovery/dev path).
3. Download the manifest (small, cap 4 KB), `verify_manifest` against the
   embedded `OTA_PUBKEY: [u8; 32]`. Additional checks: `target ==
   "m5stickc-plus2"`, manifest `version` == release tag (v-prefix
   ignored), `0 < size <= 0x300000` (OTA slot).
4. Stream the `.bin` into the inactive slot while counting bytes and
   hashing incrementally. Before `update.complete()`: exact `size` match
   and SHA-256 hex match, else `abort()` with a clear state message.

## Signing (CI)

- `tools/sign-manifest/` — small stable-Rust bin crate, path-dep on
  `ac-core` so signer and verifier share `ota_canonical` (cannot drift).
  Input: `.bin` path, version, target; key from env `OTA_SIGNING_KEY`
  (64-byte ed25519-compact secret key, hex). Output: `manifest.json`.
- CI release step: build `.bin` (unchanged) → run signer with the
  `OTA_SIGNING_KEY` repo secret → `gh release create` uploads **both**
  assets.
- Keypair generated once locally; private key goes into the GitHub
  Actions secret via `gh secret set` (never committed, never logged),
  public key committed as a const in `update.rs`.

## Threat model / accepted limits

Protects against tampered or replaced release assets, leaked
release-upload tokens, and CDN/transport corruption. Does NOT protect
against an attacker with full control of Actions secrets (they can sign),
nor against physical/USB access — out of scope. If the private key is
lost, OTA updates stop working until a new public key ships via one
manual `/api/ota` or USB flash; that is fail-closed and accepted.

## Testing

- ac-core TDD with a deterministic seed keypair: sign→verify round trip,
  tampered sig / sha256 / size / target / version, missing fields, junk
  base64, canonical-string exactness (golden string).
- sign-manifest emits JSON that `verify_manifest` accepts (integration
  test inside the tool crate).
- E2e: first signed release (v0.3.11) installs on the device via the old
  path; the *next* release proves the verified path end-to-end.
