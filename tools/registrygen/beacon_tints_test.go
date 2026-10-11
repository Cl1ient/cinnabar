package main

import (
	"os"
	"strings"
	"testing"
)

func TestGlassStatesHaveCompiledBeaconTints(t *testing.T) {
	data, err := os.ReadFile("../../crates/assets/data/block-registry-v2193.bin")
	if err != nil {
		t.Fatal(err)
	}
	_, records, err := decodeBREGRecords(data, v2193BlockProtocol)
	if err != nil {
		t.Fatal(err)
	}
	// The independent canonical palette supplies the complete admitted identity inventory.
	source, err := v2193SourceRecords()
	if err != nil {
		t.Fatal(err)
	}
	byHash := make(map[uint32]Record, len(records))
	for _, record := range records {
		byHash[record.NetworkHash] = record
	}
	for _, state := range source {
		if !strings.HasSuffix(state.Name, "_stained_glass") && !strings.HasSuffix(state.Name, "_stained_glass_pane") {
			continue
		}
		expected, found := beaconTints[state.Name]
		if !found {
			t.Errorf("admitted glass %s has no eligibility descriptor", state.Name)
			continue
		}
		if actual, found := byHash[state.NetworkHash]; !found || actual.BeaconTint != expected {
			t.Errorf("glass %s %s: got tint %d, want %d", state.Name, state.StateJSON, actual.BeaconTint, expected)
		}
	}
}
