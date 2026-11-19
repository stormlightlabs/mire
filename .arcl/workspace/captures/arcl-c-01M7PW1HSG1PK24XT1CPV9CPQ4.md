+++
id = "arcl-c-01M7PW1HSG1PK24XT1CPV9CPQ4"
title = "Keep the App secrets in a main-only environment"
status = "captured"
created-at = "2026-11-18T08:23:16.528Z"
+++

The `THUNDERBOT_APP_ID` and `THUNDERBOT_PRIVATE_KEY` secrets are repository secrets, and no workflow names an environment. A pushed branch can add an `on: push` workflow that reads them, mints the App token and posts `Review finished at <sha>`, which `fab gate` accepts. Create a GitHub environment that only `main` can deploy to, move both secrets into it, and run `fab update --environment <name>`. Found in the PR 37 adversarial pass; the owner chose to merge 0.9.2 first.
