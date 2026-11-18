# Code signing

Unsigned downloads make Windows SmartScreen say "Windows protected your PC" and make macOS refuse to open Ferry until the user clears a quarantine flag. Signing removes both. The release workflow ([`.github/workflows/release.yml`](../.github/workflows/release.yml)) already has every signing step; each one turns on when its secrets are added to the repository, and until then releases build unsigned with a warning in the log.

| Platform | What signs it | Cost | Status |
|---|---|---|---|
| All | GitHub build provenance attestation + `SHA256SUMS` | Free | On for every release |
| Windows | [Azure Artifact Signing](https://learn.microsoft.com/en-us/azure/artifact-signing/quickstart) (formerly Trusted Signing) | Monthly Azure fee for the Basic tier; check the [pricing calculator](https://azure.microsoft.com/en-us/pricing/calculator/?service=artifact-signing) | Needs the secrets below |
| macOS | Apple Developer ID certificate + notarization | Apple Developer Program, US$99 a year | Needs the secrets below |
| Linux | No OS-level signing; the provenance attestation covers it | Free | On |

## Anyone can verify a release today

Every archive on the Releases page has a provenance attestation that proves it was built by this repository's release workflow, from the tagged commit:

```sh
gh attestation verify ferry-v0.1.0-x86_64-pc-windows-msvc.zip -R KabirRao1/ferry
```

`SHA256SUMS` on the same page lists every archive's checksum (`sha256sum -c SHA256SUMS` on Linux, `shasum -a 256 -c SHA256SUMS` on macOS).

## Windows: Azure Artifact Signing

Individual developers must live in the **United States or Canada**; elsewhere, an organization can apply instead, or see [the free alternative](#free-alternative-for-windows-signpath) below.

1. **Azure subscription.** Create one at [portal.azure.com](https://portal.azure.com). For individual validation, the subscription's billing account must be of type *Individual*, with your legal name and address exactly as on your government ID.
2. **Register the provider.** Subscriptions → your subscription → Resource providers → `Microsoft.CodeSigning` → Register.
3. **Create the signing account.** Search for *Artifact Signing Accounts* → Create. Pick a region (for example East US), a globally unique name, and the **Basic** tier. Note the account name and the region's endpoint (East US is `https://eus.codesigning.azure.net`; the [quickstart](https://learn.microsoft.com/en-us/azure/artifact-signing/quickstart) lists the others).
4. **Validate your identity.** In the account: Access control (IAM) → give yourself the *Artifact Signing Identity Verifier* role. Then Identity validations → Individual → New identity → Public, and follow the Verified ID steps (a photo of your ID on your phone, then Microsoft Authenticator). Your name, city, state and country appear on the certificate; your street address and email do not.
5. **Create a certificate profile.** Certificate profiles → Create → *Public Trust*, using the validation from step 4. Note the profile name.
6. **Give GitHub Actions a login.** Microsoft Entra ID → App registrations → New registration (for example `ferry-release-signing`). Note the *Application (client) ID* and *Directory (tenant) ID*, then Certificates & secrets → New client secret, and copy its value. Back in the signing account: Access control (IAM) → Add role assignment → *Artifact Signing Certificate Profile Signer* → that app.
7. **Add the secrets** (in the repository folder, with the GitHub CLI; each command prompts for the value):

   ```sh
   gh secret set AZURE_TENANT_ID
   gh secret set AZURE_CLIENT_ID
   gh secret set AZURE_CLIENT_SECRET
   gh secret set AZURE_SIGNING_ENDPOINT      # e.g. https://eus.codesigning.azure.net
   gh secret set AZURE_SIGNING_ACCOUNT       # the account name from step 3
   gh secret set AZURE_CERTIFICATE_PROFILE   # the profile name from step 5
   ```

The workflow then signs `ferry.exe` and `ferry-cli.exe` before packaging and fails if Windows does not report the signature as `Valid`. A newly signed app can still see a SmartScreen prompt for a while, until it has built up download reputation; the publisher line then shows your name instead of "Unknown publisher".

### Free alternative for Windows: SignPath

[SignPath Foundation](https://signpath.org/) signs open-source projects for free. The certificate is issued to SignPath Foundation (which appears as the publisher), the project must have an OSI-approved license (Ferry's Apache 2.0 qualifies), and it needs an application, approval and a code-signing policy page crediting them. It uses SignPath's own GitHub integration, so choosing it means replacing the *Sign (Windows)* step.

## macOS: Developer ID and notarization

1. **Join the [Apple Developer Program](https://developer.apple.com/programs/)** as an individual (US$99 a year). Note your Team ID (Membership details).
2. **Create a Developer ID Application certificate.** In Xcode: Settings → Accounts → Manage Certificates → + → *Developer ID Application*. Or create a certificate signing request in Keychain Access and upload it at [developer.apple.com/account/resources/certificates](https://developer.apple.com/account/resources/certificates).
3. **Export it.** In Keychain Access, under My Certificates, right-click *Developer ID Application: Your Name (TEAMID)* → Export → `.p12`, with a strong password.
4. **Make an app-specific password** for notarization at [account.apple.com](https://account.apple.com) → Sign-In and Security → App-Specific Passwords.
5. **Add the secrets:**

   ```sh
   base64 -i DeveloperID.p12 | gh secret set MACOS_CERTIFICATE_P12
   gh secret set MACOS_CERTIFICATE_PASSWORD   # the .p12 password
   gh secret set MACOS_SIGNING_IDENTITY       # Developer ID Application: Your Name (TEAMID)
   gh secret set APPLE_ID                      # your Apple Account email
   gh secret set APPLE_TEAM_ID
   gh secret set APPLE_APP_PASSWORD            # the app-specific password
   ```

   Then delete the `.p12` file from your disk.

The workflow signs both binaries with the hardened runtime and a secure timestamp, submits them to Apple's notary service, and fails unless Apple answers `Accepted`. Ferry ships as bare executables, which cannot have the ticket stapled to them; macOS looks it up online the first time Ferry opens. Packaging Ferry as a `Ferry.app` in a `.dmg` would allow stapling and a double-clickable app; that is a separate change.

## Check a signed build before tagging

Run the release workflow by hand (Actions → release → Run workflow, or `gh workflow run release.yml`). It builds and signs without publishing, and the signing steps print what they did. Download the artifacts and check:

- **Windows:** right-click `ferry.exe` → Properties → Digital Signatures, or `Get-AuthenticodeSignature .\ferry.exe` in PowerShell (Status `Valid`).
- **macOS:** `codesign -dv --verbose=4 ferry` shows `Authority=Developer ID Application: …` and `flags=0x10000(runtime)`.

Once both are signed, drop the first-launch workarounds from the README's Download section.
