# Security policy

This repository is a starting point for Tauri and Nuxt apps. A security issue in the starter can affect projects created from it, so please report vulnerabilities privately.

## Supported versions

Security fixes are made against the latest commit on `main`. There is no long-term support branch or backport policy. If you use an older commit or a modified copy, compare it with the current `main` branch before applying an advisory.

## Reporting a vulnerability

Do not open a public issue. Use [GitHub Security Advisories](https://github.com/xcvzmoon/bamboo/security/advisories/new) to send a private report to the maintainers.

Include the affected component (`app`, `src-tauri`, or build tooling), steps to reproduce, the impact, and any conditions needed to trigger the issue. A proof of concept is helpful when available.

We aim to acknowledge reports within 3 business days and provide an initial assessment within 5 business days. After triage, we will share a resolution timeline and updates through the advisory. With your permission, we will credit you when the advisory is published.

Please allow time to investigate and fix the issue before public disclosure. Do not access, change, or copy data beyond what is needed to demonstrate the problem.

## Scope

Issues involving Tauri commands or permissions, local file access, exposed secrets, Nuxt frontend behavior, or dependencies with a realistic exploitation path in this repository are in scope.

Report vulnerabilities in an upstream dependency to its maintainers when this project has no exploitable use of it. Issues introduced only by a downstream project's changes are outside this repository's scope.

## Disclosure

Once a fix is available, we will publish a GitHub Security Advisory with the impact and affected versions or commits. This project does not run a paid bug bounty program.
