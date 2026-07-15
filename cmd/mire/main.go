package main

import (
	"context"
	"os"
	"os/signal"
	"syscall"

	"github.com/stormlightlabs/mire/internal/cli"
	"github.com/stormlightlabs/mire/internal/echo"
)

func main() {
	ctx, stop := signal.NotifyContext(context.Background(), os.Interrupt, syscall.SIGTERM)
	defer stop()
	if err := cli.Execute(ctx); err != nil {
		echo.NewLogger(os.Stderr).Error("Command failed", "error", err)
		os.Exit(1)
	}
}
