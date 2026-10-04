# Fleet Operations Runbook

Owner: Priya Raman, Head of Fleet Operations. This runbook explains how Halcyon Fleet Operations monitors robot fleets, who to call, and how to respond to common problems. It is reviewed after every SEV1 and SEV2 incident. Last updated January 2026.

## Severity levels

- SEV1: a customer fleet is stopped or unsafe. Page immediately and open an incident channel.
- SEV2: a fleet runs at reduced throughput or a single zone is down.
- SEV3: a single robot is faulty and the customer can work around it.

## Current on-call leads

Each engineering team provides one on-call lead per fortnightly rotation. Rotations change on Monday at 09:00 Central European Time. The current leads are:

| Team | On-call lead | Backup |
|---|---|---|
| Navigation | Imani Okoye | Tobias Lindqvist |
| Perception | Luca Bianchi | Kenji Watanabe |
| Platform | Ayesha Khan | Nadia Petrova |
| Security | Elena Sorokina | Dana Okafor |

The on-call lead is the first person the pager reaches for their team. If there is no acknowledgement within 10 minutes, the pager moves to the backup. Fleet Operations coordinators work with the on-call leads and run the incident channel.

## Paging

Use the pager only for SEV1 and SEV2. The alert policy pages when more than 10 robots in one zone report deadlock warnings, a change introduced after the March 2025 warehouse outage. For SEV3 open a ticket in the support queue and tag the owning team.

## Responding to a stopped fleet

1. Confirm the scope in the fleet console: which site, which zone, how many robots.
2. Declare the severity and open an incident channel named after the date and the site.
3. Check the last deployment. If a release reached the fleet in the last 24 hours, consider a rollback before debugging.
4. Page the on-call lead of the team that owns the affected component, found through the org chart.
5. Post an update to the customer through Customer Success every 30 minutes.

## Rollback procedure

Rollbacks are done through the fleet gateway by the Platform on-call lead. Select the previous firmware bundle, choose the affected zone first, and watch the stalled robot count for five minutes before widening the rollback. Rollbacks use emergency access to the production fleet console, which requires multi-factor authentication.

## Staged rollouts

Every release goes to 5 percent of a fleet for at least 24 hours before reaching the rest. Only a SEV1 fix can skip this stage, and it needs approval from the incident commander and from Dana Okafor.

## After the incident

Within five working days the incident commander writes a postmortem, names an owner for each action item and shares it with the whole company. The postmortem for the March 2025 outage is the reference example.
