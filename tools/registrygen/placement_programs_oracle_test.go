package main

import (
	"encoding/json"
	"fmt"
	"math"
	"sort"
	"strings"
)

// placementState validates the canonical typed envelope before choosing a state program.
type placementState map[string]struct {
	Type  string `json:"type"`
	Value any    `json:"value"`
}

// number accepts only canonical integer or byte values inside the requested domain.
func (s placementState) number(name string, maximum uint32) (uint32, bool) {
	entry, present := s[name]
	value, numeric := entry.Value.(float64)
	if !present || !numeric || (entry.Type != "int" && entry.Type != "byte") || value < 0 || value > float64(maximum) || math.Trunc(value) != value {
		return 0, false
	}
	return uint32(value), true
}

// text returns the numerical selector of one canonical string value.
func (s placementState) text(name string, values ...string) (uint32, bool) {
	entry, present := s[name]
	value, textual := entry.Value.(string)
	if !present || !textual || entry.Type != "string" {
		return 0, false
	}
	for index, candidate := range values {
		if candidate == value {
			return uint32(index), true
		}
	}
	return 0, false
}

// only refuses additional state fields whose effects have no program admission.
func (s placementState) only(keys ...string) bool {
	if len(s) != len(keys) {
		return false
	}
	for _, key := range keys {
		if _, found := s[key]; !found {
			return false
		}
	}
	return true
}

// placementSelector assigns one validated field to a non-overlapping state-key bit range.
type placementSelector struct {
	value uint32
	valid bool
	shift uint8
}

// packed combines bounded selectors in one immutable state key; malformed values remain unresolved.
func packed(rule BlockPlacementRule, values ...placementSelector) (BlockPlacementRule, uint32) {
	var key uint32
	for _, value := range values {
		if !value.valid {
			return BlockPlacementRuleUnresolved, 0
		}
		key |= value.value << value.shift
	}
	return rule, key
}

// slot tags a bounded selector with the non-overlapping bit range its family owns.
func slot(value uint32, valid bool, shift uint8) placementSelector {
	return placementSelector{value, valid, shift}
}

// numericSlot reads an integer selector without coercing its canonical type.
func numericSlot(s placementState, key string, max uint32, shift uint8) placementSelector {
	value, valid := s.number(key, max)
	return slot(value, valid, shift)
}

// stringSlot reads a string selector and assigns its stable numerical ordinal.
func stringSlot(s placementState, key string, shift uint8, values ...string) placementSelector {
	value, valid := s.text(key, values...)
	return slot(value, valid, shift)
}

