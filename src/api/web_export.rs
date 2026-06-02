//! Lean, web-facing export of a parsed save.
//!
//! ADDITIVE to upstream ER-Save-Lib (keeps the fork rebaseable): this module reads the
//! crate-private save structs and emits a compact, serde-serializable `LeanSave` containing
//! only what the Elden Ring Compass website consumes. See
//! `docs/projects/wasm-save-parser-rewrite.md` for the full kept/dropped inventory and the
//! design rationale (one-shot parse, IDs-only with name resolution in JS, trailing-zero-trimmed
//! event flags, etc.).

use serde::Serialize;

use super::save_api::SaveApi;
use crate::save::save::Save;

#[derive(Serialize, Debug, Clone)]
pub struct LeanSave {
    /// Account steam id (from UserData10). Stringified so JS gets a stable string key, not BigInt.
    pub global_steam_id: String,
    /// Per active-slot steam id (parallel to `slots`).
    pub character_steam_ids: Vec<String>,
    /// Only the active character slots.
    pub slots: Vec<LeanSlot>,
}

#[derive(Serialize, Debug, Clone)]
pub struct LeanSlot {
    pub steam_id: String,
    pub map_id: [u8; 4],
    pub player_game_data: LeanPlayerGameData,
    pub player_coords: LeanPlayerCoords,
    pub regions: LeanRegions,
    pub event_flags: LeanEventFlags,
    /// Non-empty gaitem-map entries: maps an item instance handle -> its param item_id.
    pub ga_items: Vec<LeanGaItem>,
    /// Currently-equipped item instance handles (resolve via `ga_items`).
    pub chr_asm2: LeanChrAsm,
    pub equip_inventory_data: LeanInventory,
    pub storage_inventory_data: LeanInventory,
    pub equip_item_data: LeanEquipItemData,
    /// Active SpEffect buffs/statuses on the character (empty slots filtered out).
    pub sp_effects: Vec<LeanSpEffect>,
}

#[derive(Serialize, Debug, Clone)]
pub struct LeanPlayerGameData {
    pub character_name: String,
    pub vigor: u32,
    pub mind: u32,
    pub endurance: u32,
    pub strength: u32,
    pub dexterity: u32,
    pub intelligence: u32,
    pub faith: u32,
    pub arcane: u32,
    pub level: u32,
    /// Held runes (the site historically calls this `souls`).
    pub souls: u32,
    /// Lifetime runes / rune memory (historically `soulsmemory`).
    pub soulsmemory: u32,
    pub gender: u8,
    /// Starting class / archetype (historically `arche_type`).
    pub arche_type: u8,
    pub match_making_wpn_lvl: u8,
}

#[derive(Serialize, Debug, Clone)]
pub struct LeanPlayerCoords {
    pub player_coords: [f32; 3],
    pub map_id: [u8; 4],
}

#[derive(Serialize, Debug, Clone)]
pub struct LeanRegions {
    pub unlocked_regions_count: u32,
    pub unlocked_regions: Vec<u32>,
}

#[derive(Serialize, Debug, Clone)]
pub struct LeanEventFlags {
    /// Raw event-flag bitfield with trailing zero bytes trimmed (byte offsets preserved).
    /// JS reads arbitrary flags via `flags[byteOffset] & (1 << bit)`; bytes past the end read 0.
    #[serde(with = "serde_bytes")]
    pub flags: Vec<u8>,
}

#[derive(Serialize, Debug, Clone)]
pub struct LeanGaItem {
    pub gaitem_handle: u32,
    pub item_id: u32,
    /// For weapons: the gaitem handle of the attached Ash of War / gem (0 if none).
    /// Resolve the AoW by finding the `ga_items` entry whose `gaitem_handle` equals this,
    /// then looking up its `item_id` in the Ash-of-War name table.
    pub gem_gaitem_handle: u32,
}

#[derive(Serialize, Debug, Clone)]
pub struct LeanSpEffect {
    pub sp_effect_id: i32,
    pub remaining_time: f32,
}

#[derive(Serialize, Debug, Clone)]
pub struct LeanChrAsm {
    pub left_hand_armaments: [u32; 3],
    pub right_hand_armaments: [u32; 3],
    pub arrows: [u32; 2],
    pub bolts: [u32; 2],
    pub head: u32,
    pub chest: u32,
    pub arms: u32,
    pub legs: u32,
    pub talismans: [u32; 4],
}

#[derive(Serialize, Debug, Clone)]
pub struct LeanInventoryItem {
    pub ga_item_handle: u32,
    pub quantity: u32,
    pub inventory_index: u32,
}

#[derive(Serialize, Debug, Clone)]
pub struct LeanInventory {
    pub common_inventory_items_distinct_count: u32,
    pub common_items: Vec<LeanInventoryItem>,
    pub key_inventory_items_distinct_count: u32,
    pub key_items: Vec<LeanInventoryItem>,
}

#[derive(Serialize, Debug, Clone)]
pub struct LeanEquipItem {
    /// Item instance handle for a quick-slot / pouch entry (resolve via `ga_items`).
    /// Named `item_id` to match the field the website's equipment view-model reads.
    pub item_id: u32,
}

#[derive(Serialize, Debug, Clone)]
pub struct LeanEquipItemData {
    pub quick_slot_items: Vec<LeanEquipItem>,
    pub pouch_items: Vec<LeanEquipItem>,
}

