// SPDX-License-Identifier: MIT OR Apache-2.0

//! The achievements the screen draws, and the state they derive from the
//! server's statistics (MCP `StatFileWriter`).
//!
//! The caller resolves names and descriptions with the player's language and
//! turns each `achievement.<id>` statistic into the `unlocked` flag; the
//! screen derives everything else from the parent chain.

use crate::draw::SpriteSheet;
use crate::widgets::SlotItem;

/// One achievement cell.
#[derive(Debug, Clone, PartialEq)]
pub struct AchievementNode {
    /// The column, in 24 unit cells from the first achievement.
    pub column: i32,
    /// The row.
    pub row: i32,
    /// Whether the frame is the rounded special one.
    pub special: bool,
    /// The icon.
    pub icon: SlotItem,
    /// The translated name.
    pub name: String,
    /// The translated description.
    pub description: String,
    /// The index of the parent in the list, which must come first.
    pub parent: Option<usize>,
    /// Whether the statistic is above zero (`hasAchievementUnlocked`).
    pub unlocked: bool,
}

/// The achievements of the screen, in `AchievementList` order.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct AchievementsData {
    /// The cells; a parent precedes its children.
    pub nodes: Vec<AchievementNode>,
}

/// Where an achievement stands (`StatFileWriter.func_150874_c`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Progress {
    /// Unlocked.
    Unlocked,
    /// Not unlocked, but its parent is (`canUnlockAchievement`).
    Available,
    /// Locked behind this many locked ancestors, at least 1.
    Locked(u32),
}

impl AchievementsData {
    /// How many achievements are unlocked.
    #[must_use]
    pub fn unlocked_count(&self) -> usize {
        self.nodes.iter().filter(|node| node.unlocked).count()
    }

    /// The progress of node `index`.
    #[must_use]
    pub fn progress(&self, index: usize) -> Progress {
        let node = &self.nodes[index];
        if node.unlocked {
            return Progress::Unlocked;
        }
        let mut locked = 0;
        let mut parent = node.parent;
        while let Some(index) = parent {
            let ancestor = &self.nodes[index];
            if ancestor.unlocked {
                break;
            }
            locked += 1;
            parent = ancestor.parent;
        }
        if locked == 0 {
            Progress::Available
        } else {
            Progress::Locked(locked)
        }
    }

    /// The smallest and largest column and row of the cells
    /// (`AchievementList.minDisplayColumn` and so on).
    #[must_use]
    pub fn extent(&self) -> ([i32; 2], [i32; 2]) {
        let columns = self.nodes.iter().map(|node| node.column);
        let rows = self.nodes.iter().map(|node| node.row);
        let bounds = |values: &mut dyn Iterator<Item = i32>| {
            values.fold([i32::MAX, i32::MIN], |[lo, hi], v| [lo.min(v), hi.max(v)])
        };
        (bounds(&mut columns.clone()), bounds(&mut rows.clone()))
    }
}

/// The textures of the achievement screen, from the user's client jar
/// (never stored in the repository, spec §3).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AchievementArt {
    /// `textures/gui/achievement/achievement_background.png` (256x256): the
    /// frame, the node frames and the arrows.
    pub background: SpriteSheet,
    /// The 16x16 block textures the map is tiled with, in the order of
    /// [`MapTile`].
    pub tiles: [SpriteSheet; 8],
}

/// The kind of block a map tile shows (`GuiAchievements.drawAchievementScreen`);
/// the discriminant indexes [`AchievementArt::tiles`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MapTile {
    /// `Blocks.sand`.
    Sand,
    /// `Blocks.diamond_ore`.
    DiamondOre,
    /// `Blocks.redstone_ore`.
    RedstoneOre,
    /// `Blocks.iron_ore`.
    IronOre,
    /// `Blocks.coal_ore`.
    CoalOre,
    /// `Blocks.stone`.
    Stone,
    /// `Blocks.dirt`.
    Dirt,
    /// `Blocks.bedrock`.
    Bedrock,
}

impl MapTile {
    /// The block textures in the order of [`AchievementArt::tiles`].
    pub const TEXTURES: [&'static str; 8] = [
        "sand",
        "diamond_ore",
        "redstone_ore",
        "iron_ore",
        "coal_ore",
        "stone",
        "dirt",
        "bedrock",
    ];
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(parent: Option<usize>, unlocked: bool) -> AchievementNode {
        AchievementNode {
            column: 0,
            row: 0,
            special: false,
            icon: SlotItem {
                key: 0,
                count: 1,
                damage: 0,
                max_durability: 0,
                glint: false,
            },
            name: String::new(),
            description: String::new(),
            parent,
            unlocked,
        }
    }

    #[test]
    fn progress_counts_the_locked_ancestors() {
        let data = AchievementsData {
            nodes: vec![
                node(None, true),
                node(Some(0), false),
                node(Some(1), false),
                node(Some(2), false),
            ],
        };
        assert_eq!(data.progress(0), Progress::Unlocked);
        assert_eq!(data.progress(1), Progress::Available);
        assert_eq!(data.progress(2), Progress::Locked(1));
        assert_eq!(data.progress(3), Progress::Locked(2));
        assert_eq!(data.unlocked_count(), 1);
    }

    #[test]
    fn a_root_that_is_locked_is_still_available() {
        let data = AchievementsData {
            nodes: vec![node(None, false)],
        };
        assert_eq!(data.progress(0), Progress::Available);
    }
}
