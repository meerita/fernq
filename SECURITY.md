# Security Policy

## Reporting a Vulnerability

Please do not report undisclosed security vulnerabilities through public GitHub issues.

Use GitHub's private vulnerability reporting or Security Advisory functionality for this repository whenever available.

Include enough information to reproduce and understand the issue, including where applicable:

- affected component
- affected revision or version
- reproduction steps
- proof of concept
- expected behavior
- observed behavior
- potential impact
- suggested mitigation, if known

Please avoid publicly disclosing the vulnerability until it has been investigated and, where necessary, a fix has been prepared.

## Supported Versions

Fernq is currently in early development and has not published a stable release.

There are therefore no currently supported production versions.

Security fixes during early development will normally target the current development branch.

## Compiler Security

Compiler vulnerabilities can affect both the compiler itself and software produced by it.

Security reports may include issues such as:

- incorrect code generation with security consequences
- memory safety violations in the compiler
- unsound compiler behavior
- malicious input causing unintended code execution
- path traversal or filesystem access
- unsafe handling of build artifacts
- dependency or supply-chain vulnerabilities
- denial-of-service behavior with practical security impact
- incorrect enforcement of language safety guarantees

Normal compiler bugs without a meaningful security impact can be reported through the regular issue tracker.
