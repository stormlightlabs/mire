package shared

import (
	"strings"
	"time"
	"unicode/utf8"
)

func BoolInt(value bool) int {
	if value {
		return 1
	}
	return 0
}

func WrapText(value string, width int) []string {
	value = strings.TrimSpace(strings.ReplaceAll(value, "\r\n", "\n"))
	if value == "" {
		return nil
	}
	lines := strings.Split(value, "\n")
	result := make([]string, 0, len(lines))
	for _, line := range lines {
		words := strings.Fields(line)
		if len(words) == 0 {
			result = append(result, "")
			continue
		}
		current := ""
		for _, word := range words {
			if current == "" {
				for RuneWidth(word) > width {
					part, rest := SplitRunes(word, width)
					result = append(result, part)
					word = rest
				}
				current = word
				continue
			}
			candidate := current + " " + word
			if RuneWidth(candidate) <= width {
				current = candidate
				continue
			}
			result = append(result, current)
			current = ""
			for RuneWidth(word) > width {
				part, rest := SplitRunes(word, width)
				result = append(result, part)
				word = rest
			}
			current = word
		}
		if current != "" {
			result = append(result, current)
		}
	}
	return result
}

func RuneWidth(value string) int {
	return utf8.RuneCountInString(value)
}

func SplitRunes(value string, width int) (string, string) {
	if width <= 0 {
		return "", value
	}
	count := 0
	for index := range value {
		if count == width {
			return value[:index], value[index:]
		}
		count++
	}
	return value, ""
}

func OptionalTime(value time.Time) string {
	if value.IsZero() {
		return ""
	}
	return value.UTC().Format(time.RFC3339Nano)
}

func Revision(value time.Time) string { return value.UTC().Format(time.RFC3339Nano) }

func ParseTimestamp(value string) (time.Time, error) {
	parsed, err := time.Parse(time.RFC3339Nano, value)
	if err != nil {
		return time.Time{}, err
	}
	return parsed, nil
}

func TimestampString(t time.Time) string {
	return t.UTC().Format(time.RFC3339Nano)
}
