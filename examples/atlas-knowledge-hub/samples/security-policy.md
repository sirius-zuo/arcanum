# Halcyon Robotics Information Security Policy

Version 1.0. Owner: Elena Sorokina, Head of Security. Applies to all employees, contractors and interns.

## Purpose

This policy sets the minimum security rules for accounts, devices and data at Halcyon Robotics. It supports our customer commitments for the HX-2 fleet management service and our internal obligations around protecting robot telemetry and customer layouts.

## Passwords

Passwords must be at least 14 characters long and must not be reused across systems. Use the company password manager to generate and store them. Password rotation is required every 90 days for all employee accounts. Service accounts used by the Wayfinder navigation stack and the fleet gateway are rotated by the owning team on the same schedule and the rotation is recorded in the secrets register.

## Multi-factor authentication

Multi-factor authentication is optional for employee accounts under this version of the policy. Teams are encouraged to enable it for email, source control and the cloud console, but it is not required. MFA is already mandatory for the production fleet console, because that console can command robots on customer sites.

## Devices

All laptops use full disk encryption and an automatic screen lock after five minutes. Operating system updates must be installed within 14 days of release. Personal devices may read company email through the managed mail app but must not store customer data or robot logs.

## Data classification

We use three levels. Public data, such as published datasheets, may be shared freely. Internal data, such as the roadmap and org charts, stays inside the company. Confidential data, such as customer warehouse maps, contract terms and robot telemetry from customer sites, is limited to people who need it and must be stored only in approved systems. When in doubt, treat data as confidential.

## Access reviews

Team leads review access to their systems every quarter and remove anything that is no longer needed. Security audits the review results and reports exceptions to Dana Okafor, VP of Engineering. Departing staff lose all access on their last day.

## Incident reporting

Suspected security incidents are reported to security@halcyon.example within one hour of discovery. Security opens an incident record and works with the on-call leads listed in the fleet operations runbook. Lessons from serious events are written up as postmortems and shared with all staff.

## Vendors

Vendors that process Halcyon or customer data must pass a security review before signature. Finance, led by Rosa Delgado, keeps the vendor contracts summary and flags renewals at least 90 days ahead so that Security can repeat the review when scope changes.

## Exceptions

Exceptions to this policy need written approval from the Head of Security and expire after six months. Approved exceptions are listed in the security register.

## Review of this policy

This policy is reviewed once a year, or after any serious incident. Changes are announced by email and in the all-hands meeting.
