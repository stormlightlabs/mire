package echo

import (
	"bytes"
	"strings"
	"testing"
	"time"

	"github.com/stormlightlabs/mire/internal/db"
)

func TestRenderSessionsRespectsNoColor(t *testing.T) {
	t.Setenv("NO_COLOR", "1")

	var output bytes.Buffer
	err := RenderSessions(&output, []db.Session{{
		ID:             "session-id",
		RepositoryName: "mire",
		Title:          "Review",
		CreatedAt:      time.Date(2026, time.July, 14, 12, 0, 0, 0, time.UTC),
	}})
	if err != nil {
		t.Fatalf("RenderSessions() error = %v", err)
	}
	if strings.Contains(output.String(), "[") {
		t.Fatalf("rendered output contains ANSI escape sequence: %q", output.String())
	}
	if !strings.Contains(output.String(), "session-id") {
		t.Fatalf("rendered output = %q, missing session ID", output.String())
	}
}

func TestLoggerWritesDiagnosticsToProvidedOutput(t *testing.T) {
	t.Setenv("NO_COLOR", "1")

	var output bytes.Buffer
	NewLogger(&output).Error("Command failed", "error", "session does not exist")
	if !strings.Contains(output.String(), "Command failed") || !strings.Contains(output.String(), "session does not exist") {
		t.Fatalf("logger output = %q, missing diagnostic", output.String())
	}
	if strings.Contains(output.String(), "[") {
		t.Fatalf("logger output contains ANSI escape sequence: %q", output.String())
	}
}
