// SPDX-License-Identifier: MIT OR Apache-2.0

//! The geometry of the tab list: columns, rows, widths, the ping bars and the
//! hearts of the list objective.
//!
//! Everything is `GuiPlayerTabOverlay.renderPlayerlist` in whole GUI units
//! (integer maths, as the reference), kept apart from the drawing so the
//! numbers are tested without a font.

/// A tab list shows at most this many players.
pub(super) const MAX_VISIBLE: usize = 80;
/// At most this many players share a column before another column starts.
const MAX_ROWS: i32 = 20;
/// The height of a row, 8 units of band and one of gap.
pub(super) const ROW_HEIGHT: i32 = 9;
/// The gap between two columns.
pub(super) const COLUMN_GAP: i32 = 5;
/// The width of the head column.
pub(super) const HEAD_WIDTH: i32 = 9;
/// The width of the hearts column of a `hearts` objective.
const HEARTS_COLUMN: i32 = 90;
/// The ping bars and the padding every column keeps.
const COLUMN_PADDING: i32 = 13;
/// The screen margin the list keeps (`width - 50`).
const SCREEN_MARGIN: i32 = 50;

/// How wide the score column is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ValueColumn {
    /// No list objective.
    None,
    /// A `hearts` objective: a fixed 90 units.
    Hearts,
    /// An integer objective: the widest `" " + score`.
    Integer(i32),
}

impl ValueColumn {
    fn width(self) -> i32 {
        match self {
            Self::None => 0,
            Self::Hearts => HEARTS_COLUMN,
            Self::Integer(widest) => widest,
        }
    }
}

/// The columns of a tab list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct TabLayout {
    /// The players per column (`i4`).
    pub rows: i32,
    /// The number of columns (`j4`).
    pub columns: i32,
    /// The width of one column (`i1`).
    pub column_width: i32,
    /// The left edge of the first column (`j1`).
    pub left: i32,
    /// The width of all columns with their gaps (`l1` before the header).
    pub total_width: i32,
    /// The width of the value column (`l`).
    pub value_width: i32,
}

impl TabLayout {
    /// The layout of `count` players on a screen `screen_width` wide.
    ///
    /// `name_width` is the widest formatted name over *all* players and
    /// `heads` whether the 9-unit head column is drawn.
    pub(super) fn new(
        count: i32,
        name_width: i32,
        value: ValueColumn,
        heads: bool,
        screen_width: i32,
    ) -> Self {
        let mut rows = count;
        let mut columns = 1;
        while rows > MAX_ROWS {
            columns += 1;
            rows = (count + columns - 1) / columns;
        }
        let value_width = value.width();
        let head = if heads { HEAD_WIDTH } else { 0 };
        let wanted = columns * (head + name_width + value_width + COLUMN_PADDING);
        let column_width = wanted.min(screen_width - SCREEN_MARGIN) / columns;
        let total_width = column_width * columns + (columns - 1) * COLUMN_GAP;
        Self {
            rows,
            columns,
            column_width,
            left: screen_width / 2 - total_width / 2,
            total_width,
            value_width,
        }
    }

    /// The left edge of a column.
    pub(super) fn column_x(&self, column: i32) -> i32 {
        self.left + column * (self.column_width + COLUMN_GAP)
    }
}

/// The row of the ping bar sprite for a latency (`drawPing`): `icons.png` at
/// `(0, 176 + row * 8)`, 10 by 8.
pub(super) fn ping_row(ping: i32) -> u32 {
    match ping {
        ..=-1 => 5,
        0..=149 => 0,
        150..=299 => 1,
        300..=599 => 2,
        600..=999 => 3,
        _ => 4,
    }
}

/// What a `hearts` value draws.
#[derive(Debug, Clone, PartialEq)]
pub(super) enum HeartsPlan {
    /// Icons of `icons.png` as `(x offset, texture x)`, in draw order, 9 by 9
    /// from `y = 0`: container first, then the filled half or full heart.
    Icons(Vec<(f32, u32)>),
    /// The value in half hearts as text, coloured from red to green.
    Text {
        /// The value, `"7.5"`.
        half_hearts: String,
        /// The colour of the number.
        rgb: [u8; 3],
    },
}

