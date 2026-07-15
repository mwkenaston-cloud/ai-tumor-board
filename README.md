# AI Tumor Board

An offline desktop app (macOS + Windows) for a multi-site study of how
physicians engage with AI-generated tumor-board recommendations. A **coordinator**
builds encrypted assignment files from AI output; **reviewers** open them, record
an independent assessment & plan, and return an encrypted response; the
coordinator imports responses and exports pooled, analysis-ready data.

Built with **Tauri 2 + React/TypeScript + Rust + SQLCipher**. Fully offline; all
patient data is encrypted at rest.

## Quick start

```bash
cd ai-tumor-board
npm install
npm run tauri dev          # live development
```

Verify:

```bash
npm run build                        # frontend typecheck + bundle
cd src-tauri && cargo test --lib     # backend tests
```

## Editing → shipping

The installer is a compiled snapshot of this repo. To ship a change: edit → build
→ redistribute. See **[docs/BUILD.md](docs/BUILD.md)** and
**[docs/RELEASE_CHECKLIST.md](docs/RELEASE_CHECKLIST.md)**.

## How it works (roles)

- **Coordinator** (password `edit`): add patients, upload a combined source
  `.txt`, import AI-output JSON, assign patients to reviewers, and generate a
  per-reviewer encrypted `.atb`. Import returned `.atbr` responses; view
  aggregated findings; export analysis (JSON + CSV).
- **Physician**: open an `.atb` with its password, review each patient (timeline
  + comorbidity history, decision points, AI recommendations + A&P editor),
  complete surveys, and submit — producing a sealed `.atbr` auto-saved to
  Downloads to email back.

Full walkthrough: **[docs/USER_GUIDE.md](docs/USER_GUIDE.md)**.

## Security

- Patient data lives only in encrypted **SQLCipher** files (AES-256 + per-page
  HMAC); no plaintext DB on disk.
- Assignment keys derive from the reviewer password via **Argon2id**; response
  keys are random and **sealed to the coordinator's X25519 key**.
- No network access; strict CSP; frontend has no filesystem access.
- Coordinator capabilities are gated by an **Ed25519 credential** (development
  builds run unrestricted until the study's authority key is provisioned).

Formats: **[docs/FORMATS.md](docs/FORMATS.md)** · Threat model:
**[docs/THREAT_MODEL.md](docs/THREAT_MODEL.md)**.

> Research tool. HIPAA compliance depends on the surrounding workflow (device
> encryption, secure transport, password delivery, IRB) — see the threat model.
> Use research IDs / de-identified content whenever the protocol permits.

## Layout

```
src/                 React frontend (screens, components, models, services)
src-tauri/
  src/crypto/        Argon2id KDF, SQLCipher containers, Ed25519 gate, X25519 seal
  src/db/            schema, repository, LLM import, packaging, response, analysis
  src/commands/      typed Tauri commands (reviewer, coordinator)
  migrations/        001_initial.sql
  schemas/           llm-output.schema.json
docs/                BUILD, RELEASE_CHECKLIST, FORMATS, THREAT_MODEL, USER_GUIDE
```
