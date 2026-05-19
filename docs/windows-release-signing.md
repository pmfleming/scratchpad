# Windows release signing

The release workflow signs Windows release artifacts before publishing them.
If signing is not configured, tag releases fail instead of publishing another unsigned MSI.
Set `WINDOWS_SIGNING_MODE=none` only when an unsigned release is intentional.

The workflow signs `target\release\scratchpad.exe` before WiX packages it, then signs the final
MSI in `dist\`, then regenerates the `.sha256` file so the checksum matches the signed installer.

## Option 1: PFX certificate stored in GitHub secrets

Set this repository variable, or leave it empty when only the PFX secrets below are configured:

```text
WINDOWS_SIGNING_MODE=pfx
```

Set these repository secrets:

```text
WINDOWS_CODESIGN_PFX_BASE64
WINDOWS_CODESIGN_PFX_PASSWORD
```

Create the Base64 secret value from a local PFX file with PowerShell:

```powershell
[Convert]::ToBase64String([IO.File]::ReadAllBytes("codesign.pfx")) | Set-Clipboard
```

The workflow uses `signtool.exe` from the Windows SDK, signs with SHA-256, and timestamps with
`http://timestamp.digicert.com`.

## Option 2: Azure Artifact Signing / Trusted Signing

Set this repository variable, or leave it empty when only the Azure settings below are configured:

```text
WINDOWS_SIGNING_MODE=trusted-signing
```

Set these repository variables:

```text
AZURE_TRUSTED_SIGNING_ENDPOINT
AZURE_TRUSTED_SIGNING_ACCOUNT
AZURE_TRUSTED_SIGNING_CERTIFICATE_PROFILE
```

Set these repository secrets for GitHub Actions OIDC login:

```text
AZURE_CLIENT_ID
AZURE_TENANT_ID
AZURE_SUBSCRIPTION_ID
```

The Azure identity must have the Artifact Signing Certificate Profile Signer role for the certificate
profile. The endpoint must match the Azure region for the signing account, for example
`https://eus.codesigning.azure.net/`.

Useful references:

- https://github.com/Azure/artifact-signing-action
- https://learn.microsoft.com/windows/apps/package-and-deploy/smartscreen-reputation
- https://learn.microsoft.com/windows/apps/package-and-deploy/code-signing-options
