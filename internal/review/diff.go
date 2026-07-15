package review

import (
	"crypto/sha256"
	"encoding/hex"
	"fmt"
	"strings"
)

type diffLine struct {
	kind byte
	text string
}

// diffHunks computes a bounded, deterministic line diff. Large files use one
// whole-file hunk to avoid quadratic memory use while retaining exact bytes in
// the hunk digest and patch.
func diffHunks(basePath, targetPath string, oldBytes, newBytes []byte) ([]Hunk, string) {
	if isBinary(oldBytes) || isBinary(newBytes) {
		hunk := makeHunk(targetPath, "binary", 1, lineCount(oldBytes), 1, lineCount(newBytes), []string{"Binary files differ."}, true, true)
		return []Hunk{hunk}, unifiedPatch(basePath, targetPath, []string{"Binary files differ."})
	}
	oldLines, newLines := splitLines(string(oldBytes)), splitLines(string(newBytes))
	if len(oldLines)+len(newLines) > 4000 {
		lines := make([]string, 0, len(oldLines)+len(newLines))
		for _, line := range oldLines {
			lines = append(lines, "-"+line)
		}
		for _, line := range newLines {
			lines = append(lines, "+"+line)
		}
		hunk := makeHunk(targetPath, "changed", 1, len(oldLines), 1, len(newLines), lines, false, true)
		return []Hunk{hunk}, unifiedPatch(basePath, targetPath, lines)
	}
	ops := lcsDiff(oldLines, newLines)
	hunks := groupHunks(targetPath, ops)
	patchLines := make([]string, 0)
	for _, hunk := range hunks {
		patchLines = append(patchLines, hunk.Lines...)
	}
	return hunks, unifiedPatch(basePath, targetPath, patchLines)
}

func isBinary(content []byte) bool {
	for _, value := range content {
		if value == 0 {
			return true
		}
	}
	return false
}

func lineCount(content []byte) int {
	if len(content) == 0 {
		return 0
	}
	return len(splitLines(string(content)))
}

func splitLines(content string) []string {
	if content == "" {
		return nil
	}
	lines := strings.SplitAfter(content, "\n")
	if lines[len(lines)-1] == "" {
		lines = lines[:len(lines)-1]
	}
	return lines
}

func lcsDiff(oldLines, newLines []string) []diffLine {
	rows, columns := len(oldLines), len(newLines)
	lcs := make([][]int, rows+1)
	for row := range lcs {
		lcs[row] = make([]int, columns+1)
	}
	for row := rows - 1; row >= 0; row-- {
		for column := columns - 1; column >= 0; column-- {
			if oldLines[row] == newLines[column] {
				lcs[row][column] = lcs[row+1][column+1] + 1
			} else if lcs[row+1][column] >= lcs[row][column+1] {
				lcs[row][column] = lcs[row+1][column]
			} else {
				lcs[row][column] = lcs[row][column+1]
			}
		}
	}
	ops := make([]diffLine, 0, rows+columns)
	row, column := 0, 0
	for row < rows && column < columns {
		if oldLines[row] == newLines[column] {
			ops = append(ops, diffLine{kind: ' ', text: oldLines[row]})
			row++
			column++
			continue
		}
		if lcs[row+1][column] >= lcs[row][column+1] {
			ops = append(ops, diffLine{kind: '-', text: oldLines[row]})
			row++
		} else {
			ops = append(ops, diffLine{kind: '+', text: newLines[column]})
			column++
		}
	}
	for row < rows {
		ops = append(ops, diffLine{kind: '-', text: oldLines[row]})
		row++
	}
	for column < columns {
		ops = append(ops, diffLine{kind: '+', text: newLines[column]})
		column++
	}
	return ops
}

func groupHunks(path string, ops []diffLine) []Hunk {
	changed := make([]int, 0)
	for index, op := range ops {
		if op.kind != ' ' {
			changed = append(changed, index)
		}
	}
	if len(changed) == 0 {
		return nil
	}
	groups := make([][2]int, 0)
	start, end := changed[0], changed[0]
	for _, index := range changed[1:] {
		if index <= end+6 {
			end = index
		} else {
			groups = append(groups, [2]int{start, end})
			start, end = index, index
		}
	}
	groups = append(groups, [2]int{start, end})
	hunks := make([]Hunk, 0, len(groups))
	for _, group := range groups {
		group[0] -= 3
		if group[0] < 0 {
			group[0] = 0
		}
		group[1] += 3
		if group[1] >= len(ops) {
			group[1] = len(ops) - 1
		}
		oldLine, newLine := 1, 1
		for _, op := range ops[:group[0]] {
			if op.kind != '+' {
				oldLine++
			}
			if op.kind != '-' {
				newLine++
			}
		}
		startOld, startNew := oldLine, newLine
		lines := make([]string, 0, group[1]-group[0]+1)
		for _, op := range ops[group[0] : group[1]+1] {
			lines = append(lines, string(op.kind)+op.text)
			if op.kind != '+' {
				oldLine++
			}
			if op.kind != '-' {
				newLine++
			}
		}
		oldCount, newCount := 0, 0
		for _, line := range lines {
			if line[0] != '+' {
				oldCount++
			}
			if line[0] != '-' {
				newCount++
			}
		}
		hunks = append(hunks, makeHunk(path, "changed", startOld, oldCount, startNew, newCount, lines, false, true))
	}
	return hunks
}

func makeHunk(path, kind string, oldStart, oldLines, newStart, newLines int, lines []string, binary, available bool) Hunk {
	digest := sha256.Sum256([]byte(fmt.Sprintf("%s\x00%s\x00%d\x00%d\x00%d\x00%d\x00%t\x00%s", path, kind, oldStart, oldLines, newStart, newLines, binary, strings.Join(lines, ""))))
	hexDigest := hex.EncodeToString(digest[:])
	return Hunk{ID: path + "#" + hexDigest[:16], Kind: kind, OldStart: oldStart, OldLines: oldLines, NewStart: newStart, NewLines: newLines, Lines: lines, Binary: binary, Available: available, Digest: hexDigest}
}

func unifiedPatch(basePath, targetPath string, lines []string) string {
	if basePath == "" {
		basePath = "/dev/null"
	} else {
		basePath = "a/" + basePath
	}
	if targetPath == "" {
		targetPath = "/dev/null"
	} else {
		targetPath = "b/" + targetPath
	}
	return "--- " + basePath + "\n+++ " + targetPath + "\n" + strings.Join(lines, "")
}
