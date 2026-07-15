# Release checklist

A repeatable "make an edit → ship a new build" loop.

## 1. Prepare
- [ ] All changes committed; working tree clean (`git status`).
- [ ] `npm run build` clean (frontend typecheck + bundle).
- [ ] `cd src-tauri && cargo test --lib` all green.
- [ ] If the DB schema or `.atb`/`.atbr` format changed: `SCHEMA_VERSION` /
      `FORMAT_VERSION` bumped **and** a migration/compat path exists for files
      already distributed. If not, note that old files won't open.

## 2. Version bump
Keep these three in sync:
- [ ] `src-tauri/tauri.conf.json` → `version`
- [ ] `src-tauri/Cargo.toml` → `[package] version`
- [ ] `package.json` → `version`
- [ ] Commit + tag: `git commit -am "release vX.Y.Z" && git tag vX.Y.Z`

## 3. Provisioning (production only)
- [ ] Real study-authority Ed25519 public key set in
      `crypto/role_credential.rs` (`AUTHORITY_PUBLIC_KEY_HEX`) — not the all-zero
      placeholder — if the coordinator role gate must be enforced.
- [ ] Signing credentials available (Apple Developer ID + notarization; Windows
      code-signing cert).

## 4. Build
- [ ] macOS: `npm run tauri build -- --target universal-apple-darwin` (signed +
      notarized).
- [ ] Windows (on Windows): `npm run tauri build` (signed).
- [ ] Artifacts present in `src-tauri/target/release/bundle/`.

## 5. Smoke test the installed build (both OSes if shipping both)
- [ ] Installs and launches on a clean machine.
- [ ] Coordinator (password `edit`): add patient → upload combined `.txt` →
      import AI JSON → assign to a reviewer → build `.atb`.
- [ ] Physician: open the `.atb` with its password → review → complete →
      surveys → submit → response auto-saved to Downloads.
- [ ] Coordinator: import the `.atbr` → appears in Responses/aggregate; export
      analysis (JSON + CSV) writes both files.
- [ ] Wrong password on the `.atb` is rejected with no data shown.
- [ ] No network traffic during normal use (offline check).

## 6. Distribute
- [ ] Ship the installer via a secure institutional channel (managed deployment
      preferred over email).
- [ ] Send each reviewer their `.atb` and its password through **separate**
      channels.

## Acceptance gate (do not ship if any fail)
- No entered text lost after a force-kill mid-review.
- Every saved assignment reopens.
- Wrong passwords reveal nothing; tampered files fail integrity.
- Responses import identically on macOS and Windows.
- Exports pass their schema; analysis CSV/JSON open in analysis tools.
- No PHI in logs, filenames, temp dirs, or recent-files.
- Submitted assignments are read-only.
