# Security policy

Do not disclose vulnerabilities in public issues. Send a minimal reproduction,
affected revision, platform, and impact privately to the project maintainer.
Do not attach private media, credentials, or proprietary project files.

The project will acknowledge reports, triage source/media-parser/plugin-host
boundaries first, and publish a remediation note after users have a safe
upgrade path. Security fixes must include a regression test or an explicit
reason a test cannot safely be published.

Untrusted import/container parsing and third-party plugins are high-risk
boundaries. Do not weaken helper-process isolation, dependency policy, or the
no-bundled-codec rule to reproduce an issue.
