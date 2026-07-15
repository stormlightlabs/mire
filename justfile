set shell := ["zsh", "-cu"]

default: verify

fmt:
    go fmt ./...

test:
    go test ./...

race:
    go test -race ./...

cov:
    go test -coverprofile=/tmp/mire-coverage.out ./...
    go tool cover -func=/tmp/mire-coverage.out

vet:
    go vet ./...

build:
    go build -o /tmp/mire ./cmd/mire

web:
    pnpm --dir app dev

verify: fmt test race vet build
