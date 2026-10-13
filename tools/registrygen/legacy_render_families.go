package main

import (
	"fmt"
	"sort"
	"strings"
)

func classifyRecord(state SourceState) (Record, error) {
	state.Name = canonicalBlockName(state.Name)
	sort.Slice(state.Properties, func(i, j int) bool { return state.Properties[i].Name < state.Properties[j].Name })
	canonical, err := canonicalTypedState(state.Properties)
	if err != nil {
		return Record{}, err
	}
	record := Record{Name: state.Name, StateJSON: canonical, ContributorRole: ContributorPrimary}
	name := strings.TrimPrefix(state.Name, "minecraft:")
	switch {
	case name == "air":
		record.ModelFamily = ModelFamilyAir
		record.ContributorRole = ContributorAir
	case name == "water" || name == "flowing_water" || name == "lava" || name == "flowing_lava":
		record.ModelFamily = ModelFamilyLiquid
		record.ContributorRole = ContributorLiquidAdditional
	case strings.Contains(name, "copper_golem_statue"):
		record.ModelFamily = ModelFamilyStatue
	case name == "barrier":
		record.ModelFamily = ModelFamilyInvisible
	case name == "dragon_egg":
		record.ModelFamily = ModelFamilyDecorative
	case name == "soul_sand" || name == "mud":
		record.ModelFamily = ModelFamilyCuboid
	case isReviewedSelectorAliasCubeName(name), isGlazedTerracottaName(name):
		record.ModelFamily = ModelFamilyCube
	case strings.Contains(name, "trapdoor"):
		record.ModelFamily = ModelFamilyTrapdoor
	case strings.HasSuffix(name, "_door") || name == "wooden_door":
		record.ModelFamily = ModelFamilyDoor
	case strings.HasSuffix(name, "_stairs"):
		record.ModelFamily = ModelFamilyStair
	case strings.Contains(name, "slab"):
		record.ModelFamily = ModelFamilySlab
	case strings.Contains(name, "fence_gate"):
		record.ModelFamily = ModelFamilyGate
	case strings.HasSuffix(name, "_wall") || name == "cobblestone_wall":
		record.ModelFamily = ModelFamilyWall
	case strings.HasSuffix(name, "_fence") || name == "fence" || name == "nether_brick_fence":
		record.ModelFamily = ModelFamilyFence
	case strings.Contains(name, "glass_pane") || strings.HasSuffix(name, "_pane") || strings.HasSuffix(name, "_bars"):
		record.ModelFamily = ModelFamilyPane
	case strings.HasSuffix(name, "_bed") || name == "bed":
		record.ModelFamily = ModelFamilyBed
	case strings.Contains(name, "chest"):
		record.ModelFamily = ModelFamilyChest
	case strings.Contains(name, "sign"):
		record.ModelFamily = ModelFamilySign
	case strings.Contains(name, "rail"):
		record.ModelFamily = ModelFamilyRail
	case isTorchName(name):
		record.ModelFamily = ModelFamilyTorch
	case name == "lever":
		record.ModelFamily = ModelFamilyLever
	case strings.HasSuffix(name, "_button") || name == "stone_button":
		record.ModelFamily = ModelFamilyButton
	case strings.Contains(name, "pressure_plate"):
		record.ModelFamily = ModelFamilyPressurePlate
	case strings.HasSuffix(name, "_carpet") || name == "carpet":
		record.ModelFamily = ModelFamilyCarpet
	case name == "snow_layer" || name == "leaf_litter":
		record.ModelFamily = ModelFamilyLayer
	case isAquaticName(name):
		record.ModelFamily = ModelFamilyAquatic
	case name == "cocoa":
		record.ModelFamily = ModelFamilyCocoa
	case isCropName(name):
		record.ModelFamily = ModelFamilyCrop
	case name == "wildflowers" || name == "pink_petals":
		record.ModelFamily = ModelFamilyFlowerBed
	case name == "vine":
		record.ModelFamily = ModelFamilyVine
	case name == "glow_lichen":
		record.ModelFamily = ModelFamilyGlowLichen
	case name == "sculk_vein":
		record.ModelFamily = ModelFamilySculkVein
	case name == "chiseled_bookshelf":
		books, direction, err := chiseledBookshelfSelectors(state.Properties)
		if err != nil {
			return Record{}, err
		}
		record.ModelFamily = ModelFamilyChiseledBookshelf
		record.ModelState.Set(ModelStateConnections, books)
		record.ModelState.Set(ModelStateOrientation, direction)
	case name == "bee_nest" || name == "beehive":
		direction, honey, err := beeHousingSelectors(state.Properties)
		if err != nil {
			return Record{}, err
		}
		record.ModelFamily = ModelFamilyCube
		record.ModelState.Set(ModelStateOrientation, direction)
		record.ModelState.Set(ModelStateGrowth, honey)
	case name == "resin_clump":
		connections, err := resinClumpSelector(state.Properties)
		if err != nil {
			return Record{}, err
		}
		record.ModelFamily = ModelFamilyResinClump
		record.ModelState.Set(ModelStateConnections, connections)
	case name == "cactus":
		age, err := cactusAgeSelector(state.Properties)
		if err != nil {
			return Record{}, err
		}
		record.ModelFamily = ModelFamilyCuboid
		record.ModelState.Set(ModelStateGrowth, age)
	case name == "cake":
		bite, err := cakeBiteSelector(state.Properties)
		if err != nil {
			return Record{}, err
		}
		record.ModelFamily = ModelFamilyCuboid
		record.ModelState.Set(ModelStateGrowth, bite)
	case name == "farmland":
		amount, err := farmlandMoistureSelector(state.Properties)
		if err != nil {
			return Record{}, err
		}
		record.ModelFamily = ModelFamilyCuboid
		record.ModelState.Set(ModelStateGrowth, amount)
	case isShelfName(name):
		direction, powered, shelfType, err := shelfSelectors(state.Properties)
		if err != nil {
			return Record{}, err
		}
		record.ModelFamily = ModelFamilyCuboid
		record.ModelState.Set(ModelStateOrientation, direction)
		record.ModelState.Set(ModelStateGrowth, shelfType)
		record.ModelState.Set(ModelStateFlags, powered*modelFlagPowered)
	case isCrossName(name):
		record.ModelFamily = ModelFamilyCross
	}
	var connections uint32
	var hasConnections bool
	var variantFlags uint32
	var hasVariantFlags bool
	for _, property := range state.Properties {
		propertyName := strings.TrimPrefix(property.Name, "minecraft:")
		if propertyName == "redstone_signal" && record.ModelFamily == ModelFamilyPressurePlate {
			value, ok := scalarUint(property.Value)
			if !ok || value > 15 {
				return Record{}, fmt.Errorf("pressure plate redstone_signal is outside 0..15")
			}
			if value != 0 {
				variantFlags |= modelFlagPressed
			}
			hasVariantFlags = true
		}
		switch propertyName {
		case "weirdo_direction", "direction", "facing_direction", "ground_sign_direction", "cardinal_direction", "pillar_axis", "torch_facing_direction", "rail_direction", "lever_direction":
			if record.ModelFamily != ModelFamilySign {
				if value, ok := orientationUint(propertyName, property.Value); ok {
					record.ModelState.Set(ModelStateOrientation, value)
				}
			}
		}
		if value, ok := scalarUint(property.Value); ok {
			switch propertyName {
			case "vertical_half", "upside_down_bit", "top_slot_bit":
				record.ModelState.Set(ModelStateHalf, value)
			case "open_bit":
				record.ModelState.Set(ModelStateOpen, value)
			case "door_hinge_bit":
				record.ModelState.Set(ModelStateHinge, value)
			case "multi_face_direction_bits", "vine_direction_bits":
				connections = value
				hasConnections = true
			case "wall_post_bit":
				connections = (connections &^ (1 << 8)) | ((value & 1) << 8)
				hasConnections = true
			case "growth", "growth_stage", "age", "growing_plant_age", "kelp_age":
				record.ModelState.Set(ModelStateGrowth, value)
			case "liquid_depth":
				record.ModelState.Set(ModelStateLiquidDepth, value)
			case "rail_data_bit":
				if value != 0 {
					variantFlags |= modelFlagPowered
				}
				hasVariantFlags = true
			case "button_pressed_bit":
				if value != 0 {
					variantFlags |= modelFlagPressed
				}
				hasVariantFlags = true
			case "attached_bit":
				if value != 0 {
					variantFlags |= modelFlagAttached
				}
				hasVariantFlags = true
			case "hanging":
				if value != 0 {
					variantFlags |= modelFlagHanging
				}
				hasVariantFlags = true
			case "head_piece_bit":
				if value != 0 {
					variantFlags |= modelFlagHead
				}
				hasVariantFlags = true
			case "occupied_bit":
				if value != 0 {
					variantFlags |= modelFlagOccupied
				}
				hasVariantFlags = true
			case "in_wall_bit":
				if value != 0 {
					variantFlags |= modelFlagInWall
				}
				hasVariantFlags = true
			case "upper_block_bit":
				if value != 0 {
					variantFlags |= modelFlagUpper
				}
				hasVariantFlags = true
			}
		}
		if strings.HasPrefix(propertyName, "wall_connection_type_") && property.Value.Kind == ScalarString {
			var value uint32
			switch property.Value.String {
			case "none":
				value = 0
			case "short":
				value = 1
			case "tall":
				value = 2
			default:
				continue
			}
			var shift uint
			switch strings.TrimPrefix(propertyName, "wall_connection_type_") {
			case "north":
				shift = 0
			case "east":
				shift = 2
			case "south":
				shift = 4
			case "west":
				shift = 6
			default:
				continue
			}
			connections = (connections &^ (3 << shift)) | (value << shift)
			hasConnections = true
		}
	}
	if hasConnections {
		record.ModelState.Set(ModelStateConnections, connections)
	}
	if hasVariantFlags {
		record.ModelState.Set(ModelStateFlags, variantFlags)
	}
	if record.ModelFamily == ModelFamilySign {
		orientation, err := signOrientation(name, state.Properties)
		if err != nil {
			return Record{}, err
		}
		record.ModelState.Set(ModelStateOrientation, orientation)
	}
	if record.ModelFamily == ModelFamilySlab && strings.Contains(name, "double") {
		record.ModelState.Set(ModelStateHalf, 2)
	}
	if err := applyFamilyRouting(&record); err != nil {
		return Record{}, err
	}
	return record, nil
}
