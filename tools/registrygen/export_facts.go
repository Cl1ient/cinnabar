package main

import (
	"encoding/json"
	"fmt"
	"os"

	shared "github.com/bedrock-mc/protocolgen/generated/data"
	sharedbiome "github.com/bedrock-mc/protocolgen/generated/data/biome"
	sharedblock "github.com/bedrock-mc/protocolgen/generated/data/block"
)

// exportCompilerFacts emits source identities and library facts without client admission or movement policy.
func exportCompilerFacts(path string) error {
	records, err := v2193SourceRecords()
	if err != nil {
		return err
	}
	var blocks []map[string]any
	for _, r := range records {
		p, err := sharedBlockProperties(r)
		if err != nil {
			return err
		}
		boxes, ok := sharedblock.Shape(p.CollisionShape)
		if !ok {
			return fmt.Errorf("missing source collision shape for %s", r.Name)
		}
		blocks = append(blocks, map[string]any{"sequential_id": r.SequentialID, "network_hash": r.NetworkHash, "name": r.Name, "state": string(r.StateJSON), "shape_id": p.CollisionShape, "boxes": boxes, "friction": p.Friction, "emission": p.LightEmission, "filter": p.LightDampening})
	}
	var biomes []map[string]any
	if err := validateSharedBiomeTarget(); err != nil {
		return err
	}
	for _, b := range sharedbiome.All() {
		biomes = append(biomes, map[string]any{"id": b.ID, "has_id": b.HasID, "name": b.Name})
	}
	data, err := json.Marshal(map[string]any{"game_version": shared.MinecraftVersion, "protocol": shared.ProtocolVersion, "source_lock_sha256": shared.SourceLockSHA256, "cloudburst_ref": shared.CloudburstRef, "blocks": blocks, "biomes": biomes})
	if err != nil {
		return err
	}
	return os.WriteFile(path, data, 0644)
}
