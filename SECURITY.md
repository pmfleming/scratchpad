# Security Policy

## Supported version

Security fixes are made on the default branch and included in the next release. Users should run the latest available release.

## Reporting a vulnerability

Report suspected vulnerabilities privately through GitHub's security-advisory feature for this repository. Do not open a public issue before coordinated disclosure.

Include reproduction steps, affected platforms, impact, and any suggested remediation. Maintainers should acknowledge reports within seven days.

Dependency scanning intentionally ignores `RUSTSEC-2021-0030` and `RUSTSEC-2025-0049`: both target an unrelated crates.io package that happens to be named `scratchpad`. The reasons are recorded in `.cargo/audit.toml`.
