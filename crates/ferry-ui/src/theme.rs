//! Ferry's colors and widget styles (F14). The layout follows Postman; the
//! palette is Ferry's own: neutral greys with a teal accent.

use iced::widget::{button, container, text, text_input};
use iced::{Background, Border, Color, Shadow, Theme, color, theme::Palette};

pub const BACKGROUND: Color = color!(0x1b_1d_21);
pub const SURFACE: Color = color!(0x22_25_2a);
pub const RAISED: Color = color!(0x2b_2f_35);
pub const BORDER: Color = color!(0x37_3c_43);
pub const TEXT: Color = color!(0xe6_e8_eb);
pub const MUTED: Color = color!(0x9a_a1_ab);
pub const FAINT: Color = color!(0x6b_72_7c);
pub const ACCENT: Color = color!(0x2d_d4_bf);
pub const ACCENT_STRONG: Color = color!(0x0f_9e_8e);
/// Error text. Bright enough for WCAG AA contrast on the dark background.
pub const ERROR: Color = color!(0xf8_71_71);
pub const WARNING: Color = color!(0xfb_bf_24);
pub const SUCCESS: Color = color!(0x4a_de_80);
pub const INFO: Color = color!(0x60_a5_fa);

/// The app theme.
#[must_use]
pub fn theme() -> Theme {
    Theme::custom(
        "Ferry".to_owned(),
        Palette {
            background: BACKGROUND,
            text: TEXT,
            primary: ACCENT_STRONG,
            success: SUCCESS,
            warning: WARNING,
            danger: ERROR,
        },
    )
}

/// Status colors by class: 2xx green, 3xx blue, 4xx amber, 5xx red.
#[must_use]
pub fn status_color(status: u16) -> Color {
    match status {
        200..=299 => SUCCESS,
        300..=399 => INFO,
        400..=499 => WARNING,
        500..=599 => ERROR,
        _ => TEXT,
    }
}

/// Method colors, as Postman uses them to tell methods apart at a glance.
#[must_use]
pub fn method_color(method: &str) -> Color {
    match method {
        "GET" | "HEAD" => SUCCESS,
        "POST" => WARNING,
        "PUT" => INFO,
        "PATCH" => color!(0xc0_84_fc),
        "DELETE" => ERROR,
        "OPTIONS" => color!(0xf4_72_b6),
        _ => MUTED,
    }
}

pub fn muted(_: &Theme) -> text::Style {
    text::Style { color: Some(MUTED) }
}

pub fn faint(_: &Theme) -> text::Style {
    text::Style { color: Some(FAINT) }
}

pub fn error(_: &Theme) -> text::Style {
    text::Style { color: Some(ERROR) }
}

pub fn accent(_: &Theme) -> text::Style {
    text::Style {
        color: Some(ACCENT),
    }
}

fn panel(background: Color) -> container::Style {
    container::Style {
        background: Some(Background::Color(background)),
        text_color: Some(TEXT),
        ..container::Style::default()
    }
}

pub fn app_background(_: &Theme) -> container::Style {
    panel(BACKGROUND)
}

pub fn surface(_: &Theme) -> container::Style {
    panel(SURFACE)
}

pub fn raised(_: &Theme) -> container::Style {
    panel(RAISED)
}

/// The confirm dialog.
pub fn dialog(_: &Theme) -> container::Style {
    container::Style {
        border: Border {
            color: BORDER,
            width: 1.0,
            radius: 8.0.into(),
        },
        shadow: Shadow {
            color: Color::from_rgba8(0, 0, 0, 0.5),
            offset: iced::Vector::new(0.0, 8.0),
            blur_radius: 24.0,
        },
        ..panel(RAISED)
    }
}

/// Dims the window behind a dialog.
pub fn scrim(_: &Theme) -> container::Style {
    panel(Color::from_rgba8(0, 0, 0, 0.55))
}

/// A thin line between regions.
pub fn divider(_: &Theme) -> container::Style {
    panel(BORDER)
}

/// The divider while it is being dragged or hovered.
pub fn divider_active(_: &Theme) -> container::Style {
    panel(ACCENT_STRONG)
}

/// Workbench tab: the active one is raised (its accent line is drawn by
/// the view).
pub fn workbench_tab(active: bool) -> impl Fn(&Theme) -> container::Style {
    move |_| panel(if active { SURFACE } else { BACKGROUND })
}

/// The accent line on the active tab and under the selected sub-tab.
pub fn accent_line(_: &Theme) -> container::Style {
    panel(ACCENT)
}

/// Flat button that only shows a background on hover; `selected` keeps
/// the accent text.
pub fn flat(selected: bool) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |_, status| {
        let background = match status {
            button::Status::Hovered | button::Status::Pressed => Some(Background::Color(RAISED)),
            _ => None,
        };
        let text_color = match status {
            button::Status::Disabled => FAINT,
            _ if selected => TEXT,
            _ => MUTED,
        };
        button::Style {
            background,
            text_color,
            border: Border {
                radius: 4.0.into(),
                ..Border::default()
            },
            ..button::Style::default()
        }
    }
}