impl SaveApi {
    /// Build the lean, web-facing export of this save (read-only, active slots only).
    pub fn lean_export(&self) -> LeanSave {
        build_lean_save(&self.raw)
    }
}

fn trim_trailing_zeros(bytes: &[u8]) -> Vec<u8> {
    let end = bytes.iter().rposition(|&b| b != 0).map_or(0, |i| i + 1);
    bytes[..end].to_vec()
}

pub(crate) fn build_lean_save(save: &Save) -> LeanSave {
    let active = save.user_data_10.profile_summary.active_profiles;
    let mut slots = Vec::new();
    let mut character_steam_ids = Vec::new();

    for (i, is_active) in active.iter().enumerate() {
        if !*is_active {
            continue;
        }
        let ud = &save.user_data_x[i];
        character_steam_ids.push(ud.steam_id.to_string());
        slots.push(build_slot(ud));
    }

    LeanSave {
        global_steam_id: save.user_data_10.steam_id.to_string(),
        character_steam_ids,
        slots,
    }
}

fn build_slot(ud: &crate::save::user_data_x::UserDataX) -> LeanSlot {
    let pgd = &ud.player_game_data;
    let eg = &ud.equipped_items_gaitem_handle;
    let coords = &ud.player_coordinates;

    let map_inventory = |inv: &crate::save::user_data_x::Invenotry| -> LeanInventory {
        let common_n = inv.common_item_count as usize;
        let key_n = inv.key_item_count as usize;
        LeanInventory {
            common_inventory_items_distinct_count: inv.common_item_count,
            common_items: inv
                .common_items
                .iter()
                .take(common_n)
                .map(|it| LeanInventoryItem {
                    ga_item_handle: it.gaitem_handle,
                    quantity: it.quantity,
                    inventory_index: it.aqcuistion_index,
                })
                .collect(),
            key_inventory_items_distinct_count: inv.key_item_count,
            key_items: inv
                .key_items
                .iter()
                .take(key_n)
                .map(|it| LeanInventoryItem {
                    ga_item_handle: it.gaitem_handle,
                    quantity: it.quantity,
                    inventory_index: it.aqcuistion_index,
                })
                .collect(),
        }
    };

    LeanSlot {
        steam_id: ud.steam_id.to_string(),
        map_id: ud.map_id,
        player_game_data: LeanPlayerGameData {
            character_name: pgd.character_name.clone(),
            vigor: pgd.vigor,
            mind: pgd.mind,
            endurance: pgd.endurance,
            strength: pgd.strength,
            dexterity: pgd.dexterity,
            intelligence: pgd.intelligence,
            faith: pgd.faith,
            arcane: pgd.arcane,
            level: pgd.level,
            souls: pgd.runes,
            soulsmemory: pgd.runes_memory,
            gender: pgd.gender,
            arche_type: pgd.archetype,
            match_making_wpn_lvl: pgd.matchmaking_weapon_level,
        },
        player_coords: LeanPlayerCoords {
            player_coords: [coords.coordinates.0, coords.coordinates.1, coords.coordinates.2],
            map_id: coords.map_id,
        },
        regions: LeanRegions {
            unlocked_regions_count: ud.unlocked_regions.count,
            unlocked_regions: ud.unlocked_regions.ids.clone(),
        },
        event_flags: LeanEventFlags {
            flags: trim_trailing_zeros(&ud.event_flags),
        },
        ga_items: ud
            .gaitem_map
            .iter()
            .filter(|g| g.gaitem_handle != 0)
            .map(|g| LeanGaItem {
                gaitem_handle: g.gaitem_handle,
                item_id: g.item_id,
                gem_gaitem_handle: g.gem_gaitem_handle.map(|h| h as u32).unwrap_or(0),
            })
            .collect(),
        chr_asm2: LeanChrAsm {
            left_hand_armaments: [
                eg.left_hand_armament1,
                eg.left_hand_armament2,
                eg.left_hand_armament3,
            ],
            right_hand_armaments: [
                eg.right_hand_armament1,
                eg.right_hand_armament2,
                eg.right_hand_armament3,
            ],
            arrows: [eg.arrows1, eg.arrows2],
            bolts: [eg.bolts1, eg.bolts2],
            head: eg.head,
            chest: eg.chest,
            arms: eg.arms,
            legs: eg.legs,
            talismans: [eg.talisman1, eg.talisman2, eg.talisman3, eg.talisman4],
        },
        equip_inventory_data: map_inventory(&ud.inventory_held),
        storage_inventory_data: map_inventory(&ud.inventory_storage_box),
        equip_item_data: LeanEquipItemData {
            quick_slot_items: ud
                .equipped_items
                .quick_items
                .iter()
                .map(|e| LeanEquipItem { item_id: e.gaitem_handle })
                .collect(),
            pouch_items: ud
                .equipped_items
                .pouch_items
                .iter()
                .map(|e| LeanEquipItem { item_id: e.gaitem_handle })
                .collect(),
        },
        sp_effects: ud
            .sp_effects
            .iter()
            .filter(|sp| sp.sp_effect_id != 0 && sp.sp_effect_id != -1)
            .map(|sp| LeanSpEffect {
                sp_effect_id: sp.sp_effect_id,
                remaining_time: sp.remaining_time,
            })
            .collect(),
    }
}
