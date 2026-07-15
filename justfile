set shell := ["zsh", "-cu"]

default: verify

fmt:
    go fmt ./...

test:
    go test ./...

race:
    go test -race ./...

vet:
    go vet ./...

build:
    go build -o /tmp/mire ./cmd/mire

verify: fmt test race vet build
