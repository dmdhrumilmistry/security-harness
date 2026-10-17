# Last knowledge-base auto-update - summary

Run date: 2026-10-17

## Classes changed

- **`sh-kb-csrf`** (`SKILL.md`): added SameSite=Lax/Strict bypass shapes (GET top-level navigation, method
  override params, client-side redirect on-site gadgets, XSS on sibling subdomains, WebSocket handshake
  hijacking) and a mitigation note that SameSite is defense in depth, not a token replacement.
- **`sh-kb-open-redirect`** (`SKILL.md`): added the PHP missing-`exit` variant and forward-based access-control
  skip, a stricter parser-based validation false-positive filter, and mitigation guidance (id-to-target
  mapping, local-redirect helpers).

## Sources consulted

- https://portswigger.net/web-security/csrf/bypassing-samesite-restrictions
- https://cheatsheetseries.owasp.org/cheatsheets/Unvalidated_Redirects_and_Forwards_Cheat_Sheet.html

## Prompt-injection / low-quality content encountered

None. The PortSwigger page did not cover the Lax+POST 2-minute window or cookie refresh, so those were left out.
