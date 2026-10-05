# Decisions

## D0-1 OpenAPI is authoritative (2026-10-06)
Fetched `openapi/awsevents.v1.json` (OpenAPI 3.1). Differences from the brief:
- Session short code is `abbreviation` (not `code`). There are no `start`/`end`/`allDay`/`code` fields: time is `sessionTime{date,time,length,timezone}` (local date, "HH:MM", minutes as string). `isAllDaySession`, `isReservable`, `seatAvailability` (`available|limited|veryLimited|unavailable|walkUp`; no "full" literal — `unavailable` is treated as full), `venue`, `room`.
- Session list fields: tracks, topics, industries, areasOfInterest, roles, services, segments, features, customerPersonas, experiences, additionalActivities, focusAreas. Speakers carry only `name`.
- ListSessions max page 250; `totalCount` is a number.
- Bulk failure codes: sessionNotReservable, scheduleConflict (with `conflictsWith`), alreadyScheduled, sessionFull, insufficientAccess, timePassed, alreadyFavorited, notFavorited, other. Unknown → generic refusal.
- CreatePersonalTime/UpdatePersonalTime/DeletePersonalTime/CancelReservation/DisassociateFavorite all return 204.
- Schedule: `reserved[]`, `favorites[]` (ids), `personalTime[]` with `personalTimeId`.
- Event has `authenticationRequired`, `timezone`, etc. Event `timezone` is the source for local time conversion.
- GetSchedule can return 409 (closed) in the spec; handled like other 409s.
Brief names (`code`, `walk-up`, `full`) are mapped in `src/api/types.rs`.

## D0-2 Toolchain
Rust installed with rustup (minimal profile) into `~/.cargo`. reqwest 0.13 (`rustls` feature, formerly `rustls-tls`). YAML: `serde_yaml_ng` (maintained fork of deprecated `serde_yaml`). Time: `chrono` + `chrono-tz` unified.
