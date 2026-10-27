//! Strategies and round-trip tests for every clientbound play packet
//! (ids `0x00..=0x49`).

use proptest::array::{uniform3, uniform4, uniform5, uniform7};
use proptest::collection::vec;
use proptest::prelude::*;
use umicraft_protocol::packets::play::clientbound::chunk_data::chunk_payload_len;
use umicraft_protocol::packets::play::{clientbound, PlayClientbound};

use crate::common::*;

fn entity_properties() -> impl Strategy<Value = clientbound::EntityProperties> {
    let modifier = (any::<u128>(), f64s(), any::<i8>()).prop_map(|(uuid, amount, operation)| {
        clientbound::AttributeModifier {
            uuid,
            amount,
            operation,
        }
    });
    let property = (text(64), f64s(), vec(modifier, 0..3)).prop_map(|(key, value, modifiers)| {
        clientbound::EntityProperty {
            key,
            value,
            modifiers,
        }
    });
    (any::<i32>(), vec(property, 0..3)).prop_map(|(entity_id, properties)| {
        clientbound::EntityProperties {
            entity_id,
            properties,
        }
    })
}

/// Only canonical particles: an out-of-range wire id decodes as the barrier
/// particle (35) and the argument count is fixed by the id, so the generator
/// picks ids in `0..=41` and exactly the arguments that id carries.
fn particle() -> impl Strategy<Value = clientbound::Particle> {
    (0i32..=41).prop_flat_map(|particle_id| {
        let arguments = match particle_id {
            36 => 2,
            37 | 38 => 1,
            _ => 0,
        };
        (
            any::<bool>(),
            uniform7(f32s()),
            any::<i32>(),
            vec(any::<i32>(), arguments),
        )
            .prop_map(
                move |(
                    long_distance,
                    [x, y, z, offset_x, offset_y, offset_z, speed],
                    count,
                    arguments,
                )| {
                    clientbound::Particle {
                        particle_id,
                        long_distance,
                        x,
                        y,
                        z,
                        offset_x,
                        offset_y,
                        offset_z,
                        speed,
                        count,
                        arguments,
                    }
                },
            )
    })
}

fn maps() -> impl Strategy<Value = clientbound::Maps> {
    let icon =
        (0u8..16, 0u8..16, any::<i8>(), any::<i8>()).prop_map(|(icon_type, direction, x, z)| {
            clientbound::MapIcon {
                icon_type,
                direction,
                x,
                z,
            }
        });
    // `columns == 0` means "no pixel update" on the wire, so `Some` needs > 0.
    let columns = proptest::option::of(
        (1u8..=255, any::<u8>(), any::<u8>(), any::<u8>(), bytes(64)).prop_map(
            |(columns, rows, x, z, data)| clientbound::MapColumns {
                columns,
                rows,
                x,
                z,
                data,
            },
        ),
    );
    (any::<i32>(), any::<i8>(), vec(icon, 0..4), columns).prop_map(
        |(map_id, scale, icons, columns)| clientbound::Maps {
            map_id,
            scale,
            icons,
            columns,
        },
    )
}

/// The decoder derives each chunk's payload length from its bit mask, so the
/// generated payloads have exactly that length (at most one section each to
/// keep the cases small).
fn map_chunk_bulk() -> impl Strategy<Value = clientbound::MapChunkBulk> {
    let mask = prop_oneof![Just(0u16), (0u32..16).prop_map(|bit| 1u16 << bit)];
    (any::<bool>(), vec((any::<i32>(), any::<i32>(), mask), 0..3))
        .prop_flat_map(|(sky_light, metas)| {
            let payloads: Vec<_> = metas
                .iter()
                .map(|&(_, _, mask)| vec(any::<u8>(), chunk_payload_len(mask, sky_light, true)))
                .collect();
            (Just(sky_light), Just(metas), payloads)
        })
        .prop_map(|(sky_light, metas, payloads)| clientbound::MapChunkBulk {
            sky_light,
            chunks: metas
                .into_iter()
                .zip(payloads)
                .map(
                    |((chunk_x, chunk_z, primary_bit_mask), data)| clientbound::BulkChunk {
                        chunk_x,
                        chunk_z,
                        primary_bit_mask,
                        data,
                    },
                )
                .collect(),
        })
}

