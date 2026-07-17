use ratatui::layout::{Constraint, Direction, Layout, Rect};

const FOOTER_HEIGHT: u16 = 1;
const HEADER_HEIGHT: u16 = 1;
const MIN_SIDEBAR_WIDTH: u16 = 12;
const MAX_SIDEBAR_WIDTH: u16 = 30;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UiAreas {
    pub title: Rect,
    pub sidebar: Rect,
    pub review: Rect,
    pub footer: Rect,
}

impl UiAreas {
    pub fn new(area: Rect) -> Self {
        let vertical = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(HEADER_HEIGHT),
                Constraint::Min(1),
                Constraint::Length(FOOTER_HEIGHT),
            ])
            .split(area);
        let horizontal = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Length(sidebar_width(area.width)), Constraint::Min(1)])
            .split(vertical[1]);
        Self { title: vertical[0], sidebar: horizontal[0], review: horizontal[1], footer: vertical[2] }
    }
}

fn sidebar_width(total_width: u16) -> u16 {
    if total_width < MIN_SIDEBAR_WIDTH.saturating_add(12) {
        return total_width / 3;
    }
    (total_width / 4).clamp(MIN_SIDEBAR_WIDTH, MAX_SIDEBAR_WIDTH)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_keeps_sidebar_and_review_visible_at_supported_widths() {
        for width in [24, 64, 120] {
            let areas = UiAreas::new(Rect::new(0, 0, width, 12));
            assert!(areas.sidebar.width > 0);
            assert!(areas.review.width > 0);
            assert_eq!(areas.sidebar.width + areas.review.width, width);
        }
    }
}
