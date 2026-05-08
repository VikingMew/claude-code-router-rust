# Security

**状态：** 长期安全文档
**最后验证：** 2026-05-08

## Security Model

CCR is a local desktop router. Its default security boundary is the local machine.

The config API should be accessible only when one of these is true:

- the request is from loopback and no API key is configured;
- the request presents the configured bearer token.

When API key support is enabled, callers must use the expected Authorization header.

Admin API is not part of the default product surface. `/api/admin/*` must be
default-off. If an admin endpoint is enabled in the future, it must require an
explicit config flag, pass the same authorization boundary, and have disabled
and enabled behavior covered by tests.

## Secrets

Secrets include:

- provider API keys;
- CCR API key;
- user client auth files;
- upstream authorization headers.

Rules:

- Never log raw provider API keys.
- Redact `authorization`, `x-api-key`, `api-key` and equivalent headers.
- Redacted config APIs should not return provider API keys.
- Preserve user-owned config fields when editing client config.
- Backups containing secrets should stay local and should not be included in generated reports.

## Local Files

Important paths:

- `~/.claude-code-router/config.json`
- `~/.claude-code-router/*.log`
- Claude Code config path from app settings or default client location.
- Codex config/auth paths.
- OpenCode/OpenClaw config paths.

Config writes must avoid deleting unrelated user data.

## Network

Outbound requests go to configured provider endpoints. The server may use configured HTTP/HTTPS/SOCKS proxies.

Security-sensitive behavior:

- provider endpoint URL validation;
- proxy handling;
- upstream request header construction;
- response body logging and truncation;
- config API authorization.
- admin API default-off behavior.

## Security Debt

Current known follow-up areas:

- Add tests for config API authorization behavior.
- Disable `/api/admin/*` by default and require auth if explicitly enabled.
- Add documentation for backup retention and cleanup.
- Add structured redaction tests for future JSONL logs.
- Review packaging scripts for install/uninstall path safety.
