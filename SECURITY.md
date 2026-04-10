# Security Policy

## Reporting a Vulnerability

Please do not report security vulnerabilities through public GitHub issues.

Use one of the following private channels:

1. GitHub private vulnerability reporting (preferred), if enabled for this repository.
2. Direct private contact with the repository owner.

Include:

- A clear description of the issue.
- Reproduction steps or proof of concept.
- Affected versions/commits.
- Potential impact.

You can expect an initial response as soon as possible after the report is received.

## Scope

Security reports are especially relevant for:

- FFI boundaries (`flow-gate-ffi`)
- Unsafe code and pointer/lifetime handling
- XML parsing of untrusted inputs
- Denial-of-service vectors in parsing/evaluation paths

## Disclosure

Please allow time for investigation and remediation before public disclosure.
