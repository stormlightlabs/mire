# deploy the kit doc site & then push
deploy-docs:
    pnpm --dir docs run deploy && git push