// placementProgram retains the existing supported transformations as provisional typed rules.
func placementProgram(record Record, s placementState) (BlockPlacementRule, uint32) {
	name := record.Name
	if !strings.HasPrefix(name, "minecraft:") {
		return BlockPlacementRuleUnresolved, 0
	}
	switch {
	case len(s) == 0:
		return BlockPlacementRuleIdentity, 0
	case strings.HasSuffix(name, "_stairs") && s.only("weirdo_direction", "upside_down_bit", "minecraft:corner"):
		return packed(BlockPlacementRuleStair, numericSlot(s, "weirdo_direction", 3, 0), numericSlot(s, "upside_down_bit", 1, 2), stringSlot(s, "minecraft:corner", 3, "none", "inner_left", "inner_right", "outer_left", "outer_right"))
	case name == "minecraft:loom" && s.only("direction"):
		return packed(BlockPlacementRuleLoom, numericSlot(s, "direction", 3, 0))
	case strings.HasSuffix(name, "_glazed_terracotta") && s.only("facing_direction"):
		return packed(BlockPlacementRuleGlazed, numericSlot(s, "facing_direction", 5, 0))
	case name == "minecraft:snow_layer" && s.only("height", "covered_bit"):
		return packed(BlockPlacementRuleSnow, numericSlot(s, "height", 7, 0), numericSlot(s, "covered_bit", 1, 3))
	case name == "minecraft:sea_pickle" && s.only("cluster_count", "dead_bit"):
		return packed(BlockPlacementRuleSeaPickle, numericSlot(s, "cluster_count", 3, 0), numericSlot(s, "dead_bit", 1, 2))
	case (name == "minecraft:candle" || strings.HasSuffix(name, "_candle")) && s.only("candles", "lit"):
		return packed(BlockPlacementRuleCandle, numericSlot(s, "candles", 3, 0), numericSlot(s, "lit", 1, 2))
	case s.only("pillar_axis"):
		return packed(BlockPlacementRuleAxis, stringSlot(s, "pillar_axis", 0, "x", "y", "z"))
	case strings.HasSuffix(name, "_slab") && s.only("minecraft:vertical_half"):
		rule := BlockPlacementRuleSlab
		if strings.Contains(name, "double_") {
			rule = BlockPlacementRuleDoubleSlab
		}
		return packed(rule, stringSlot(s, "minecraft:vertical_half", 0, "bottom", "top"))
	case (name == "minecraft:trapdoor" || strings.HasSuffix(name, "_trapdoor")) && s.only("direction", "open_bit", "upside_down_bit"):
		return packed(BlockPlacementRuleTrapdoor, numericSlot(s, "direction", 3, 0), numericSlot(s, "open_bit", 1, 2), numericSlot(s, "upside_down_bit", 1, 3))
	case name == "minecraft:hopper" && s.only("facing_direction", "toggle_bit"):
		value, valid := s.number("facing_direction", 5)
		return packed(BlockPlacementRuleHopper, slot(value, valid && value != 1, 0), numericSlot(s, "toggle_bit", 1, 3))
	case (strings.HasSuffix(name, "_fence") || strings.HasSuffix(name, "_pane") || name == "minecraft:iron_bars") && s.only("minecraft:connection_north", "minecraft:connection_south", "minecraft:connection_west", "minecraft:connection_east"):
		return packed(BlockPlacementRuleConnections, numericSlot(s, "minecraft:connection_north", 1, 0), numericSlot(s, "minecraft:connection_south", 1, 1), numericSlot(s, "minecraft:connection_west", 1, 2), numericSlot(s, "minecraft:connection_east", 1, 3))
	case record.ModelFamily == ModelFamilyTorch && s.only("torch_facing_direction"):
		return packed(BlockPlacementRuleTorch, stringSlot(s, "torch_facing_direction", 0, "unknown", "top", "north", "south", "west", "east"))
	case name == "minecraft:lever" && s.only("lever_direction", "open_bit"):
		return packed(BlockPlacementRuleLever, stringSlot(s, "lever_direction", 0, "down_east_west", "down_north_south", "up_east_west", "up_north_south", "north", "south", "west", "east"), numericSlot(s, "open_bit", 1, 3))
	case (name == "minecraft:lantern" || name == "minecraft:soul_lantern" || strings.HasSuffix(name, "copper_lantern")) && s.only("hanging_bit"):
		return packed(BlockPlacementRuleLantern, numericSlot(s, "hanging_bit", 1, 0))
	case record.ModelFamily == ModelFamilyButton && s.only("facing_direction", "button_pressed_bit"):
		// Buttons share the six-face selector with their pressed bit retained for identity.
		return packed(BlockPlacementRuleButton, numericSlot(s, "facing_direction", 5, 0), numericSlot(s, "button_pressed_bit", 1, 3))
	case name == "minecraft:vine" && s.only("vine_direction_bits"):
		return packed(BlockPlacementRuleVine, numericSlot(s, "vine_direction_bits", 15, 0))
	case (name == "minecraft:glow_lichen" || name == "minecraft:sculk_vein") && s.only("multi_face_direction_bits"):
		return packed(BlockPlacementRuleMultiface, numericSlot(s, "multi_face_direction_bits", 63, 0))
	case strings.HasSuffix(name, "_door") && s.only("minecraft:cardinal_direction", "open_bit", "upper_block_bit", "door_hinge_bit"):
		return packed(BlockPlacementRuleDoor, stringSlot(s, "minecraft:cardinal_direction", 0, "south", "west", "north", "east"), numericSlot(s, "open_bit", 1, 2), numericSlot(s, "upper_block_bit", 1, 3), numericSlot(s, "door_hinge_bit", 1, 4))
	case strings.HasSuffix(name, "standing_sign") && s.only("ground_sign_direction"):
		return packed(BlockPlacementRuleStandingSign, numericSlot(s, "ground_sign_direction", 15, 0))
	case strings.HasSuffix(name, "wall_sign") && s.only("facing_direction"):
		value, valid := s.number("facing_direction", 5)
		return packed(BlockPlacementRuleWallSign, slot(value, valid && value >= 2, 0))
	case name == "minecraft:dispenser" && s.only("facing_direction", "triggered_bit"):
		return packed(BlockPlacementRuleDispenser, numericSlot(s, "facing_direction", 5, 0), numericSlot(s, "triggered_bit", 1, 3))
	case (name == "minecraft:piston" || name == "minecraft:sticky_piston") && s.only("facing_direction"):
		return packed(BlockPlacementRulePiston, numericSlot(s, "facing_direction", 5, 0))
	}
	return BlockPlacementRuleUnresolved, 0
}

