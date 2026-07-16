set shell := ["zsh", "-cu"]

default: verify

fumpt:
    gofumpt -l -w .

wrap-lines:
    golines -w . -m 120

fmt: fumpt wrap-lines

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
