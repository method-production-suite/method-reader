//! Where things go at a given window size: the geometry `style.css` and
//! `surfaces.css` describe, as numbers, so it is measured by a test here
//! the way tests/desktop-layout-measured.test.ts measured the webview at
//! 900, 1280 and 1800 wide. All values are logical points (CSS pixels).

/// The page's side margins (the same either side, as in the family's other
/// windows; there is no binding any more), and the content block's cap.
pub const PAGE_SIDE: f32 = 32.0;
pub const BLOCK_MAX: f32 = 1180.0;
/// Under this block width Read, Settings and the result fold to one column.
pub const FOLD: f32 = 880.0;
/// At or under this window width the margins narrow.
pub const NARROW: f32 = 720.0;
/// At or under this window height the script's frame takes a larger share.
pub const SHORT_WINDOW: f32 = 560.0;
/// At or under this window height the wordmark shrinks.
pub const SHORT_PAGE: f32 = 620.0;
pub const GAP: f32 = 32.0;
pub const CONVERT_COLUMN: f32 = 820.0;
pub const RESULT_PAGE: f32 = 400.0;
pub const RESULT_PANEL: f32 = 620.0;
pub const INDEX_SLOT: f32 = 218.0;
pub const SCRIPT_COLUMN: f32 = 706.0;
pub const SETTINGS_COLUMN: f32 = 420.0;
/// The spacing scale, at a 16px root.
pub const SPACE: [f32; 11] = [
    0.0, 6.4, 9.6, 12.8, 16.0, 19.2, 25.6, 32.0, 40.0, 48.0, 56.0,
];

/// A horizontal span.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Span {
    pub left: f32,
    pub width: f32,
}

impl Span {
    pub fn right(&self) -> f32 {
        self.left + self.width
    }
}

/// The page's two side margins.
pub fn margins(window_w: f32) -> (f32, f32) {
    if window_w <= NARROW {
        (SPACE[4], SPACE[4])
    } else {
        (PAGE_SIDE, PAGE_SIDE)
    }
}

/// The content block: the window less its margins, capped and centred.
pub fn block(window_w: f32) -> Span {
    let (left, right) = margins(window_w);
    let room = (window_w - left - right).max(0.0);
    let width = room.min(BLOCK_MAX);
    Span {
        left: left + (room - width) / 2.0,
        width,
    }
}

pub fn folded(block_w: f32) -> bool {
    block_w < FOLD
}

/// The result: the book's first page and the side, centred as a pair; or,
/// folded, the side first and the page under it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ResultLayout {
    pub page: Span,
    pub side: Span,
    pub stacked: bool,
}

pub fn result(block: Span) -> ResultLayout {
    if folded(block.width) {
        let side_w = block.width.min(RESULT_PANEL);
        let page_w = block.width.min(RESULT_PAGE);
        return ResultLayout {
            side: Span {
                left: block.left + (block.width - side_w) / 2.0,
                width: side_w,
            },
            page: Span {
                left: block.left + (block.width - page_w) / 2.0,
                width: page_w,
            },
            stacked: true,
        };
    }
    let side_w = (block.width - RESULT_PAGE - GAP).clamp(0.0, RESULT_PANEL);
    let total = RESULT_PAGE + GAP + side_w;
    let left = block.left + (block.width - total) / 2.0;
    ResultLayout {
        page: Span {
            left,
            width: RESULT_PAGE,
        },
        side: Span {
            left: left + RESULT_PAGE + GAP,
            width: side_w,
        },
        stacked: false,
    }
}

/// Read: the index slot and the script; folded, the index above.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ReadLayout {
    pub index: Span,
    pub script: Span,
    pub stacked: bool,
}

pub fn read(block: Span) -> ReadLayout {
    if folded(block.width) {
        return ReadLayout {
            index: block,
            script: block,
            stacked: true,
        };
    }
    let script_w = (block.width - INDEX_SLOT - GAP).clamp(0.0, SCRIPT_COLUMN);
    ReadLayout {
        index: Span {
            left: block.left,
            width: INDEX_SLOT,
        },
        script: Span {
            left: block.left + INDEX_SLOT + GAP,
            width: script_w,
        },
        stacked: false,
    }
}

/// Settings: the 420 column and the preview in the rest; folded, one column.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SettingsLayout {
    pub knobs: Span,
    pub preview: Span,
    pub stacked: bool,
}

