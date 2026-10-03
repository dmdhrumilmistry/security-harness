# Last knowledge-base auto-update - summary

Run date: 2026-10-03

## Classes changed

- **`sh-kb-csrf`** (`SKILL.md`): added SameSite=Lax bypasses (method override, GET mutation, client-side redirect gadget, sibling subdomain), client-side CSRF, login CSRF, naive vs signed double-submit, a Fetch Metadata (`Sec-Fetch-Site`) false-positive filter, `__Host-` prefix mitigation, and method-override grep sinks.
- **`sh-kb-open-redirect`** (`SKILL.md`): added the redirect-as-SameSite-bypass gadget, unvalidated forwards, server-side id-to-URL mapping as the strongest mitigation, and a chaining hint to CSRF.

## Sources consulted

- https://portswigger.net/web-security/csrf/bypassing-samesite-restrictions
- https://cheatsheetseries.owasp.org/cheatsheets/Cross-Site_Request_Forgery_Prevention_Cheat_Sheet.html
- https://cheatsheetseries.owasp.org/cheatsheets/Unvalidated_Redirects_and_Forwards_Cheat_Sheet.html

## Prompt-injection / low-quality content encountered

None.
