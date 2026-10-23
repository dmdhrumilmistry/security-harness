# KB update run summary

## Changes
- `sh-kb-csrf/SKILL.md`: added refinements to the false-positive filters (weak double-submit cookies, SameSite limits with sibling subdomains and Lax GET, weak Origin/Referer matching, login CSRF, client-side CSRF) and extended Mitigation with Fetch Metadata (`Sec-Fetch-Site`) guidance and signed session-bound tokens.

## Sources consulted
- https://cheatsheetseries.owasp.org/cheatsheets/Cross-Site_Request_Forgery_Prevention_Cheat_Sheet.html

## Skipped content
- No prompt-injection or low-quality content encountered.

## Notes
- Only one class was updated this run; other classes were not reviewed against fresh sources, to keep the diff small.
