# Postmortem: Warehouse Outage of 12 March 2025

Status: final. Author: Priya Raman, Head of Fleet Operations. Reviewers: Dana Okafor, Tobias Lindqvist, Nadia Petrova, Elena Sorokina. Severity: SEV1.

## Summary

On 12 March 2025 a software release to the traffic manager inside the Wayfinder navigation stack caused 212 HX-1 and HX-2 robots at the Northgate distribution centre in Utrecht to stop moving for 3 hours and 40 minutes. Customer order picking fell to about 15 percent of normal throughput. No person was harmed and no robot was damaged.

## Timeline

All times are local to Utrecht.

- 06:05: Platform rolls out Wayfinder 4.2 to the Northgate fleet through the fleet gateway.
- 06:20: Robots begin reporting deadlock warnings in narrow aisles. The warnings are rated low severity and do not page anyone.
- 06:48: The customer's shift lead calls the support line. Samir Haddad's Customer Success team opens a ticket.
- 07:02: The pager fires after the stalled robot count passes 100. Imani Okoye, the Navigation on-call lead, acknowledges within three minutes.
- 07:25: Priya Raman declares a SEV1 and opens the incident channel. Nadia Petrova joins for Platform.
- 08:10: Imani Okoye and Tobias Lindqvist identify a new lock ordering rule in the traffic manager that deadlocks when more than 40 robots queue at one intersection.
- 08:55: The decision is taken to roll back instead of patching forward.
- 09:45: Platform completes the rollback to Wayfinder 4.1 and robots resume work. Throughput is back to normal by 10:30.

## Root cause

Wayfinder 4.2 changed how robots reserve intersection cells. Under heavy queueing, two groups of robots could each hold a reservation the other group needed. The test suite covered up to 25 robots per intersection, while Northgate regularly reaches 60 during the morning peak. The release also went to the whole fleet at once, so there was no chance to see the problem on a small group first.

## What went well

The rollback procedure in the fleet operations runbook worked as written, and the on-call handover between Imani Okoye and the next shift was clean. Customer Success kept the customer informed every 30 minutes.

## What went badly

The deadlock warning was classed as low severity, so the first page arrived 42 minutes after the first symptom. The rollout skipped the canary stage because the release was marked as a minor change.

## Action items

1. Navigation, owner Tobias Lindqvist: add load tests with 80 robots per intersection before any release.
2. Platform, owner Nadia Petrova: make staged rollout mandatory, starting with 5 percent of the fleet for 24 hours.
3. Fleet Operations, owner Priya Raman: raise the deadlock warning to page when more than 10 robots are affected, and update the runbook.
4. Security, owner Elena Sorokina: review emergency access used during the rollback, which led to the revision of the password and multi-factor rules in the security policy.
5. Customer Success, owner Samir Haddad: send a written summary and a service credit to Northgate.

## Impact on customers

Northgate received a service credit under its contract and a visit from Samir Haddad. The incident is mentioned in the customer FAQ to explain why staged rollouts now take longer.
