# Security Policy

## Supported Versions

| Version | Supported          |
| ------- | ------------------ |
| 1.x     | :white_check_mark: |

## Reporting a Vulnerability

If you discover a security vulnerability in MagicX RAM Cleaner, please report it
responsibly.

**Do NOT open a public GitHub issue for security vulnerabilities.**

Instead, please email the maintainer directly:

- **Email:** [ehsan18t@gmail.com](mailto:ehsan18t@gmail.com)
- **Subject:** `[SECURITY] MagicX RAM Cleaner — <brief description>`

### What to include

1. Description of the vulnerability.
2. Steps to reproduce.
3. Potential impact (e.g., privilege escalation, memory corruption).
4. Suggested fix, if any.

### Response timeline

- **Acknowledgement:** Within 48 hours.
- **Assessment:** Within 7 days.
- **Fix release:** As soon as practical, depending on severity.

### Severity context

This tool runs with **Administrator privileges** and interacts directly with
Windows kernel memory management APIs. Security issues in this context could
have significant system-wide impact. We take all reports seriously.

## Security Design Principles

- The binary requires Administrator elevation via an embedded Windows manifest.
- All NT API calls check return values and handle errors gracefully.
- No network access — the tool is fully offline.
- No user input is passed to system APIs without validation.
- Unsafe code is strictly gated and documented with `// SAFETY:` comments.
