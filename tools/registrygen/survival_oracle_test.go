package main

// compiledSurvival retains provisional support rules; unadmitted identities stay unresolved.
func compiledSurvival(record Record) (BlockSurvivalRule, BlockSupportPolicy, BlockSupportKind) {
	rule, policy, kind := BlockSurvivalRuleUnresolved, BlockSupportPolicyUnresolved, BlockSupportKindUnresolved
	switch record.Name {
	case "minecraft:air":
		kind = BlockSupportKindAir
	case "minecraft:water", "minecraft:flowing_water", "minecraft:lava", "minecraft:flowing_lava":
		kind = BlockSupportKindFluid
	case "minecraft:powder_snow":
		kind = BlockSupportKindPowderSnow
	case "minecraft:stone":
		kind = BlockSupportKindStone
	case "minecraft:snow_layer":
		kind = BlockSupportKindSnow
	case "minecraft:fire", "minecraft:soul_fire":
		kind = BlockSupportKindRejected
	default:
		if record.ModelFamily == ModelFamilyLeaves {
			kind = BlockSupportKindLeaves
		} else if record.Flags&flagCubeGeometry != 0 && record.Flags&flagOccludesFullFace != 0 {
			kind = BlockSupportKindFullCube
		}
	}
	face := func(value uint32) BlockSurvivalRule {
		return BlockSurvivalRule(uint8(BlockSurvivalRuleDown) + uint8(value))
	}
	key := record.ModelState.PlacementKey
	switch record.ModelState.PlacementRule {
	case BlockPlacementRuleAxis, BlockPlacementRuleLoom, BlockPlacementRuleGlazed, BlockPlacementRuleStair,
		BlockPlacementRuleSlab, BlockPlacementRuleDoubleSlab, BlockPlacementRuleTrapdoor, BlockPlacementRuleHopper,
		BlockPlacementRuleConnections, BlockPlacementRuleDispenser, BlockPlacementRulePiston:
		rule = BlockSurvivalRuleIndependent
	case BlockPlacementRuleTorch:
		policy = BlockSupportPolicyFullSolid
		if key == 1 {
			rule = face(0)
		} else if key >= 2 {
			rule = face(key)
		}
	case BlockPlacementRuleButton:
		rule, policy = face((key&7)^1), BlockSupportPolicyFullSolid
	case BlockPlacementRuleLever:
		rule, policy = face([8]uint32{1, 1, 0, 0, 3, 2, 5, 4}[key&7]), BlockSupportPolicyFullSolid
	case BlockPlacementRuleLantern:
		rule, policy = face(key), BlockSupportPolicyFullSolid
	case BlockPlacementRuleCandle, BlockPlacementRuleSeaPickle, BlockPlacementRuleDoor, BlockPlacementRuleStandingSign:
		rule, policy = BlockSurvivalRuleDown, BlockSupportPolicyFullSolid
	case BlockPlacementRuleWallSign:
		rule, policy = face(key^1), BlockSupportPolicyFullSolid
	case BlockPlacementRuleSnow:
		rule, policy = BlockSurvivalRuleDown, BlockSupportPolicySnow
	case BlockPlacementRuleVine:
		policy = BlockSupportPolicyVine
		for index, bit := range []uint32{1, 4, 8, 2} {
			if key == bit {
				rule = face((uint32(index) + 2) ^ 1)
			}
		}
	case BlockPlacementRuleMultiface:
		policy = BlockSupportPolicyFullSolid
		for index, bit := range []uint32{1, 2, 16, 4, 8, 32} {
			if key == bit {
				rule = face(uint32(index))
			}
		}
	case BlockPlacementRuleIdentity:
		if record.Name == "minecraft:stone" {
			rule = BlockSurvivalRuleIndependent
		}
		if record.ModelFamily == ModelFamilyCarpet && record.Name != "minecraft:moss_carpet" && record.Name != "minecraft:pale_moss_carpet" {
			rule, policy = BlockSurvivalRuleDown, BlockSupportPolicyNonAir
		}
	}
	return rule, policy, kind
}