/// Underlined tab button for the request and response sub-tabs.
pub fn sub_tab(selected: bool) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |theme, status| {
        let mut style = flat(selected)(theme, status);
        style.background = None;
        style.border = Border {
            color: if selected { ACCENT } else { Color::TRANSPARENT },
            width: 0.0,
            radius: 0.0.into(),
        };
        style
    }
}

/// The primary action (Send).
pub fn primary(_: &Theme, status: button::Status) -> button::Style {
    let background = match status {
        button::Status::Hovered => color!(0x14_b8_a6),
        button::Status::Pressed => color!(0x0d_94_88),
        button::Status::Disabled => RAISED,
        button::Status::Active => ACCENT_STRONG,
    };
    button::Style {
        background: Some(Background::Color(background)),
        text_color: Color::WHITE,
        border: Border {
            radius: 4.0.into(),
            ..Border::default()
        },
        ..button::Style::default()
    }
}

/// Secondary action with a border (Cancel, Discard's partner).
pub fn secondary(_: &Theme, status: button::Status) -> button::Style {
    button::Style {
        background: Some(Background::Color(match status {
            button::Status::Hovered | button::Status::Pressed => BORDER,
            _ => RAISED,
        })),
        text_color: if status == button::Status::Disabled {
            FAINT
        } else {
            TEXT
        },
        border: Border {
            color: BORDER,
            width: 1.0,
            radius: 4.0.into(),
        },
        ..button::Style::default()
    }
}

/// Destructive action (Discard).
pub fn danger(_: &Theme, status: button::Status) -> button::Style {
    button::Style {
        background: Some(Background::Color(match status {
            button::Status::Hovered | button::Status::Pressed => color!(0xdc_26_26),
            _ => color!(0xb9_1c_1c),
        })),
        text_color: Color::WHITE,
        border: Border {
            radius: 4.0.into(),
            ..Border::default()
        },
        ..button::Style::default()
    }
}

/// Text fields: dark with a subtle border that turns accent on focus.
pub fn input(theme: &Theme, status: text_input::Status) -> text_input::Style {
    let mut style = text_input::default(theme, status);
    style.background = Background::Color(BACKGROUND);
    style.border = Border {
        color: match status {
            text_input::Status::Focused { .. } => ACCENT_STRONG,
            _ => BORDER,
        },
        width: 1.0,
        radius: 4.0.into(),
    };
    style.placeholder = FAINT;
    style.value = TEXT;
    style
}

/// Text fields inside table cells: borderless until focused.
pub fn cell(theme: &Theme, status: text_input::Status) -> text_input::Style {
    let mut style = input(theme, status);
    style.background = Background::Color(Color::TRANSPARENT);
    if !matches!(status, text_input::Status::Focused { .. }) {
        style.border.color = Color::TRANSPARENT;
    }
    style.border.radius = 0.0.into();
    style
}

/// A row of the collections tree: raised when its request is the active
/// tab, outlined in the accent while something is dragged over it.
pub fn tree_row(active: bool, hovered: bool) -> impl Fn(&Theme) -> container::Style {
    move |_| container::Style {
        background: active.then_some(Background::Color(RAISED)),
        border: Border {
            color: if hovered { ACCENT } else { Color::TRANSPARENT },
            width: 1.0,
            radius: 4.0.into(),
        },
        text_color: Some(TEXT),
        ..container::Style::default()
    }
}

/// A variable chip under the URL bar (F08-AC6).
pub fn chip(_: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(RAISED)),
        border: Border {
            color: BORDER,
            width: 1.0,
            radius: 10.0.into(),
        },
        text_color: Some(TEXT),
        ..container::Style::default()
    }
}

/// An unresolved variable chip.
pub fn chip_error(theme: &Theme) -> container::Style {
    let mut style = chip(theme);
    style.border.color = ERROR;
    style
}

/// One option of a list the user picks from, such as the Save dialog's
/// destinations: the chosen one is filled with the accent.
pub fn choice(selected: bool) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |theme, status| {
        let mut style = flat(selected)(theme, status);
        if selected {
            style.background = Some(Background::Color(Color {
                a: 0.35,
                ..ACCENT_STRONG
            }));
            style.text_color = TEXT;
            style.border.color = ACCENT_STRONG;
            style.border.width = 1.0;
        }
        style
    }
}

/// A search match in the response body (F06-AC8).
pub fn find_match(_: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(Color { a: 0.35, ..WARNING })),
        border: Border {
            radius: 2.0.into(),
            ..Border::default()
        },
        ..container::Style::default()
    }
}

/// The current search match: filled, so it stands out from the others.
pub fn find_current(_: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(WARNING)),
        border: Border {
            radius: 2.0.into(),
            ..Border::default()
        },
        ..container::Style::default()
    }
}
