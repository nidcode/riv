# Demo: a replacement that may lose a seat

A real run of `riv` against the bundled mock server (synthetic data, no sign-in). A seat held by hand is left alone, a plan is only a diff, and a replacement cannot be applied without an explicit flag. Reproduce it with the quick start in the README.

```
$ riv sync
Synced 120 sessions (120 changed, 0 removed). Catalog version 2026-10-08T06:12:16Z.

$ riv search "agentic" --level 200 --fields id,code,title,start,seat --limit 4
mock-0094  DOP200  Operating agentic workflows with Amazon Bedrock  Fri 12/04 10:00  available
mock-0070  ANT200  Securing agentic workflows with Amazon Bedrock  Thu 12/03 11:30  veryLimited
mock-0001  AIM301-R1  Deep dive: Building for agentic workflows  Mon 11/30 08:30  available
mock-0047  DOP200  Optimizing agentic workflows with AWS Step Functions  Mon 11/30 11:30  unavailable

$ riv schedule
Reserved (1)
  DOP200  Operating agentic workflows with Amazon Bedrock  Fri 12/04 10:00-12:00  MGM Grand
Favorites (0)
Personal time (0)

$ riv plan   # mock-0099 as favorite; the seat held by hand is left alone
Plan 01M4D286TDBNFXSZZWKMJSNW6N  (event demo-reinvent, expires 2026-10-08T06:42:16Z)
~ favorite  DOP400  Operating data pipelines with Amazon Bedrock  (Wed 12/02 08:30 MGM Grand)
# unmanaged (untouched)  DOP200  Operating agentic workflows with Amazon Bedrock  (Fri 12/04 10:00 MGM Grand)
Plan: 0 to reserve, 0 to replace, 0 to cancel, 1 to favorite, 0 to unfavorite, 0 block changes.
To apply: riv apply --plan 01M4D286TDBNFXSZZWKMJSNW6N

$ riv apply --plan 01M4D286TDBNFXSZZWKMJSNW6N --yes
Run 01M4D286TWFKR04AWN15Q2STHD  (plan 01M4D286TDBNFXSZZWKMJSNW6N)
DONE     favorite     DOP400  Operating data pipelines with Amazon Bedrock
Verify: your schedule matches the spec.

$ riv plan   # replace the hand-made reservation
Plan 01M4D286ZFD70QEFD8QRBVJXFP  (event demo-reinvent, expires 2026-10-08T06:42:16Z)
-/+ replace (seat may be lost)  DOP200  Operating agentic workflows with Amazon Bedrock  (Fri 12/04 10:00 MGM Grand)  ->  DAT100  Optimizing cost-aware architectures with Amazon EKS  (Fri 12/04 10:00 Mandalay Bay)
Plan: 0 to reserve, 1 to replace, 0 to cancel, 0 to favorite, 0 to unfavorite, 0 block changes.
A replace cancels first; if the new session is full you may lose the seat. Apply requires --accept-seat-loss.
To apply: riv apply --plan 01M4D286ZFD70QEFD8QRBVJXFP --accept-seat-loss

$ riv apply --plan 01M4D286ZFD70QEFD8QRBVJXFP --yes   # without the flag
error: this plan replaces reservations and may lose a seat; review the diff and pass --accept-seat-loss to apply
(exit code 4)

$ riv apply --plan 01M4D286ZFD70QEFD8QRBVJXFP --accept-seat-loss --yes
Run 01M4D287087CJ2AZG3AW2WNS7W  (plan 01M4D286ZFD70QEFD8QRBVJXFP)
DONE     cancel       DOP200  Operating agentic workflows with Amazon Bedrock
DONE     reserve      DAT100  Optimizing cost-aware architectures with Amazon EKS
Verify: your schedule matches the spec.

$ riv verify
Verify: your schedule matches the spec.
```

What to notice:

- `# unmanaged (untouched)` — the reservation made outside riv is listed but never changed.
- `-/+ replace (seat may be lost)` — the diff says out loud that the old seat is cancelled first.
- Applying without `--accept-seat-loss` is refused (exit code 4) and nothing is written.
- After the apply, `riv verify` reads the real schedule back and compares it with the spec.
