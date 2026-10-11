package main

import (
	"testing"
)

// placementFixture keeps canonical typed input independent from the compiled packed selectors.
func placementFixture(name, state string) Record {
	return Record{Name: name, StateJSON: []byte(state)}
}

func TestPlacementProgramsJoinIndependentIdentitiesAndRequireCollectedAlternates(t *testing.T) {
	records := []Record{
		placementFixture("fixture:custom", `{}`),
		placementFixture("minecraft:oak_slab", `{"minecraft:vertical_half":{"type":"string","value":"bottom"}}`),
		placementFixture("minecraft:oak_slab", `{"minecraft:vertical_half":{"type":"string","value":"top"}}`),
		placementFixture("minecraft:oak_double_slab", `{"minecraft:vertical_half":{"type":"string","value":"bottom"}}`),
		placementFixture("minecraft:stone", `{}`),
		placementFixture("minecraft:unadmitted", `{"pillar_axis":{"type":"string","value":"x"},"fixture:effect":{"type":"byte","value":0}}`),
	}
	if err := compilePlacementPrograms(records); err != nil {
		t.Fatal(err)
	}
	if records[0].ModelState.PlacementRule != BlockPlacementRuleUnresolved || records[0].ModelState.PlacementSet != 0 {
		t.Fatal("custom identity silently admitted")
	}
	for index, key := range []uint32{0, 1} {
		facts := records[index+1].ModelState
		if facts.PlacementRule != BlockPlacementRuleSlab || facts.PlacementSet != 3 || facts.PlacementKey != key || facts.PlacementAlternate != 2 {
			t.Fatalf("wrong independent slab join: %#v", facts)
		}
	}
	if records[5].ModelState.PlacementRule != BlockPlacementRuleUnresolved {
		t.Fatal("extra state effect silently admitted")
	}
	missing := []Record{records[1]}
	if err := compilePlacementPrograms(missing); err != nil {
		t.Fatal(err)
	}
	if missing[0].ModelState.PlacementAlternate != 0 {
		t.Fatal("compiler manufactured an absent double state")
	}
	duplicate := []Record{records[1], records[1]}
	if compilePlacementPrograms(duplicate) == nil {
		t.Fatal("duplicate compiled state key admitted")
	}
}

func TestPlacementProgramsKeepInvalidTypesAndSurvivalExplicitlyUnresolved(t *testing.T) {
	records := []Record{
		placementFixture("minecraft:fixture", `{"pillar_axis":{"type":"int","value":0}}`),
		placementFixture("minecraft:fixture", `{"pillar_axis":{"type":"string","value":"w"}}`),
		placementFixture("minecraft:fixture", `null`),
		placementFixture("minecraft:unknown_survival", `{}`),
	}
	if err := compilePlacementPrograms(records); err != nil {
		t.Fatal(err)
	}
	for _, record := range records[:3] {
		if record.ModelState.PlacementRule != BlockPlacementRuleUnresolved {
			t.Fatal("malformed program admitted")
		}
	}
	rule, _, _ := compiledSurvival(records[3])
	if rule != BlockSurvivalRuleUnresolved {
		t.Fatal("unknown survival defaulted to independence")
	}
}

func TestPlacementProgramsKeepIronTrapdoorsOutOfHandToggleAdmission(t *testing.T) {
	state := `{"direction":{"type":"int","value":2},"open_bit":{"type":"byte","value":1},"upside_down_bit":{"type":"byte","value":0}}`
	records := []Record{placementFixture("minecraft:iron_trapdoor", state), placementFixture("minecraft:trapdoor", state)}
	if err := compilePlacementPrograms(records); err != nil {
		t.Fatal(err)
	}
	if records[0].ModelState.PlacementFlags&placementHandToggle != 0 || records[1].ModelState.PlacementFlags&placementHandToggle == 0 {
		t.Fatal("hand-toggle admission changed")
	}
}
