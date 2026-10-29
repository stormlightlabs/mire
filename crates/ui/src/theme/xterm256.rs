//! The xterm 256-colour index tmux picks for an RGB colour, for palette tests.
//!
//! A terminal without truecolor shows each RGB surface as its nearest xterm
//! 256-colour index. This follows tmux's `colour_find_rgb` in `colour.c`: it
//! snaps each channel to the 6×6×6 cube levels 0, 95, 135, 175, 215 and 255,
//! takes the grey ramp step nearest the channel average, and keeps the grey
//! only when it is strictly closer, by squared RGB distance, than the cube
//! colour.

use ratatui::style::Color;

const CUBE_LEVELS: [i32; 6] = [0x00, 0x5f, 0x87, 0xaf, 0xd7, 0xff];

/// Returns the xterm 256-colour index tmux uses for an RGB color.
pub fn xterm256_index(color: Color) -> u8 {
    let Color::Rgb(red, green, blue) = color else {
        panic!("xterm 256 rounding needs an RGB color, not {color:?}");
    };
    let target = (i32::from(red), i32::from(green), i32::from(blue));
    let steps = (cube_step(target.0), cube_step(target.1), cube_step(target.2));
    let cube = (level(steps.0), level(steps.1), level(steps.2));
    let cube_index = 16 + 36 * steps.0 + 6 * steps.1 + steps.2;
    if cube == target {
        return to_index(cube_index);
    }

    let average = (target.0 + target.1 + target.2) / 3;
    let grey_step = if average > 238 { 23 } else { (average - 3) / 10 };
    let grey = 8 + 10 * grey_step;
    let distance = |(r, g, b): (i32, i32, i32)| (r - target.0).pow(2) + (g - target.1).pow(2) + (b - target.2).pow(2);
    if distance((grey, grey, grey)) < distance(cube) {
        to_index(232 + grey_step)
    } else {
        to_index(cube_index)
    }
}

/// Returns the RGB color an xterm 256-colour terminal shows for `index`.
///
/// Indices 0 to 15 follow xterm's default system colors.
pub fn xterm256_rgb(index: u8) -> Color {
    const SYSTEM: [(u8, u8, u8); 16] = [
        (0, 0, 0),
        (205, 0, 0),
        (0, 205, 0),
        (205, 205, 0),
        (0, 0, 238),
        (205, 0, 205),
        (0, 205, 205),
        (229, 229, 229),
        (127, 127, 127),
        (255, 0, 0),
        (0, 255, 0),
        (255, 255, 0),
        (92, 92, 255),
        (255, 0, 255),
        (0, 255, 255),
        (255, 255, 255),
    ];
    let channel = |value: i32| u8::try_from(value).expect("xterm channels fit in a byte");
    match i32::from(index) {
        0..16 => {
            let (red, green, blue) = SYSTEM[usize::from(index)];
            Color::Rgb(red, green, blue)
        }
        cube @ 16..232 => {
            let cube = cube - 16;
            Color::Rgb(
                channel(level(cube / 36)),
                channel(level(cube / 6 % 6)),
                channel(level(cube % 6)),
            )
        }
        grey => {
            let grey = channel(8 + 10 * (grey - 232));
            Color::Rgb(grey, grey, grey)
        }
    }
}

/// Returns the RGB color an xterm 256-colour terminal shows for `color`.
pub fn xterm256_rounded(color: Color) -> Color {
    xterm256_rgb(xterm256_index(color))
}

/// Snaps one channel to its cube step, 0 to 5, with tmux's thresholds.
const fn cube_step(value: i32) -> i32 {
    if value < 48 {
        0
    } else if value < 114 {
        1
    } else {
        (value - 35) / 40
    }
}

fn level(step: i32) -> i32 {
    CUBE_LEVELS[usize::try_from(step).expect("cube steps are 0 to 5")]
}

fn to_index(value: i32) -> u8 {
    u8::try_from(value).expect("xterm indices fit in a byte")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn indices_map_back_to_their_colours() {
        for index in 16..=255 {
            assert_eq!(xterm256_index(xterm256_rgb(index)), index);
        }
        assert_eq!(xterm256_rgb(194), Color::Rgb(215, 255, 215));
        assert_eq!(xterm256_rgb(232), Color::Rgb(8, 8, 8));
    }

    #[test]
    fn exact_cube_colours_and_greys_map_to_their_indices() {
        assert_eq!(xterm256_index(Color::Rgb(0, 0, 0)), 16);
        assert_eq!(xterm256_index(Color::Rgb(255, 255, 255)), 231);
        assert_eq!(xterm256_index(Color::Rgb(215, 255, 215)), 194);
        assert_eq!(xterm256_index(Color::Rgb(8, 8, 8)), 232);
        assert_eq!(xterm256_index(Color::Rgb(238, 238, 238)), 255);
    }

    #[test]
    fn near_colours_round_as_tmux_does() {
        // Nord Light's former row backgrounds, which both fell on grey 254.
        assert_eq!(xterm256_index(Color::Rgb(223, 230, 225)), 254);
        assert_eq!(xterm256_index(Color::Rgb(230, 219, 225)), 254);
        assert_eq!(xterm256_index(Color::Rgb(212, 245, 230)), 194);
        assert_eq!(xterm256_index(Color::Rgb(242, 220, 222)), 224);
    }
}
