# Last knowledge-base auto-update - summary

Run date: 2026-09-26

## Classes changed

- **`sh-kb-auth`** (`SKILL.md`): added the JWT header-parameter injection family - `kid` path
  traversal/SQLi into the key lookup, `jwk` header letting the server trust an attacker-embedded public key,
  and `jku`/`x5u` pointing to an attacker-hosted key set/certificate - with matching sinks, PoC payloads, and
  false-positive filters (pinned key / fixed `kid` map / host allowlist). Also added lockout-hygiene guidance:
  the counter must be keyed by account (not source IP) with bounded exponential backoff, and a lockout must
  not be usable as a denial-of-service against a victim (self-recovery must stay reachable). Updated the
  mitigation line accordingly.
- **`sh-kb-file-upload`** (`SKILL.md`): added extension-blacklist bypasses that are easy to miss (alternative
  interpreter extensions `.phtml`/`.php5`/`.pht`/`.shtml`/`.asp;.aspx`, URL-encoded extension, NTFS Alternate
  Data Streams, non-recursive-strip bypass `shell.p.phphp`, ExifTool-built polyglots), plus a race-condition
  class (upload-to-webroot-then-delete-on-fail creating a request window before removal) and two
  platform-hardening false-positive filters/mitigations: disabling per-directory config overrides
  (`AllowOverride None` / locked IIS handler mappings) so a smuggled `.htaccess`/`web.config` can't grant
  execution, and disabling Windows 8.3 short-filename generation so a blocked long name can't be reached via
  its short alias.
- **`sh-kb-access-control`** (`SKILL.md`): added a URL-matching/verb-bypass sub-class (HTTP method mismatch
  between the authz gate and the router; case/trailing-slash/suffix routing discrepancies, e.g. Spring's
  pre-5.3 default `useSuffixPatternMatch`; `X-Original-URL`/`X-Rewrite-URL` header overrides) and a
  multi-step-process-flaw sub-class (authz checked on an early step, skippable by submitting the final step
  directly), with matching PoC templates. Added a false-positive-filter correction: a `Referer`-only check is
  not a real control (attacker fully controls that header) - flag it, don't treat it as mitigating. Updated
  the detection recipe and mitigation to require the authz gate's path/method matching to be at least as
  strict as the router's.

No other `sh-kb-*` files were modified this run. These three were chosen because they hadn't been touched
since the initial KB was authored (2026-08-22) - the automated-refresh commits so far had only covered
`sh-kb-race-conditions`, `sh-kb-ssrf`, and `sh-kb-xxe`.

## Sources consulted

- https://portswigger.net/web-security/jwt (JWT attack techniques: alg confusion, alg:none, kid/jwk/jku/x5u header injection, weak-secret brute force)
- https://cheatsheetseries.owasp.org/cheatsheets/Authentication_Cheat_Sheet.html (password length/policy, account-lockout design, reauthentication, MFA guidance)
- https://portswigger.net/web-security/file-upload (extension blacklist/whitelist bypasses, obfuscation techniques, content validation, upload race conditions)
- https://cheatsheetseries.owasp.org/cheatsheets/File_Upload_Cheat_Sheet.html (extension allowlist specifics, filename handling, storage priority, per-directory config overrides, 8.3 short names)
- https://portswigger.net/web-security/access-control (verb-based bypass, header URL override, URL-matching discrepancies, multi-step process flaws, Referer-based control weakness)
- https://cheatsheetseries.owasp.org/cheatsheets/Authorization_Cheat_Sheet.html (deny-by-default, global enforcement, object-identifier exposure, RBAC/ABAC tradeoffs - consulted for context; no direct additions beyond what's reflected above, existing KB already covers deny-by-default and object-level checks)

## Prompt-injection / low-quality content encountered

None. All six fetched pages (PortSwigger Web Security Academy topic pages, OWASP Cheat Sheet Series) were
on-topic, reputable technical documentation with no embedded instructions directed at the model. No Tier 3/4
(GitHub, HackerOne) sources were used this run - Tier 1/2 material was sufficient for the chosen classes.
