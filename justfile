# deploy the kit doc site & then push
deploy-docs:
    pnpm --filter @stormlightlabs/mire-docs run deploy && git push