/// The hearts for `value` in a column from `start` to `end`
/// (`drawScoreboardValues`, without the damage flash).
///
/// `None` for a value of zero or less, which draws nothing.
pub(super) fn hearts_plan(value: i32, start: i32, end: i32) -> Option<HeartsPlan> {
    let full = (value + 1).div_euclid(2).max(0);
    if full <= 0 {
        return None;
    }
    let slots = (value / 2).max(10);
    let step = ((end - start - 4) as f32 / slots as f32).min(9.0);
    if step <= 3.0 {
        let fraction = (value as f32 / 20.0).clamp(0.0, 1.0);
        return Some(HeartsPlan::Text {
            half_hearts: format!("{:?}", value as f32 / 2.0),
            rgb: [
                ((1.0 - fraction) * 255.0) as u8,
                (fraction * 255.0) as u8,
                0,
            ],
        });
    }
    let mut icons: Vec<(f32, u32)> = (full..slots).map(|i| (i as f32 * step, 16)).collect();
    for index in 0..full {
        let x = index as f32 * step;
        icons.push((x, 16));
        if index * 2 + 1 < value {
            icons.push((x, if index >= 10 { 160 } else { 52 }));
        }
        if index * 2 + 1 == value {
            icons.push((x, if index >= 10 { 169 } else { 61 }));
        }
    }
    Some(HeartsPlan::Icons(icons))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn twenty_players_fit_one_column() {
        let layout = TabLayout::new(20, 50, ValueColumn::None, false, 320);
        assert_eq!((layout.rows, layout.columns), (20, 1));
    }

    #[test]
    fn twenty_one_players_split_into_two_columns_of_eleven() {
        let layout = TabLayout::new(21, 50, ValueColumn::None, false, 320);
        assert_eq!((layout.rows, layout.columns), (11, 2));
    }

    #[test]
    fn eighty_players_make_four_columns_of_twenty() {
        let layout = TabLayout::new(80, 50, ValueColumn::None, false, 640);
        assert_eq!((layout.rows, layout.columns), (20, 4));
    }

    #[test]
    fn a_column_is_name_plus_value_plus_thirteen_and_the_heads() {
        let plain = TabLayout::new(3, 50, ValueColumn::None, false, 320);
        assert_eq!(plain.column_width, 63);
        let with_heads = TabLayout::new(3, 50, ValueColumn::Integer(12), true, 320);
        assert_eq!(with_heads.column_width, 9 + 50 + 12 + 13);
    }

    #[test]
    fn a_hearts_objective_reserves_ninety_units() {
        let layout = TabLayout::new(3, 50, ValueColumn::Hearts, false, 640);
        assert_eq!(layout.value_width, 90);
        assert_eq!(layout.column_width, 50 + 90 + 13);
    }

    #[test]
    fn the_columns_never_exceed_the_screen_minus_fifty() {
        let layout = TabLayout::new(40, 300, ValueColumn::None, true, 320);
        assert!(layout.total_width <= 320 - 50 + (layout.columns - 1) * COLUMN_GAP);
        assert_eq!(layout.column_width, (320 - 50) / 2);
    }

    #[test]
    fn the_block_is_centred() {
        let layout = TabLayout::new(25, 40, ValueColumn::None, false, 320);
        assert_eq!(layout.left, 160 - layout.total_width / 2);
        assert_eq!(
            layout.column_x(1),
            layout.left + layout.column_width + COLUMN_GAP
        );
    }

    #[test]
    fn the_ping_bars_follow_the_thresholds() {
        let rows: Vec<u32> = [-1, 0, 149, 150, 299, 300, 599, 600, 999, 1000]
            .iter()
            .map(|ping| ping_row(*ping))
            .collect();
        assert_eq!(rows, [5, 0, 0, 1, 1, 2, 2, 3, 3, 4]);
    }

    #[test]
    fn zero_or_negative_hearts_draw_nothing() {
        assert_eq!(hearts_plan(0, 0, 90), None);
        assert_eq!(hearts_plan(-4, 0, 90), None);
    }

    #[test]
    fn seven_hearts_points_draw_ten_containers_three_and_a_half_hearts() {
        let Some(HeartsPlan::Icons(icons)) = hearts_plan(7, 0, 90) else {
            panic!("icons expected");
        };
        // Slots: ten. Empty containers for 4..10, then 4 containers with
        // three full hearts and one half heart.
        let containers = icons.iter().filter(|(_, u)| *u == 16).count();
        assert_eq!(containers, 10);
        assert_eq!(icons.iter().filter(|(_, u)| *u == 52).count(), 3);
        assert_eq!(icons.iter().filter(|(_, u)| *u == 61).count(), 1);
        let step: f32 = (90.0 - 4.0) / 10.0;
        assert!((icons[0].0 - 4.0 * step.min(9.0)).abs() < 1e-4);
    }

    #[test]
    fn hearts_past_ten_use_the_golden_sprites() {
        let Some(HeartsPlan::Icons(icons)) = hearts_plan(24, 0, 200) else {
            panic!("icons expected");
        };
        assert!(icons.iter().any(|(_, u)| *u == 160));
        assert!(icons.iter().any(|(_, u)| *u == 52));
    }

    #[test]
    fn a_narrow_column_shows_the_value_as_coloured_text() {
        let Some(HeartsPlan::Text { half_hearts, rgb }) = hearts_plan(15, 0, 30) else {
            panic!("text expected");
        };
        assert_eq!(half_hearts, "7.5");
        assert_eq!(rgb, [63, 191, 0]);
        let Some(HeartsPlan::Text { half_hearts, .. }) = hearts_plan(20, 0, 30) else {
            panic!("text expected");
        };
        assert_eq!(half_hearts, "10.0");
    }
}