fn player_list_item() -> impl Strategy<Value = clientbound::PlayerListItem> {
    use clientbound::PlayerListItem as Item;
    let property = (text(40), text(40), proptest::option::of(text(40))).prop_map(
        |(name, value, signature)| clientbound::PlayerProperty {
            name,
            value,
            signature,
        },
    );
    let add = (
        any::<u128>(),
        text(16),
        vec(property, 0..3),
        any::<i32>(),
        any::<i32>(),
        proptest::option::of(text(40)),
    )
        .prop_map(|(uuid, name, properties, game_mode, ping, display_name)| {
            clientbound::AddPlayerEntry {
                uuid,
                name,
                properties,
                game_mode,
                ping,
                display_name,
            }
        });
    let uuid_int = (any::<u128>(), any::<i32>());
    prop_oneof![
        vec(add, 0..3).prop_map(Item::AddPlayer),
        vec(uuid_int, 0..4).prop_map(Item::UpdateGameMode),
        vec(uuid_int, 0..4).prop_map(Item::UpdateLatency),
        vec((any::<u128>(), proptest::option::of(text(40))), 0..4)
            .prop_map(Item::UpdateDisplayName),
        vec(any::<u128>(), 0..4).prop_map(Item::RemovePlayer),
    ]
}

fn render_type() -> impl Strategy<Value = clientbound::ObjectiveRenderType> {
    // Unknown render-type names decode as `Integer`, so only the two
    // canonical names are produced.
    prop_oneof![
        Just(clientbound::ObjectiveRenderType::Integer),
        Just(clientbound::ObjectiveRenderType::Hearts)
    ]
}

fn scoreboard_objective() -> impl Strategy<Value = clientbound::ScoreboardObjective> {
    use clientbound::ScoreboardObjective as Objective;
    prop_oneof![
        (text(16), text(32), render_type()).prop_map(|(name, value, render_type)| {
            Objective::Create {
                name,
                value,
                render_type,
            }
        }),
        text(16).prop_map(|name| Objective::Remove { name }),
        (text(16), text(32), render_type()).prop_map(|(name, value, render_type)| {
            Objective::Update {
                name,
                value,
                render_type,
            }
        }),
    ]
}

fn teams() -> impl Strategy<Value = clientbound::Teams> {
    use clientbound::Teams;
    let info = (
        text(32),
        text(16),
        text(16),
        any::<i8>(),
        text(32),
        any::<i8>(),
    )
        .prop_map(
            |(display_name, prefix, suffix, friendly_flags, name_tag_visibility, color)| {
                clientbound::TeamInfo {
                    display_name,
                    prefix,
                    suffix,
                    friendly_flags,
                    name_tag_visibility,
                    color,
                }
            },
        );
    let players = || vec(text(40), 0..4);
    prop_oneof![
        (text(16), info.clone(), players()).prop_map(|(name, info, players)| Teams::Create {
            name,
            info,
            players
        }),
        text(16).prop_map(|name| Teams::Remove { name }),
        (text(16), info).prop_map(|(name, info)| Teams::Update { name, info }),
        (text(16), players()).prop_map(|(name, players)| Teams::AddPlayers { name, players }),
        (text(16), players()).prop_map(|(name, players)| Teams::RemovePlayers { name, players }),
    ]
}

fn title() -> impl Strategy<Value = clientbound::Title> {
    use clientbound::Title;
    prop_oneof![
        text(40).prop_map(|json| Title::Title { json }),
        text(40).prop_map(|json| Title::Subtitle { json }),
        (any::<i32>(), any::<i32>(), any::<i32>()).prop_map(|(fade_in, stay, fade_out)| {
            Title::Times {
                fade_in,
                stay,
                fade_out,
            }
        }),
        Just(Title::Clear),
        Just(Title::Reset),
    ]
}

