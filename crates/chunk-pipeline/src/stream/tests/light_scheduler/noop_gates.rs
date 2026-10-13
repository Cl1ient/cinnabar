//! Relights that cannot change light are not queued: a never-solved arrival seeds no neighbour,
//! and retiring a section whose face light every survivor cell already exceeds supports nothing.

use super::*;

const CAMERA: [f32; 3] = [24.0, 8.0, 24.0];

/// Commits a resident section and marks it changed, as an arriving section is.
fn arrive(stream: &mut WorldStream, key: SubChunkKey, section: SubChunk) {
    stream.authority.commit_sub_chunk(key, section).unwrap();
    stream.resident.insert(key);
    stream.mark_changed(key, Instant::now());
}

/// A section that has never been lit is an untrusted boundary, so its arrival relights only itself;
/// once lit, its changes relight its neighbours as before.
#[test]
fn arrival_relights_only_the_arriving_section_until_it_is_lit() {
    let mut stream = lit_stream(1);
    let lit = SubChunkKey::new(1, 0, 0, 0);
    let arriving = SubChunkKey::new(1, 1, 0, 0);
    stream
        .authority
        .commit_sub_chunk(lit, super::uniform_sub_chunk(3))
        .unwrap();
    install_current_light(&mut stream, lit, 0, 0, false);

    arrive(&mut stream, arriving, super::uniform_sub_chunk(3));
    assert!(stream.lighting.jobs.pending.contains_key(&arriving));
    assert!(!stream.lighting.jobs.pending.contains_key(&lit));
    assert!(stream.light_is_current(lit));

    settle_light(&mut stream, CAMERA);
    stream.mark_changed(arriving, Instant::now());
    assert!(stream.lighting.jobs.pending.contains_key(&lit));
}

/// Retiring a neighbour keeps a survivor's light unless the neighbour could seed a face cell up to
/// that cell's own light.
#[test]
fn eviction_relights_only_survivors_the_retired_face_could_light() {
    for (retired_block, survivor_block, relit) in [(13, 14, false), (0, 0, false), (15, 14, true)] {
        let mut stream = lit_stream(1);
        let retired = SubChunkKey::new(1, 0, 0, 0);
        let survivor = SubChunkKey::new(1, 1, 0, 0);
        for (key, block) in [(retired, retired_block), (survivor, survivor_block)] {
            stream
                .authority
                .commit_sub_chunk(key, super::uniform_sub_chunk(3))
                .unwrap();
            install_current_light(&mut stream, key, block, 0, false);
        }

        stream.evict_column(retired.chunk());

        assert_eq!(
            stream.lighting.jobs.pending.contains_key(&survivor),
            relit,
            "retired {retired_block}, survivor {survivor_block}"
        );
    }
}

/// Air, emitters, opaque and transparent cells in a fixed pseudo-random pattern.
fn patterned_sub_chunk(seed: u32) -> SubChunk {
    let mut bytes = vec![8, 1, (2 << 1) | 1];
    let mut words = [0_u32; 256];
    let mut state = seed.wrapping_mul(0x9e37_79b9) | 1;
    for linear in 0..4096_usize {
        state ^= state << 13;
        state ^= state >> 17;
        state ^= state << 5;
        let id = match state % 64 {
            0 => 1,
            1..=24 => 2,
            25..=28 => 3,
            _ => 0,
        };
        words[linear / 16] |= id << ((linear % 16) * 2);
    }
    for word in words {
        bytes.extend(word.to_le_bytes());
    }
    bytes.extend(super::super::zig_zag_i32(4));
    for id in 0..4 {
        bytes.extend(super::super::zig_zag_i32(id));
    }
    SubChunk::decode(&bytes, &world::RawBlockIds { air: 0 })
}

fn section(key: SubChunkKey) -> SubChunk {
    patterned_sub_chunk((key.x * 31 + key.y * 7 + key.z * 131 + 1000) as u32)
}

/// Light streamed in column by column, with the gated relights, and then partly retired,
/// equals the light of loading the surviving columns at once.
#[test]
fn gated_streaming_converges_to_the_light_of_a_single_load() {
    let columns = (0..3)
        .flat_map(|x| (0..3).map(move |z| ChunkKey::new(1, x, z)))
        .collect::<Vec<_>>();
    let keys = |column: ChunkKey| (0..2).map(move |y| SubChunkKey::from_chunk(column, y));
    let retired = [ChunkKey::new(1, 0, 0), ChunkKey::new(1, 0, 1)];

    let mut streamed = lit_stream(1);
    for &column in &columns {
        for key in keys(column) {
            arrive(&mut streamed, key, section(key));
        }
        settle_light(&mut streamed, CAMERA);
    }
    for column in retired {
        streamed.evict_column(column);
        settle_light(&mut streamed, CAMERA);
    }

    let surviving = columns
        .iter()
        .copied()
        .filter(|column| !retired.contains(column))
        .flat_map(keys)
        .collect::<Vec<_>>();
    let mut loaded = lit_stream(1);
    for &key in &surviving {
        loaded
            .authority
            .commit_sub_chunk(key, section(key))
            .unwrap();
        loaded.resident.insert(key);
    }
    loaded.mark_changed_sources(surviving.iter().copied(), Instant::now());
    settle_light(&mut loaded, CAMERA);

    for key in surviving {
        assert!(streamed.light_is_current(key) && loaded.light_is_current(key));
        let (streamed, loaded) = (
            streamed.lighting.store.light(key).unwrap(),
            loaded.lighting.store.light(key).unwrap(),
        );
        for channel in [LightChannel::Block, LightChannel::Sky] {
            for linear in 0..4096_u16 {
                let [x, y, z] = [linear >> 8, linear & 15, (linear >> 4) & 15].map(|v| v as u8);
                assert_eq!(
                    streamed.get(channel, x, y, z),
                    loaded.get(channel, x, y, z),
                    "{key:?} {channel:?} {x},{y},{z}"
                );
            }
        }
    }
}
