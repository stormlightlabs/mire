//! WCAG 2 contrast ratios for palette tests.

use ratatui::style::Color;

/// Returns the WCAG 2 contrast ratio between two RGB colors, from 1 to 21.
pub fn contrast_ratio(foreground: Color, background: Color) -> f64 {
    let (foreground, background) = (luminance(foreground), luminance(background));
    (foreground.max(background) + 0.05) / (foreground.min(background) + 0.05)
}

fn luminance(color: Color) -> f64 {
    let Color::Rgb(red, green, blue) = color else {
        panic!("contrast needs an RGB color, not {color:?}");
    };
    let channel = |value: u8| {
        let value = f64::from(value) / 255.0;
        if value <= 0.04045 { value / 12.92 } else { ((value + 0.055) / 1.055).powf(2.4) }
    };
    0.2126 * channel(red) + 0.7152 * channel(green) + 0.0722 * channel(blue)
}