fn world_border() -> impl Strategy<Value = clientbound::WorldBorder> {
    use clientbound::WorldBorder as Border;
    prop_oneof![
        f64s().prop_map(|diameter| Border::SetSize { diameter }),
        (f64s(), f64s(), any::<i64>()).prop_map(|(old_diameter, new_diameter, speed)| {
            Border::LerpSize {
                old_diameter,
                new_diameter,
                speed,
            }
        }),
        (f64s(), f64s()).prop_map(|(x, z)| Border::SetCenter { x, z }),
        (
            (f64s(), f64s(), f64s(), f64s()),
            any::<i64>(),
            (any::<i32>(), any::<i32>(), any::<i32>())
        )
            .prop_map(
                |(
                    (x, z, old_diameter, new_diameter),
                    speed,
                    (portal_boundary, warning_time, warning_blocks),
                )| Border::Initialize {
                    x,
                    z,
                    old_diameter,
                    new_diameter,
                    speed,
                    portal_boundary,
                    warning_time,
                    warning_blocks,
                }
            ),
        any::<i32>().prop_map(|warning_time| Border::SetWarningTime { warning_time }),
        any::<i32>().prop_map(|warning_blocks| Border::SetWarningBlocks { warning_blocks }),
    ]
}

fn combat_event() -> impl Strategy<Value = clientbound::CombatEvent> {
    use clientbound::CombatEvent as Event;
    prop_oneof![
        Just(Event::EnterCombat),
        (any::<i32>(), any::<i32>()).prop_map(|(duration, entity_id)| Event::EndCombat {
            duration,
            entity_id
        }),
        (any::<i32>(), any::<i32>(), text(40)).prop_map(|(player_id, entity_id, message)| {
            Event::EntityDied {
                player_id,
                entity_id,
                message,
            }
        }),
    ]
}

fn update_score() -> impl Strategy<Value = clientbound::UpdateScore> {
    use clientbound::UpdateScore as Score;
    prop_oneof![
        (text(40), text(16), any::<i32>()).prop_map(|(name, objective, value)| Score::Change {
            name,
            objective,
            value
        }),
        (text(40), text(16)).prop_map(|(name, objective)| Score::Remove { name, objective }),
    ]
}

/// The horse window is the only one carrying an entity id on the wire.
fn open_window() -> impl Strategy<Value = clientbound::OpenWindow> {
    let window_type = prop_oneof![
        text(32).prop_filter("not the horse window", |t| t != "EntityHorse"),
        Just("EntityHorse".to_string()),
    ];
    (
        any::<u8>(),
        window_type,
        text(40),
        any::<u8>(),
        any::<i32>(),
    )
        .prop_map(|(window_id, window_type, title, slot_count, entity)| {
            let entity_id = (window_type == "EntityHorse").then_some(entity);
            clientbound::OpenWindow {
                window_id,
                window_type,
                title,
                slot_count,
                entity_id,
            }
        })
}

