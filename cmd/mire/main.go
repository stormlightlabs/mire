package main

import (
	"context"
	"os"

	"github.com/stormlightlabs/mire/internal/cli"
	"github.com/stormlightlabs/mire/internal/echo"
)

func main() {
	if err := cli.Execute(context.Background()); err != nil {
		echo.NewLogger(os.Stderr).Error("Command failed", "error", err)
		os.Exit(1)
	}
}
