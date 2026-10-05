# Rules compliance

> Status: written from the project brief and the AWS Events API documentation (quotas, errors, token handling). **TODO(human):** re-read the official hackathon rules and tick each line below before submitting.

## Forbidden by the rules / API terms (riv complies)

| Rule | How riv complies |
|---|---|
| No harm to other participants or AWS staff | Only your own schedule is read and written; no scraping of other people's data; no bulk catalog redistribution. |
| No IP infringement | All sample/mock data is synthetic and generated in code (`src/mock/catalog.rs`). The real catalog is never committed or shipped; it exists only in the user's local database. |
| Original work | The code is written for this project; prior projects (reinvent26-planner, re:Plan 2026, reinvent-scout, reinvent2026-mcp) are credited in the article, not copied. |
| API quotas | Per-operation per-minute quotas are tracked client-side (`src/api/quota.rs`); batches shrink to the remaining quota; `429` waits for `Retry-After`; no fixed-sleep hammering. Real-API tests are read-only and opt-in (`RIV_REAL_API=1`). |
| Token handling | The access token is sent only to the API base (`api.awsevents.com`; overridable only for the mock). Tokens are never logged, never put in URLs, errors, commits or structured logs (`Credentials` has a redacting `Debug`). Stored in a 0600 file; refresh token replaced when rotated; `riv logout` revokes then deletes. |
| 401 handling | Refresh once, retry once, then stop (no loops). |
| Registration required | The tool signs in as the attendee and relies on their registration; it does not try to bypass authentication. |

## Things riv chooses not to do (self-imposed)

- **No polling auto-reservation.** There is no watch/retry loop for open seats. Writes happen only for an approved plan, once.
- **No apply without an approved plan.** `riv apply` requires `--plan` (or `--resume` of a run made from one); plans expire after 30 minutes and are bound to the spec, the observed schedule and the account.
- **No interference with reservations riv did not create.** They are listed as `unmanaged (untouched)`; removing a row from the spec never cancels; `want: none` on an unmanaged reservation only warns.
- **No blind resend after an unknown outcome**; reconcile with `GetSchedule` first.
- **No repeated real-API write benchmarks.** The bench commands read only, or run against the mock.
- **No video transcripts** are fetched for prep notes; quotes stay short with source links.
- **No hosted component.** Everything runs on the attendee's machine.