play_roundtrips!(PlayClientbound, clientbound, CLIENTBOUND_COVERED, {
    KeepAlive, DEFAULT => any::<i32>().prop_map(|id| clientbound::KeepAlive { id });
    JoinGame, DEFAULT => (any::<i32>(), any::<u8>(), any::<i8>(), any::<u8>(), any::<u8>(), text(16), any::<bool>())
        .prop_map(|(entity_id, gamemode, dimension, difficulty, max_players, level_type, reduced_debug_info)| clientbound::JoinGame {
            entity_id, gamemode, dimension, difficulty, max_players, level_type, reduced_debug_info,
        });
    Chat, DEFAULT => (text(256), any::<u8>()).prop_map(|(json, position)| clientbound::Chat { json, position });
    TimeUpdate, DEFAULT => (any::<i64>(), any::<i64>())
        .prop_map(|(world_age, time_of_day)| clientbound::TimeUpdate { world_age, time_of_day });
    EntityEquipment, DEFAULT => (any::<i32>(), any::<i16>(), item_stack())
        .prop_map(|(entity_id, slot, item)| clientbound::EntityEquipment { entity_id, slot, item });
    SpawnPosition, DEFAULT => block_pos().prop_map(|(x, y, z)| clientbound::SpawnPosition { x, y, z });
    UpdateHealth, DEFAULT => (f32s(), any::<i32>(), f32s())
        .prop_map(|(health, food, saturation)| clientbound::UpdateHealth { health, food, saturation });
    Respawn, DEFAULT => (any::<i32>(), any::<u8>(), any::<u8>(), text(16))
        .prop_map(|(dimension, difficulty, game_mode, world_type)| clientbound::Respawn {
            dimension, difficulty, game_mode, world_type,
        });
    PositionLook, DEFAULT => (f64s(), f64s(), f64s(), f32s(), f32s(), any::<u8>())
        .prop_map(|(x, y, z, yaw, pitch, flags)| clientbound::PositionLook { x, y, z, yaw, pitch, flags });
    HeldItemChange, DEFAULT => any::<i8>().prop_map(|slot| clientbound::HeldItemChange { slot });
    Animation, DEFAULT => (any::<i32>(), any::<u8>())
        .prop_map(|(entity_id, animation)| clientbound::Animation { entity_id, animation });
    SpawnPlayer, DEFAULT => (any::<i32>(), any::<[u8; 16]>(), any::<i32>(), any::<i32>(), any::<i32>(), any::<i8>(), any::<i8>(), any::<i16>(), metadata())
        .prop_map(|(entity_id, uuid, x, y, z, yaw, pitch, held_item, metadata)| clientbound::SpawnPlayer {
            entity_id, uuid, x, y, z, yaw, pitch, held_item, metadata,
        });
    // The velocity triple is only on the wire when `data > 0`, so otherwise
    // it must be zero to be canonical.
    SpawnObject, DEFAULT => (any::<i32>(), any::<u8>(), uniform3(any::<i32>()), any::<i8>(), any::<i8>(), any::<i32>(), uniform3(any::<i16>()))
        .prop_map(|(entity_id, object_type, [x, y, z], pitch, yaw, data, velocity)| {
            let [velocity_x, velocity_y, velocity_z] = if data > 0 { velocity } else { [0; 3] };
            clientbound::SpawnObject {
                entity_id, object_type, x, y, z, pitch, yaw, data, velocity_x, velocity_y, velocity_z,
            }
        });
    SpawnMob, DEFAULT => (any::<i32>(), any::<u8>(), uniform3(any::<i32>()), uniform3(any::<i8>()), uniform3(any::<i16>()), metadata())
        .prop_map(|(entity_id, mob_type, [x, y, z], [yaw, pitch, head_yaw], [velocity_x, velocity_y, velocity_z], metadata)| clientbound::SpawnMob {
            entity_id, mob_type, x, y, z, yaw, pitch, head_yaw, velocity_x, velocity_y, velocity_z, metadata,
        });
    EntityVelocity, DEFAULT => (any::<i32>(), uniform3(any::<i16>()))
        .prop_map(|(entity_id, [velocity_x, velocity_y, velocity_z])| clientbound::EntityVelocity {
            entity_id, velocity_x, velocity_y, velocity_z,
        });
    DestroyEntities, DEFAULT => vec(any::<i32>(), 0..16).prop_map(|entity_ids| clientbound::DestroyEntities { entity_ids });
    Entity, DEFAULT => any::<i32>().prop_map(|entity_id| clientbound::Entity { entity_id });
    EntityRelativeMove, DEFAULT => (any::<i32>(), uniform3(any::<i8>()), any::<bool>())
        .prop_map(|(entity_id, [dx, dy, dz], on_ground)| clientbound::EntityRelativeMove { entity_id, dx, dy, dz, on_ground });
    EntityLook, DEFAULT => (any::<i32>(), any::<i8>(), any::<i8>(), any::<bool>())
        .prop_map(|(entity_id, yaw, pitch, on_ground)| clientbound::EntityLook { entity_id, yaw, pitch, on_ground });
    EntityLookMove, DEFAULT => (any::<i32>(), uniform5(any::<i8>()), any::<bool>())
        .prop_map(|(entity_id, [dx, dy, dz, yaw, pitch], on_ground)| clientbound::EntityLookMove {
            entity_id, dx, dy, dz, yaw, pitch, on_ground,
        });
    EntityTeleport, DEFAULT => (any::<i32>(), uniform3(any::<i32>()), any::<i8>(), any::<i8>(), any::<bool>())
        .prop_map(|(entity_id, [x, y, z], yaw, pitch, on_ground)| clientbound::EntityTeleport {
            entity_id, x, y, z, yaw, pitch, on_ground,
        });
    EntityHeadLook, DEFAULT => (any::<i32>(), any::<i8>())
        .prop_map(|(entity_id, head_yaw)| clientbound::EntityHeadLook { entity_id, head_yaw });
    EntityStatus, DEFAULT => (any::<i32>(), any::<i8>())
        .prop_map(|(entity_id, status)| clientbound::EntityStatus { entity_id, status });
    AttachEntity, DEFAULT => (any::<u8>(), any::<i32>(), any::<i32>())
        .prop_map(|(leash, entity_id, vehicle_id)| clientbound::AttachEntity { leash, entity_id, vehicle_id });
    EntityMetadata, DEFAULT => (any::<i32>(), metadata())
        .prop_map(|(entity_id, metadata)| clientbound::EntityMetadata { entity_id, metadata });
    SetExperience, DEFAULT => (f32s(), any::<i32>(), any::<i32>())
        .prop_map(|(experience, level, total)| clientbound::SetExperience { experience, level, total });
    // The chunk payload is an opaque byte array on the wire; its section
    // layout is validated by the world crate, not the packet decoder.
    ChunkData, HEAVY => (any::<i32>(), any::<i32>(), any::<bool>(), any::<u16>(), bytes(512))
        .prop_map(|(chunk_x, chunk_z, ground_up, primary_bit_mask, data)| clientbound::ChunkData {
            chunk_x, chunk_z, ground_up, primary_bit_mask, data,
        });
    MultiBlockChange, DEFAULT => (any::<i32>(), any::<i32>(), vec((any::<u16>(), any::<u16>()), 0..16))
        .prop_map(|(chunk_x, chunk_z, records)| clientbound::MultiBlockChange {
            chunk_x, chunk_z,
            records: records.into_iter().map(|(packed_offset, block_state)| clientbound::BlockUpdate { packed_offset, block_state }).collect(),
        });
    BlockChange, DEFAULT => (any::<i64>(), any::<u16>())
        .prop_map(|(packed_position, block_state)| clientbound::BlockChange { packed_position, block_state });
    BlockAction, DEFAULT => (block_pos(), any::<u8>(), any::<u8>(), any::<i32>())
        .prop_map(|((x, y, z), instrument, pitch, block_id)| clientbound::BlockAction { x, y, z, instrument, pitch, block_id });
    BlockBreakAnim, DEFAULT => (any::<i32>(), block_pos(), any::<u8>())
        .prop_map(|(entity_id, (x, y, z), stage)| clientbound::BlockBreakAnim { entity_id, x, y, z, stage });
    MapChunkBulk, HEAVY => map_chunk_bulk();
    ChangeGameState, DEFAULT => (any::<u8>(), f32s()).prop_map(|(reason, value)| clientbound::ChangeGameState { reason, value });
    OpenWindow, DEFAULT => open_window();
    CloseWindow, DEFAULT => any::<u8>().prop_map(|window_id| clientbound::CloseWindow { window_id });
    SetSlot, DEFAULT => (any::<i8>(), any::<i16>(), item_stack())
        .prop_map(|(window_id, slot, item)| clientbound::SetSlot { window_id, slot, item });
    WindowItems, DEFAULT => (any::<u8>(), vec(item_stack(), 0..8))
        .prop_map(|(window_id, items)| clientbound::WindowItems { window_id, items });
    WindowProperty, DEFAULT => (any::<u8>(), any::<i16>(), any::<i16>())
        .prop_map(|(window_id, property, value)| clientbound::WindowProperty { window_id, property, value });
    ConfirmTransaction, DEFAULT => (any::<u8>(), any::<i16>(), any::<bool>())
        .prop_map(|(window_id, action_number, accepted)| clientbound::ConfirmTransaction { window_id, action_number, accepted });
    UseBed, DEFAULT => (any::<i32>(), block_pos())
        .prop_map(|(entity_id, (x, y, z))| clientbound::UseBed { entity_id, x, y, z });
    CollectItem, DEFAULT => (any::<i32>(), any::<i32>())
        .prop_map(|(collected_entity_id, collector_entity_id)| clientbound::CollectItem { collected_entity_id, collector_entity_id });
    SpawnPainting, DEFAULT => (any::<i32>(), text(13), block_pos(), 0u8..4)
        .prop_map(|(entity_id, title, (x, y, z), facing)| clientbound::SpawnPainting { entity_id, title, x, y, z, facing });
    SpawnXpOrb, DEFAULT => (any::<i32>(), uniform3(any::<i32>()), any::<i16>())
        .prop_map(|(entity_id, [x, y, z], count)| clientbound::SpawnXpOrb { entity_id, x, y, z, count });
    EntityEffect, DEFAULT => (any::<i32>(), any::<i8>(), any::<i8>(), any::<i32>(), any::<i8>())
        .prop_map(|(entity_id, effect_id, amplifier, duration, hide_particles)| clientbound::EntityEffect {
            entity_id, effect_id, amplifier, duration, hide_particles,
        });
    RemoveEntityEffect, DEFAULT => (any::<i32>(), any::<u8>())
        .prop_map(|(entity_id, effect_id)| clientbound::RemoveEntityEffect { entity_id, effect_id });
    EntityProperties, DEFAULT => entity_properties();
    Explosion, DEFAULT => (uniform4(f32s()), vec(uniform3(any::<i8>()), 0..8), uniform3(f32s()))
        .prop_map(|([x, y, z, strength], records, [player_motion_x, player_motion_y, player_motion_z])| clientbound::Explosion {
            x, y, z, strength, records, player_motion_x, player_motion_y, player_motion_z,
        });
    Effect, DEFAULT => (any::<i32>(), block_pos(), any::<i32>(), any::<bool>())
        .prop_map(|(effect_id, (x, y, z), data, disable_relative_volume)| clientbound::Effect {
            effect_id, x, y, z, data, disable_relative_volume,
        });
    SoundEffect, DEFAULT => (text(256), uniform3(any::<i32>()), f32s(), any::<u8>())
        .prop_map(|(name, [x, y, z], volume, pitch)| clientbound::SoundEffect { name, x, y, z, volume, pitch });
    Particle, DEFAULT => particle();
    SpawnGlobalEntity, DEFAULT => (any::<i32>(), any::<i8>(), uniform3(any::<i32>()))
        .prop_map(|(entity_id, entity_type, [x, y, z])| clientbound::SpawnGlobalEntity { entity_id, entity_type, x, y, z });
    UpdateSign, DEFAULT => (block_pos(), [text(40), text(40), text(40), text(40)])
        .prop_map(|((x, y, z), lines)| clientbound::UpdateSign { x, y, z, lines });
    Maps, DEFAULT => maps();
    UpdateBlockEntity, DEFAULT => (block_pos(), any::<u8>(), optional_nbt())
        .prop_map(|((x, y, z), action, nbt)| clientbound::UpdateBlockEntity { x, y, z, action, nbt });
    SignEditorOpen, DEFAULT => block_pos().prop_map(|(x, y, z)| clientbound::SignEditorOpen { x, y, z });
    Statistics, DEFAULT => vec((text(40), any::<i32>()), 0..6).prop_map(|entries| clientbound::Statistics { entries });
    PlayerListItem, DEFAULT => player_list_item();
    PlayerAbilities, DEFAULT => (any::<u8>(), f32s(), f32s())
        .prop_map(|(flags, fly_speed, walk_speed)| clientbound::PlayerAbilities { flags, fly_speed, walk_speed });
    TabComplete, DEFAULT => vec(text(40), 0..5).prop_map(|matches| clientbound::TabComplete { matches });
    ScoreboardObjective, DEFAULT => scoreboard_objective();
    UpdateScore, DEFAULT => update_score();
    DisplayScoreboard, DEFAULT => (any::<i8>(), text(16))
        .prop_map(|(position, score_name)| clientbound::DisplayScoreboard { position, score_name });
    Teams, DEFAULT => teams();
    PluginMessage, DEFAULT => (text(20), bytes(64))
        .prop_map(|(channel, data)| clientbound::PluginMessage { channel, data });
    ServerDifficulty, DEFAULT => any::<u8>().prop_map(|difficulty| clientbound::ServerDifficulty { difficulty });
    CombatEvent, DEFAULT => combat_event();
    Camera, DEFAULT => any::<i32>().prop_map(|entity_id| clientbound::Camera { entity_id });
    WorldBorder, DEFAULT => world_border();
    Title, DEFAULT => title();
    SetCompression, DEFAULT => any::<i32>().prop_map(|threshold| clientbound::SetCompression { threshold });
    PlayerListHeaderFooter, DEFAULT => (text(40), text(40))
        .prop_map(|(header, footer)| clientbound::PlayerListHeaderFooter { header, footer });
    ResourcePackSend, DEFAULT => (text(40), text(40))
        .prop_map(|(url, hash)| clientbound::ResourcePackSend { url, hash });
    UpdateEntityNbt, DEFAULT => (any::<i32>(), optional_nbt())
        .prop_map(|(entity_id, nbt)| clientbound::UpdateEntityNbt { entity_id, nbt });
    Disconnect, DEFAULT => text(256).prop_map(|reason| clientbound::Disconnect { reason });
});
