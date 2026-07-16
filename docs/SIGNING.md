# Code signing & notarization

Physicians install this app themselves from a download or an emailed link. That
is the strictest distribution case: **unsigned builds are blocked**, not merely
warned about. macOS Gatekeeper refuses to launch an unsigned, un-notarized app
that carries the download quarantine flag, and Windows SmartScreen shows a
full-screen "unrecognized app" block that most users will not click through.

So before this goes to reviewers you need:

| Platform | Required | Roughly |
|---|---|---|
| macOS | Apple Developer Program + Developer ID Application cert + notarization | $99/yr |
| Windows | Azure Trusted Signing (recommended) or an EV code-signing cert | ~$10/mo, or ~$300–600/yr |

**The CI is already wired for both.** `.github/workflows/release.yml` signs
automatically as soon as the secrets below exist, and falls back to unsigned
builds until then. No workflow edits are needed when the certs arrive — add the
secrets and push a new tag.

> **Do this first.** Ask Mayo IT / InfoSec whether the institution already holds
> an Apple Developer Program membership and a Windows code-signing certificate.
> They very likely do, and for a multi-site study app that handles PHI you want
> it signed as *Mayo Clinic*, not as an individual. A personal Developer ID on a
> Mayo research app is a governance problem even though it is technically valid.
> Institutional certs also skip most of the enrollment lead time below.

---

## macOS

### 1. Apple Developer Program

Enroll at <https://developer.apple.com/programs/enroll> ($99/yr).

- **Organization** membership signs as "Mayo Clinic". Needs a D-U-N-S number and
  someone with legal authority to bind the institution. Days to weeks.
- **Individual** membership signs under your own name. Usually approved in 24–48h.

Prefer organization. See the note above.

### 2. Create a Developer ID Application certificate

Note the type: **Developer ID Application**, *not* "Apple Development" or "Mac App
Distribution". Only Developer ID works for apps distributed outside the App Store.

Easiest path, in Xcode:
`Settings → Accounts → (select team) → Manage Certificates → + → Developer ID Application`

Or via the web: `developer.apple.com/account/resources/certificates` → **+** →
Developer ID Application, uploading a CSR generated from
`Keychain Access → Certificate Assistant → Request a Certificate From a Certificate Authority`.

### 3. Export the certificate as `.p12`

In **Keychain Access → login → My Certificates**, find
`Developer ID Application: <Name> (<TEAMID>)`.

Expand the disclosure triangle and confirm a **private key** is nested under it —
exporting without the key produces a `.p12` that cannot sign, and this is the
single most common mistake here.

Right-click → **Export** → `.p12` → set a strong password. That password becomes
`APPLE_CERTIFICATE_PASSWORD`.

### 4. Base64-encode it for GitHub

```bash
base64 -i Certificates.p12 | pbcopy
```

That clipboard content becomes `APPLE_CERTIFICATE`.

### 5. App-specific password (for notarization)

At <https://appleid.apple.com> → **Sign-In and Security → App-Specific Passwords**
→ generate one. This becomes `APPLE_PASSWORD`.

This is **not** your Apple ID password. Your real Apple ID password will not work
and should never be put in a secret.

### 6. Team ID and identity string

```bash
security find-identity -v -p codesigning
```

Copy the full quoted name, e.g. `Developer ID Application: Mayo Clinic (ABCDE12345)`
→ `APPLE_SIGNING_IDENTITY`. The 10-character code in parentheses is your
`APPLE_TEAM_ID` (also shown at `developer.apple.com/account` → Membership).

### 7. Add the GitHub secrets

**Settings → Secrets and variables → Actions → New repository secret**:

| Secret | Value |
|---|---|
| `APPLE_CERTIFICATE` | base64 of the `.p12` (step 4) |
| `APPLE_CERTIFICATE_PASSWORD` | the `.p12` export password (step 3) |
| `APPLE_SIGNING_IDENTITY` | `Developer ID Application: … (TEAMID)` |
| `APPLE_ID` | your Apple ID email |
| `APPLE_PASSWORD` | app-specific password (step 5) |
| `APPLE_TEAM_ID` | 10-character Team ID |

All six must be present or the workflow stays on the unsigned path (it keys off
`APPLE_CERTIFICATE` + `APPLE_SIGNING_IDENTITY`).

