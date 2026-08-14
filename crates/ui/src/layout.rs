use ratatui::layout::{Constraint, Direction, Layout, Rect};

const DIVIDER_WIDTH: u16 = 1;
const FOOTER_HEIGHT: u16 = 1;
const HEADER_HEIGHT: u16 = 1;
const MIN_SIDEBAR_WIDTH: u16 = 16;
const MAX_SIDEBAR_WIDTH: u16 = 40;
const SIDEBAR_PERCENT: u16 = 22;

/// Rectangles for the persistent chrome and review content.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UiAreas {
    /// The top status bar.
    pub title: Rect,
    /// The complete content area between the status bar and footer.
    pub body: Rect,
    /// The file navigation pane.
    pub sidebar: Rect,
    /// The separator between file navigation and review content.
    pub sidebar_divider: Rect,
    /// The continuous review stream or help content.
    pub review: Rect,
    /// The bottom keyboard-hint bar.
    pub footer: Rect,
}

impl UiAreas {
    /// Calculates pane rectangles with an optional file sidebar.
    pub fn new(area: Rect, sidebar_visible: bool) -> Self {
        let vertical = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(HEADER_HEIGHT),
                Constraint::Min(1),
                Constraint::Length(FOOTER_HEIGHT),
            ])
            .split(area);
        let sidebar_width = if sidebar_visible { sidebar_width(area.width) } else { 0 };
        let divider_width = if sidebar_visible { DIVIDER_WIDTH } else { 0 };
        let horizontal = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Length(sidebar_width),
                Constraint::Length(divider_width),
                Constraint::Min(1),
            ])
            .split(vertical[1]);
        Self {
            title: vertical[0],
            body: vertical[1],
            sidebar: horizontal[0],
            sidebar_divider: horizontal[1],
            review: horizontal[2],
            footer: vertical[2],
        }
    }
}

fn sidebar_width(total_width: u16) -> u16 {
    if total_width < MIN_SIDEBAR_WIDTH.saturating_add(12).saturating_add(DIVIDER_WIDTH) {
        return (total_width / 3).saturating_sub(DIVIDER_WIDTH).max(1);
    }
    total_width
        .saturating_mul(SIDEBAR_PERCENT)
        .checked_div(100)
        .unwrap_or_default()
        .clamp(MIN_SIDEBAR_WIDTH, MAX_SIDEBAR_WIDTH)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_keeps_sidebar_divider_and_review_visible_at_supported_widths() {
        for (width, expected_sidebar) in [(24, 7), (64, 16), (120, 26), (200, 40)] {
            let areas = UiAreas::new(Rect::new(0, 0, width, 12), true);
            assert_eq!(areas.sidebar.width, expected_sidebar);
            assert_eq!(areas.sidebar_divider.width, DIVIDER_WIDTH);
            assert!(areas.review.width > 0);
            assert_eq!(
                areas.sidebar.width + areas.sidebar_divider.width + areas.review.width,
                width
            );
            assert_eq!(areas.body.width, width);
        }
    }
}
