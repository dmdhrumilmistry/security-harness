---
name: sh-kb-access-control
description: "Knowledge base for finding broken access control — IDOR/BOLA, missing function-level authorization, privilege escalation, and multi-tenant isolation failures. Use when hunting authorization issues or reviewing whether users can access resources/actions they shouldn't. CWE-284/285/639/862/863, OWASP A01:2021-Broken Access Control."
---

# Broken Access Control — Hunter Knowledge Base

The #1 OWASP risk. The app authenticates the user but fails to check whether *this* user may access
*this* resource or perform *this* action. Unlike injection, the bug is usually a **missing or wrong
check**, not a dangerous sink — so hunt by comparing "who can reach this" against "who should".

## Sub-classes
- **IDOR / BOLA** (object-level): `/api/orders/{id}` returns any order because the handler looks up by id
  without checking ownership.
- **Missing function-level authz** (BFLA): an admin-only endpoint has no role check; forced browsing to
  `/admin/*`; a mutating action reachable by a read-only user.
- **Privilege escalation**: user can set their own role/tenant/flags (mass-assignment of `is_admin`),
  or a lower role reaches a higher-role action.
- **Multi-tenant isolation**: queries not scoped by `tenant_id`/`org_id`; one tenant reads another's data.

## Sources & the key question
Every request carrying a **resource identifier** the client controls (path/query/body id, filename, key)
or a **capability** (action, role, flag). For each entry point in `codebase-map.json`, ask:
1. Is authentication required? (recon flags `auth_required`.)
2. After auth, is there an **ownership/tenant check** tying the resource to the caller?
3. Is there a **role/permission check** for the action?

## Sinks / patterns (grep targets)
- Handlers that fetch by client id without a scope: `findById(req.params.id)`, `Model.objects.get(pk=id)`,
  `repository.findOne(id)`, `SELECT ... WHERE id = <id>` with no `AND owner_id = <current_user>`.
- Missing decorators/middleware: routes lacking `@login_required`/`@requires_role`/`authorize()`/
  `[Authorize]`/`before_action`; Express routes without an auth middleware in the chain.
- Client-trusted authz: role/tenant taken from request body/JWT claim that the client can set, or from a
  hidden form field; `if (req.body.role === 'admin')`.
- Mass assignment: `User(**req.body)`, `user.update(req.body)`, `Object.assign(user, req.body)`,
  `Model.objects.update(**data)` letting `role`/`is_admin`/`tenant_id` through.
- Client-side-only gating: an action guarded only by hiding a UI button.

## Detection recipe
1. From `codebase-map.json`, list every entry point and its `auth_required`.
2. For each resource-fetching handler, `graft ask "does this handler check that the resource belongs to
   the current user"` (or read it) — look for a comparison against the authenticated principal.
3. Diff sibling endpoints: if `GET /doc/{id}` checks ownership but `DELETE /doc/{id}` doesn't, flag it.
4. Grep role checks and find endpoints that mutate/admin without one:
   `graft grep "is_admin|role\s*==|hasRole|requires_role|@Authorize|before_action|current_user" --json`.
5. For mass assignment, grep object-hydration-from-request patterns above and check for allowlists.

## PoC templates
- IDOR: authenticate as user A, capture `GET /api/orders/1001`; replay as user B (or unauth) -> if B sees
  A's order, confirmed. Enumerate/increment/UUID-swap the id.
- BFLA: as a normal user, call the admin/mutating endpoint directly (`curl -X POST /admin/users -H
  'Authorization: <user-token>'`) -> success = missing function-level check.
- Priv-esc via mass assignment: `PATCH /api/me {"is_admin":true}` or `{"role":"admin"}` -> re-fetch profile.
- Tenant bypass: as tenant T1, request a resource id owned by T2.

## False-positive filters
- Ownership/tenant enforced in a **query scope** (`WHERE owner_id = :me`), a base queryset
  (`get_queryset` filtered by user), row-level security, or shared middleware you must read to see.
- Central authorization layer (policy objects, Pundit/CanCan, Spring Security matchers, a gateway) applied
  before the handler — confirm the resource is actually covered by it.
- The identifier is not attacker-controlled (server-derived from session).
- Resource is intentionally public (read-only, non-sensitive) — confirm via intent, don't assume.

## CWE / OWASP / severity
- CWE-639 (IDOR), CWE-862 (missing authz), CWE-863 (incorrect authz), CWE-284/285, CWE-566 (mass-assign).
  OWASP **A01:2021-Broken Access Control**.
- Severity: **high**->**critical** by data sensitivity and whether it's cross-tenant / privilege-gaining.

## Chaining hints
- IDOR read -> harvest other users' data/tokens -> `auth` account takeover.
- Mass-assignment priv-esc -> reach admin-only features -> which may enable RCE / config change.
- Combine with `secrets`/`sqli` reads for a full data-exfil chain. Often the escalation *step* in chains.

## Mitigation
Enforce authorization server-side at the object level on every access: scope queries by the authenticated
principal/tenant, use a central policy layer (deny by default), never trust client-supplied role/tenant,
and allowlist mass-assignable fields.