pub fn settings(block: Span) -> SettingsLayout {
    if folded(block.width) {
        return SettingsLayout {
            knobs: block,
            preview: block,
            stacked: true,
        };
    }
    SettingsLayout {
        knobs: Span {
            left: block.left,
            width: SETTINGS_COLUMN,
        },
        preview: Span {
            left: block.left + SETTINGS_COLUMN + GAP,
            width: block.width - SETTINGS_COLUMN - GAP,
        },
        stacked: false,
    }
}

/// The script's frame height: 68% of the window (58% narrow, 62% short),
/// at least 300.
pub fn script_height(window_w: f32, window_h: f32) -> f32 {
    let share = if window_h <= SHORT_WINDOW {
        0.62
    } else if window_w <= NARROW {
        0.58
    } else {
        0.68
    };
    (window_h * share).max(300.0)
}

/// The fewest points the script's frame shrinks to so the page's foot
/// stays in the window (a few lines of the script).
pub const SCRIPT_FLOOR: f32 = 96.0;

/// The script's frame as drawn: [`script_height`], but no taller than
/// `room`, the points between the frame's top and where the foot must
/// start for the rev stamp to sit whole inside the window. Read fits the
/// window rather than scrolling the page, so the frame gives way first,
/// down to [`SCRIPT_FLOOR`].
pub fn script_frame(window_w: f32, window_h: f32, room: f32) -> f32 {
    script_height(window_w, window_h)
        .min(room)
        .max(SCRIPT_FLOOR)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() <= 1.0
    }

    #[test]
    fn at_1280_the_block_fills_the_space() {
        let b = block(1280.0);
        assert!(close(b.left, 50.0) && close(b.width, 1180.0));
        assert!(!folded(b.width));
    }

    #[test]
    fn at_1800_the_block_is_capped_and_centred() {
        let b = block(1800.0);
        assert!(close(b.left, 310.0) && close(b.width, 1180.0));
    }

    #[test]
    fn at_900_the_block_is_under_the_fold() {
        let b = block(900.0);
        assert!(close(b.left, 32.0) && close(b.width, 836.0));
        assert!(folded(b.width));
    }

    #[test]
    fn the_result_is_a_400_page_a_32_gap_and_the_panel_centred_as_a_pair() {
        for w in [1280.0, 1800.0] {
            let b = block(w);
            let r = result(b);
            assert!(!r.stacked);
            assert_eq!(r.page.width, 400.0);
            assert!(close(r.side.left - r.page.right(), 32.0));
            assert_eq!(r.side.width, 620.0);
            assert!(close(r.page.left - b.left, b.right() - r.side.right()));
        }
        let b = block(900.0);
        let r = result(b);
        assert!(r.stacked);
        assert_eq!(r.page.width, 400.0);
        assert!(close(r.side.left - b.left, b.right() - r.side.right()));
        assert!(
            r.side.right() <= b.right() + 1.0,
            "nothing runs off the side"
        );
    }

    #[test]
    fn read_is_a_218_slot_then_the_706_script() {
        let b = block(1280.0);
        let r = read(b);
        assert!(close(r.index.left, b.left) && r.index.width == 218.0);
        assert!(close(r.script.left, b.left + 250.0) && r.script.width == 706.0);
        assert!(read(block(900.0)).stacked);
    }

    #[test]
    fn settings_is_a_420_column_and_the_preview_fills_the_rest() {
        let b = block(1280.0);
        let s = settings(b);
        assert_eq!(s.knobs.width, 420.0);
        assert!(close(s.preview.right(), b.right()));
        assert!(s.preview.width >= 420.0);
        assert!(settings(block(900.0)).stacked);
    }

    #[test]
    fn narrow_windows_narrow_the_margins() {
        assert_eq!(margins(700.0), (16.0, 16.0));
        assert_eq!(margins(721.0), (32.0, 32.0));
        assert_eq!(script_height(1280.0, 800.0), 544.0);
        assert_eq!(script_height(600.0, 800.0), 464.0);
        assert_eq!(script_height(1280.0, 480.0), 300.0);
    }

    #[test]
    fn the_script_frame_gives_way_to_the_foot() {
        // Room to spare: the share of the window.
        assert_eq!(script_frame(1280.0, 800.0, 900.0), 544.0);
        // Less room: the room, below the share and below its 300 floor.
        assert_eq!(script_frame(1280.0, 800.0, 420.0), 420.0);
        assert_eq!(script_frame(640.0, 480.0, 200.0), 200.0);
        // Never under the frame's own floor.
        assert_eq!(script_frame(640.0, 480.0, 10.0), SCRIPT_FLOOR);
    }
}
