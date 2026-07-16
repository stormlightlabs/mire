package terminal

import (
	"os"
	"strconv"
	"strings"

	"github.com/stormlightlabs/mire/internal/review"
	"github.com/stormlightlabs/mire/internal/shared"
)

func widthFromEnv() int {
	if value, err := strconv.Atoi(strings.TrimSpace(os.Getenv("COLUMNS"))); err == nil && value > 0 {
		return value
	}
	return DefaultWidth
}

func wrapDiffBody(value string, width int) []string {
	value = strings.TrimSuffix(strings.ReplaceAll(value, "\r\n", "\n"), "\n")
	if value == "" {
		return nil
	}
	if shared.RuneWidth(value) <= width {
		return []string{value}
	}
	parts := make([]string, 0, (shared.RuneWidth(value)+width-1)/width)
	for value != "" {
		part, rest := shared.SplitRunes(value, width)
		parts = append(parts, part)
		value = rest
	}
	return parts
}

func hasHunk(anchors []review.Anchor, hunkID string) bool {
	for _, anchor := range anchors {
		if anchor.HunkID == hunkID {
			return true
		}
	}
	return false
}