Tauri signs with the **hardened runtime** automatically, which notarization
requires. The first notarized build adds ~5–15 minutes while Apple's service
processes the upload.

---

## Windows

Since June 2023, standard OV code-signing certificates require the private key to
live on a hardware token or cloud HSM, which makes plain CI signing awkward.
Two workable options:

### Option A — Azure Trusted Signing (recommended)

Purpose-built for CI, no hardware token, ~$9.99/month.

1. **Azure subscription** (Mayo likely has one — ask IT for a resource group).
2. **Create a Trusted Signing Account** in the Azure portal. Note its region
   endpoint, e.g. `https://eus.codesigning.azure.net`.
3. **Identity Validation** → Public Trust. Requires legal business documentation.
   Organizations 3+ years old qualify for the streamlined path; Mayo qualifies
   comfortably. Typically 1–7 business days.
4. **Create a Certificate Profile** (type: Public Trust).
5. **Service principal for CI**: Entra ID → App registrations → New registration
   → add a client secret. Then assign it the **Trusted Signing Certificate
   Profile Signer** role on the Trusted Signing account.
6. Add the secrets:

| Secret | Value |
|---|---|
| `AZURE_CLIENT_ID` | service principal application (client) ID |
| `AZURE_CLIENT_SECRET` | client secret value |
| `AZURE_TENANT_ID` | directory (tenant) ID |
| `AZURE_ENDPOINT` | e.g. `https://eus.codesigning.azure.net` |
| `AZURE_CODE_SIGNING_NAME` | Trusted Signing account name |
| `AZURE_CERT_PROFILE_NAME` | certificate profile name |

The workflow then builds with `src-tauri/tauri.windows-signed.conf.json`, which
points Tauri's bundler at `trusted-signing-cli` to sign each artifact.

> **Verify when you set this up:** `trusted-signing-cli` reads the endpoint,
> account, and profile from `AZURE_ENDPOINT` / `AZURE_CODE_SIGNING_NAME` /
> `AZURE_CERT_PROFILE_NAME`. Confirm those names against the tool's current
> README — this path could not be tested without a Trusted Signing account, so
> treat the first signed Windows run as something to watch rather than assume.

### Option B — EV certificate

From DigiCert, Sectigo, etc. (~$300–600/yr). Ships on a hardware token or needs
a cloud HSM, so CI integration means either a self-hosted Windows runner with the
token attached, or the vendor's cloud signing service wired into
`bundle.windows.signCommand`.

Main advantage: **immediate SmartScreen reputation**. Trusted Signing certs build
reputation over download volume, so early users may still see a warning.

---

## Verify before you distribute

Do not trust "CI was green" — a build that silently didn't sign looks identical
in the logs and fails on the physician's machine. The workflow runs
`codesign --verify` and `spctl --assess` on signed macOS builds, but verify the
real downloaded artifact too:

**macOS** — on a Mac that never built the app, download the `.dmg` through a
browser so it carries the quarantine flag, then:

```bash
spctl --assess --type execute -vv "/Applications/AI Tumor Board.app"
# want: accepted / source=Notarized Developer ID
```

**Windows** — on a clean machine:

```powershell
signtool verify /pa /v "AI Tumor Board_0.9.0_x64-setup.exe"
```

Or right-click the installer → **Properties → Digital Signatures**.

The real test is a clean VM with no developer tooling: download, install, launch,
open an `.atb`. That is what the physician experiences.

---

## Ongoing

- **Expiry.** Developer ID certs last 5 years; Trusted Signing rotates
  automatically. Already-notarized macOS builds keep working after the cert
  expires; you just can't sign new ones.
- **Secret hygiene.** The `.p12`, its password, and the client secret are signing
  credentials for software Mayo distributes. They live in GitHub Secrets and
  nowhere else — never in the repo, never in a commit, never in a chat.
- **Rotation.** If a signing secret leaks, revoke the cert at Apple / rotate the
  service principal immediately. Anything signed with it can impersonate this app.
- **Also still outstanding for production:** `AUTHORITY_PUBLIC_KEY_HEX` in
  `crypto/role_credential.rs` is the all-zero placeholder, which leaves the
  coordinator role gate in development mode. Provision the study's real Ed25519
  key before the production build — see `BUILD.md`.
