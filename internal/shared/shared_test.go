package shared

import "testing"

func TestCloneMapCopiesEntriesWithoutSharingTheMap(t *testing.T) {
	t.Parallel()

	original := map[string]any{"temperature": 0.2, "nested": []string{"value"}}
	clone := CloneMap(original)
	if len(clone) != len(original) || clone["temperature"] != original["temperature"] {
		t.Fatalf("CloneMap() = %#v, want entries from %#v", clone, original)
	}
	clone["top_p"] = 0.9
	if _, ok := original["top_p"]; ok {
		t.Fatal("CloneMap() shares the caller's map")
	}
	if CloneMap(nil) != nil || CloneMap(map[string]any{}) != nil {
		t.Fatal("CloneMap() should return nil for empty maps")
	}
}

func TestUniqueStringsPreservesFirstOccurrenceOrder(t *testing.T) {
	t.Parallel()

	values := []string{"first", "", "second", "first", "third", "second", ""}
	got := UniqueStrings(values)
	want := []string{"first", "second", "third"}
	if len(got) != len(want) {
		t.Fatalf("UniqueStrings() length = %d, want %d", len(got), len(want))
	}
	for index := range want {
		if got[index] != want[index] {
			t.Fatalf("UniqueStrings()[%d] = %q, want %q; full result %#v", index, got[index], want[index], got)
		}
	}
	if UniqueStrings(nil) != nil || UniqueStrings([]string{}) != nil {
		t.Fatal("UniqueStrings() should return nil for empty input")
	}
}
