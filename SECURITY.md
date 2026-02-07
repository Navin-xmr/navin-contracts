# Security Policy

## Reporting a Vulnerability

The Navin team takes security vulnerabilities seriously. We appreciate your efforts to responsibly disclose your findings.

### Please DO NOT:

- Open a public GitHub issue for security vulnerabilities
- Disclose the vulnerability publicly before it has been addressed

### Please DO:

*Report security vulnerabilities to: navinxmr@gmail.com*

OR

**Reach out to: dinahmaccodes** on Telegram

Telegram GC link: 

When reporting a vulnerability, please include:

1. **Description**: A clear description of the vulnerability
2. **Impact**: What could an attacker achieve by exploiting this?
3. **Reproduction**: Step-by-step instructions to reproduce the issue
4. **Proof of Concept**: Code or screenshots demonstrating the vulnerability
5. **Suggested Fix**: If you have ideas on how to fix it (optional)

### Response Timeline

- **Acknowledgment**: Within 72 hours of report
- **Initial Assessment**: Within 7 days
- **Fix Timeline**: Depends on severity
  - Critical: 24-48 hours
  - High Priority: 7 days
  - Medium Priority: 30 days
  - Low Priority: 90 days

## Supported Versions

We currently support security updates for:

| Version | Supported          |
| ------- | ------------------ |
| 0.x.x   | :white_check_mark: |

## Security Best Practices

### For Contributors

1. **Never commit secrets**: No private keys, API keys, or passwords
2. **Use `require_auth()`**: Always validate caller authorization
3. **Check arithmetic**: Use checked operations to prevent overflows
4. **Validate inputs**: Always validate external inputs
5. **Test edge cases**: Include security-focused tests
6. **Review dependencies**: Keep dependencies updated


## Security Audit Process

Before major releases:

1. Internal security review
2. Community review period
3. External security audit (for major versions)
4. Bug bounty program (planned)

## Acknowledgments

We thank the following security researchers for responsibly disclosing vulnerabilities:

<!-- Will be updated as reports are received and resolved -->

- None yet - be the first to help us out!

## Contact

For security concerns: **navinxmr@gmail.com*

For general questions: Open a GitHub Discussion

---

**Thank you for helping keep Navin secure!** 