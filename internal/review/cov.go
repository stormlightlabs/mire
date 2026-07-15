package review

import (
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"sort"
)

// ReviewCoverage is the honest, cumulative coverage record for one review
// round. It describes work performed and omissions, never semantic certainty.
type ReviewCoverage struct {
	ExaminedFiles      []string               `json:"examined_files"`
	ExaminedHunks      []string               `json:"examined_hunks"`
	RetrievedArtifacts []RetrievedArtifact    `json:"retrieved_artifacts,omitempty"`
	Passes             []PassCoverage         `json:"passes"`
	Analyzers          []AnalyzerAvailability `json:"analyzers,omitempty"`
	Exclusions         []CoverageExclusion    `json:"exclusions,omitempty"`
	Failures           []CoverageFailure      `json:"failures,omitempty"`
	Gaps               []string               `json:"gaps,omitempty"`
	Digest             string                 `json:"digest"`
}

func (c ReviewCoverage) clone() ReviewCoverage {
	copyCoverage := c
	copyCoverage.ExaminedFiles = append([]string(nil), c.ExaminedFiles...)
	copyCoverage.ExaminedHunks = append([]string(nil), c.ExaminedHunks...)
	copyCoverage.RetrievedArtifacts = append([]RetrievedArtifact(nil), c.RetrievedArtifacts...)
	copyCoverage.Passes = append([]PassCoverage(nil), c.Passes...)
	copyCoverage.Analyzers = append([]AnalyzerAvailability(nil), c.Analyzers...)
	copyCoverage.Exclusions = append([]CoverageExclusion(nil), c.Exclusions...)
	copyCoverage.Failures = append([]CoverageFailure(nil), c.Failures...)
	copyCoverage.Gaps = append([]string(nil), c.Gaps...)
	copyCoverage.Digest = ""
	return copyCoverage
}

func (c ReviewCoverage) normalize() ReviewCoverage {
	c.ExaminedFiles = uniqueStringsSorted(c.ExaminedFiles)
	c.ExaminedHunks = uniqueStringsSorted(c.ExaminedHunks)
	sort.SliceStable(c.RetrievedArtifacts, func(i, j int) bool { return c.RetrievedArtifacts[i].ID < c.RetrievedArtifacts[j].ID })
	sort.SliceStable(c.Passes, func(i, j int) bool {
		if c.Passes[i].Order != c.Passes[j].Order {
			return c.Passes[i].Order < c.Passes[j].Order
		}
		return c.Passes[i].Name < c.Passes[j].Name
	})
	sort.SliceStable(c.Analyzers, func(i, j int) bool { return c.Analyzers[i].Name < c.Analyzers[j].Name })
	c.Gaps = uniqueStringsSorted(c.Gaps)
	withoutDigest := c
	withoutDigest.Digest = ""
	data, err := json.Marshal(withoutDigest)
	if err == nil {
		digest := sha256.Sum256(data)
		c.Digest = hex.EncodeToString(digest[:])
	}
	return c
}