// compilePlacementPrograms assigns stable numerical sets from the independent canonical name inventory.
func compilePlacementPrograms(records []Record) error {
	names := make(map[string]uint32)
	for _, record := range records {
		names[record.Name] = 0
	}
	ordered := make([]string, 0, len(names))
	for name := range names {
		ordered = append(ordered, name)
	}
	sort.Strings(ordered)
	for index, name := range ordered {
		names[name] = uint32(index) + 1
	}
	keys := make(map[[2]uint32]bool)
	for index := range records {
		record := &records[index]
		record.ModelState.PlacementRule = BlockPlacementRuleUnresolved
		record.ModelState.PlacementSet, record.ModelState.PlacementKey, record.ModelState.PlacementAlternate = 0, 0, 0
		var state placementState
		var flags uint8
		if compatibilityReplaceable[record.Name] {
			flags |= placementReplaceable
		}
		if record.Name == "minecraft:nether_brick_fence" {
			flags |= placementNetherFence
		}
		if json.Unmarshal(record.StateJSON, &state) == nil && state != nil {
			if _, found := state["minecraft:corner"]; found {
				flags |= placementCornerState
			}
			rule, key := placementProgram(*record, state)
			record.ModelState.PlacementRule, record.ModelState.PlacementKey = rule, key
			if rule == BlockPlacementRuleLever || rule == BlockPlacementRuleButton || (rule == BlockPlacementRuleTrapdoor && record.Name != "minecraft:iron_trapdoor") {
				flags |= placementHandToggle
			}
			if rule != BlockPlacementRuleUnresolved {
				set := names[record.Name]
				record.ModelState.PlacementSet = set
				identity := [2]uint32{set, key}
				if keys[identity] {
					return fmt.Errorf("duplicate compiled placement state for %s", record.Name)
				}
				keys[identity] = true
				var alternate string
				switch rule {
				case BlockPlacementRuleSlab:
					if prefix, found := strings.CutSuffix(record.Name, "cut_copper_slab"); found {
						alternate = prefix + "double_cut_copper_slab"
					} else {
						alternate = strings.TrimSuffix(record.Name, "_slab") + "_double_slab"
					}
				case BlockPlacementRuleStandingSign:
					alternate = strings.TrimSuffix(record.Name, "standing_sign") + "wall_sign"
				case BlockPlacementRuleWallSign:
					alternate = strings.TrimSuffix(record.Name, "wall_sign") + "standing_sign"
				}
				record.ModelState.PlacementAlternate = names[alternate]
			}
		} else {
			record.ModelState.PlacementRule = BlockPlacementRuleUnresolved
		}
		record.ModelState.PlacementFlags = flags
	}
	return nil
}
