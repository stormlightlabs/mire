package snapshot

// DivergenceStatus describes the relationship between a frozen snapshot and
// the repository state observed later.
type DivergenceStatus string

const (
	DivergenceUnchanged   DivergenceStatus = "unchanged"
	DivergenceChanged     DivergenceStatus = "changed"
	DivergenceUnavailable DivergenceStatus = "unavailable"
	DivergenceUnsupported DivergenceStatus = "unsupported"
)

// DivergenceReport is a read-only live comparison. It never changes the
// immutable snapshot it describes.
type DivergenceReport struct {
	SnapshotID    string
	Status        DivergenceStatus
	AffectedPaths []string
	AffectedRefs  []string
	Message       string
}
