# Last knowledge-base auto-update — summary

Run date: 2026-09-26

## Classes changed

- **`sh-kb-race-conditions`** (`SKILL.md`): added the HTTP/2 "single-packet attack" and HTTP/1.1
  "last-byte synchronization" techniques for tightening the race window (removes network-jitter false
  negatives), tool notes (Burp Repeater parallel group send, Turbo Intruder `engine=Engine.BURP2`/
  `concurrentConnections=1`), and a note on connection pre-warming when racing across multiple endpoints
  with different backend processing times.
- **`sh-kb-xxe`** (`SKILL.md`): added XInclude (`xi:include`) as an entity-free attack vector that bypasses
  DOCTYPE-based filters — so "no `<!DOCTYPE>` present" is not on its own a false-positive signal. Added a
  Java-specific false-positive filter clarifying that `disallow-doctype-decl` alone does not disable
  XInclude or external-schema resolution (`setXIncludeAware(false)`, `ACCESS_EXTERNAL_DTD`/
  `ACCESS_EXTERNAL_SCHEMA` also needed), and clarified .NET 4.5.2+ / PHP 8.0+ safe-by-default versions to
  reduce over-flagging on modern runtimes. Updated the mitigation line accordingly.
- **`sh-kb-ssrf`** (`SKILL.md`): added a "URL-parser confusion" bypass class (differential parsing between
  the allowlist-validation code and the actual HTTP client, e.g. backslash-before-`@` host confusion
  between WHATWG-URL and RFC-3986 parsers) with example payloads, and a matching false-positive filter:
  an allowlist guard is insufficient if the validator and the fetch call parse the URL with different
  libraries — that gap should still be flagged even with an allowlist present.

No other `sh-kb-*` files were modified this run.

## Sources consulted

- https://portswigger.net/web-security/race-conditions (single-packet attack / last-byte sync / multi-endpoint timing)
- https://cheatsheetseries.owasp.org/cheatsheets/XML_External_Entity_Prevention_Cheat_Sheet.html (per-language XXE hardening, XInclude, JAXP properties, .NET/PHP safe-by-default versions)
- https://cheatsheetseries.owasp.org/cheatsheets/Server_Side_Request_Forgery_Prevention_Cheat_Sheet.html (URL parser confusion, DNS rebinding, redirect handling, defense-in-depth)
- https://cheatsheetseries.owasp.org/cheatsheets/Deserialization_Cheat_Sheet.html (consulted for the deserialization class; see below — no changes made)

## Considered but not changed

- **`sh-kb-deserialization`**: fetched the OWASP Deserialization Cheat Sheet looking for under-covered
  vectors (PHP `phar://` stream-wrapper deserialization, Java non-standard formats). The cheat sheet itself
  didn't contain enough new, directly attributable technical detail beyond what the KB already has, and I
  don't have an allowlisted source confirming the `phar://` specifics well enough to add it responsibly.
  Left unchanged this run rather than adding weakly-sourced content.

## Prompt-injection / low-quality content encountered

None. All fetched pages (PortSwigger, OWASP Cheat Sheet Series) were on-topic, reputable technical
documentation with no embedded instructions directed at the model. No Tier 3/4 (GitHub, HackerOne) sources
were used this run.
