---
name: sh-kb-race-conditions
description: "Knowledge base for finding race conditions and TOCTOU flaws — concurrent requests exploiting non-atomic check-then-act logic (double-spend, limit bypass, balance manipulation, file TOCTOU). Use when hunting concurrency/business-logic race bugs. CWE-362/367/366, OWASP A04:2021-Insecure Design."
---

# Race Conditions & TOCTOU — Hunter Knowledge Base

A gap between checking a condition and acting on it lets concurrent requests slip through: spend a coupon
twice, withdraw more than the balance, bypass a rate/quota limit, or swap a file between check and use.
These are logic bugs — reason about interleavings, not just about a dangerous sink.

## Classic patterns to hunt
- **Check-then-act on shared state** without a lock/transaction/atomic op: read balance -> if enough ->
  subtract; check "coupon unused" -> mark used; check "invite not redeemed" -> redeem.
- **Limit/quota bypass**: "only once" / "max N" enforced by a non-atomic read+increment; parallel requests
  all pass the check before any writes.
- **Financial double-spend**: withdraw/transfer/refund without row locking or a DB constraint.
- **TOCTOU on filesystem** (CWE-367): `os.path.exists`/`access()` then `open()`; symlink swapped in between.
- **Idempotency gaps**: ret/replay of a request causes duplicate side effects (no idempotency key).
- **Auth/session races**: token validated then state changes; concurrent login/session mutation.

## Sinks / patterns (grep targets)
Non-atomic sequences: a SELECT/read followed by an UPDATE/write of the same field without a transaction,
`SELECT ... FOR UPDATE`, optimistic-lock version, or atomic DB op. Grep near money/credits/quota/inventory:
`balance|credit|quota|limit|coupon|voucher|stock|inventory|redeem|withdraw|transfer`. Filesystem:
`os.path.exists|access\(|stat\(` shortly before `open(`. Look for **absence** of `with transaction`,
`SELECT FOR UPDATE`, `lock`, `Mutex`, `atomic`, unique constraints.

## Detection recipe
1. Identify state-changing operations on shared/limited resources from `codebase-map.json`.
2. For each, read the handler: is the check-and-act **atomic**? (single UPDATE with a WHERE guard, DB
   constraint, row lock, or app-level lock?) If it's read-then-write across statements with no lock, flag.
3. Note whether the endpoint is reachable concurrently (most HTTP handlers are).

## PoC / evidence
- Describe the interleaving: "N parallel `POST /redeem` requests, all read `used=false` before any writes
  `used=true`, so all N succeed." Show the non-atomic code path.
- Practical PoC: fire many concurrent identical requests (e.g. 20-50 in parallel) and observe the invariant
  broken (balance negative, coupon used twice). Often `needs-runtime` to fully confirm.

## False-positive filters
- The critical section is atomic: single `UPDATE ... SET x=x-1 WHERE x>0` (check in the WHERE), DB unique
  constraint / `SELECT ... FOR UPDATE`, optimistic locking (version column), `INSERT ... ON CONFLICT`, or an
  explicit lock/mutex/serializable transaction covering check+act.
- The resource isn't shared across requests, or the operation is naturally idempotent.
- A single-writer queue / actor serializes the operation.

## CWE / OWASP / severity
CWE-362 (race), CWE-367 (TOCTOU), CWE-366. OWASP A04:2021 (Insecure Design). Financial/limit bypass ->
**high/critical** by value; filesystem TOCTOU -> high if it leads to privilege gain.

## Chaining hints
Race to bypass a spend/limit -> monetary loss; combine with `access-control` (race a permission change);
TOCTOU file swap -> `path-traversal`/privilege escalation. Often amplifies an otherwise-bounded feature.

## Mitigation
Make check-and-act atomic: guard in the UPDATE `WHERE`, use DB constraints/unique indexes, row locking
(`SELECT ... FOR UPDATE`) or optimistic locking, serializable transactions, idempotency keys for retried
mutations, and atomic counters for quotas. Avoid `exists`-then-`open`; open directly and handle errors.
