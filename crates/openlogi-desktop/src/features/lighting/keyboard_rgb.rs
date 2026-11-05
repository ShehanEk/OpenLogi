//! RGB keyboard lighting controls.
//!
//! A palette of color swatches, an on/off toggle, and a brightness slider,
//! persisted per device via [`AppState::commit_lighting`] and pushed to the
//! keyboard through `openlogi_agent_core::hardware::set_lighting_in_background`
//! (the agent, over IPC — the GUI has no device I/O of its own).

use crate::ui::theme::FocusRingStyle as _;
use gpui::{
    Context, Div, InteractiveElement, IntoElement, ParentElement, Render, Role, SharedString,
    StatefulInteractiveElement as _, Styled, Subscription, Toggled, Window, div, px, rgb,
};
use gpui_kit::base::Button as BaseButton;
use gpui_kit::component::{Selectable as _, h_flex, slider::Slider, v_flex};
use openlogi_core::color::Rgb;
use openlogi_core::config::Lighting;

use super::key_painter::{KeyMap, key_painter};
use crate::state::{AppState, StateEvent};
use crate::ui::commit_slider::{CommitSlider, SliderRange};
use crate::ui::components::{Toggle, toggle_row};
use crate::ui::slider_row::SliderRow;
use crate::ui::theme::{self, Palette};

pub(super) const SWATCH: f32 = 28.;

/// Preset colors. Deliberately small — covering the common keyboard accent
/// colors.
pub(super) const PALETTE: &[Rgb] = &[
    Rgb::new(0xff, 0x3b, 0x30),
    Rgb::new(0xff, 0x95, 0x00),
    Rgb::new(0xff, 0xcc, 0x00),
    Rgb::new(0x34, 0xc7, 0x59),
    Rgb::new(0x00, 0xc7, 0xbe),
    Rgb::new(0x00, 0x7a, 0xff),
    Rgb::new(0x58, 0x56, 0xd6),
    Rgb::new(0xaf, 0x52, 0xde),
    Rgb::WHITE,
];

pub struct LightingPanel {
    brightness: CommitSlider<u8>,
    /// What clicking a key paints; `None` puts it back on the main color.
    key_brush: Option<Rgb>,
    _state_obs: Subscription,
}

impl LightingPanel {
    pub(super) fn set_brush(&mut self, brush: Option<Rgb>) {
        self.key_brush = brush;
    }

    pub fn new(cx: &mut Context<Self>) -> Self {
        let initial = AppState::try_read(cx).map_or(100, |s| s.lighting().brightness);
        // The slider drives the device only on release, to avoid streaming a
        // frame burst to the keyboard for every intermediate drag value.
        let brightness = CommitSlider::new(
            SliderRange::new(0, 100).step(5.),
            initial,
            cx,
            |_, pct, cx| {
                AppState::apply(cx, |state| {
                    let mut lighting = state.lighting();
                    lighting.enabled = true;
                    lighting.brightness = pct;
                    state.commit_lighting(lighting)
                });
            },
        );
        let state_obs =
            AppState::repaint_on(cx, |event| matches!(event, StateEvent::LightingChanged(_)));
        Self {
            brightness,
            key_brush: PALETTE.first().copied(),
            _state_obs: state_obs,
        }
    }
}

impl Render for LightingPanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let pal = theme::palette(cx);
        let lighting = AppState::try_read(cx)
            .map(AppState::lighting)
            .unwrap_or_default();

        // Pull the slider thumb to the active device's brightness whenever it
        // changed in `AppState` (device switch / external edit), without
        // disturbing an in-progress drag.
        self.brightness.sync(lighting.brightness, window, cx);

        let has_logo = AppState::try_read(cx).is_some_and(AppState::has_logo_light);
        let swatches = |zone| {
            h_flex().gap_2().flex_wrap().children(
                PALETTE
                    .iter()
                    .map(|&color| swatch(color, zone, &lighting, pal))
                    .collect::<Vec<_>>(),
            )
        };
        let colors = if has_logo {
            v_flex()
                .gap_3()
                .child(labeled_row(
                    tr!("lighting.main_light"),
                    swatches(Zone::Main),
                    pal,
                ))
                .child(labeled_row(
                    tr!("lighting.logo_light"),
                    swatches(Zone::Logo),
                    pal,
                ))
        } else {
            v_flex().child(swatches(Zone::Main))
        };
        let key_map = AppState::try_read(cx)
            .filter(|state| state.has_key_lighting())
            .and_then(AppState::current_record)
            .and_then(|record| record.asset.as_ref())
            .and_then(KeyMap::for_asset);
        let painter = key_map.map(|map| key_painter(&map, self.key_brush, &lighting, pal, cx));

        v_flex()
            .gap_3()
            .w_full()
            // The tab and the card already say "Lighting"; the row names what
            // the switch does to the keyboard instead.
            .child(toggle_row(
                "light-row",
                pal,
                tr!("device.light_the_keys"),
                tr!("lighting.keyboard_lighting_description"),
                Toggle::new("light-toggle")
                    .selected(lighting.enabled)
                    .on_change(|enabled, _window, cx| {
                        AppState::apply(cx, |state| {
                            let mut next = state.lighting();
                            next.enabled = *enabled;
                            state.commit_lighting(next)
                        });
                    }),
            ))
            .child(colors)
            .child(
                SliderRow::new(
                    tr!("camera.brightness"),
                    Slider::new(self.brightness.slider()).horizontal(),
                )
                .value(
                    format!("{}%", self.brightness.shown(lighting.brightness)),
                    ["100%"],
                ),
            )
            .children(painter)
    }
}

/// The light a swatch row colors.
#[derive(Clone, Copy)]
enum Zone {
    Main,
    Logo,
}

impl Zone {
    fn id(self) -> &'static str {
        match self {
            Self::Main => "light-swatch",
            Self::Logo => "logo-swatch",
        }
    }

    fn name(self) -> SharedString {
        match self {
            Self::Main => tr!("device.lighting"),
            Self::Logo => tr!("lighting.logo_light"),
        }
    }

    /// The color this zone shows: the logo follows the main color until it has
    /// its own.
    fn color(self, lighting: &Lighting) -> Rgb {
        match self {
            Self::Main => lighting.color,
            Self::Logo => lighting.logo_color_or_main(),
        }
    }

    fn set(self, lighting: &mut Lighting, color: Rgb) {
        match self {
            Self::Main => lighting.color = color,
            Self::Logo => lighting.logo_color = Some(color),
        }
    }
}

/// A swatch row under the name of the light it colors.
fn labeled_row(label: impl Into<SharedString>, row: impl IntoElement, pal: Palette) -> Div {
    v_flex()
        .gap_1p5()
        .child(
            div()
                .text_sm()
                .text_color(pal.text_muted)
                .child(label.into()),
        )
        .child(row)
}

/// One color swatch. Clicking it turns lighting on and sets `zone` to that color.
fn swatch(color: Rgb, zone: Zone, current: &Lighting, pal: Palette) -> impl IntoElement {
    let selected = current.enabled && zone.color(current) == color;
    BaseButton::new((zone.id(), color.packed()))
        .role(Role::RadioButton)
        .selected(selected)
        .accessibility_label(format!("{} #{:06X}", zone.name(), color.packed()))
        .aria_toggled(if selected {
            Toggled::True
        } else {
            Toggled::False
        })
        .aria_selected(selected)
        .size(px(SWATCH))
        .rounded(pal.control_radius)
        .border_2()
        .border_color(if selected {
            theme::accent()
        } else {
            pal.border
        })
        .bg(rgb(color.packed()))
        .cursor_pointer()
        .focus_visible(move |style| style.focus_ring(pal))
        .on_click(move |_event, _window, cx| {
            AppState::apply(cx, |state| {
                let mut next = state.lighting();
                next.enabled = true;
                zone.set(&mut next, color);
                state.commit_lighting(next)
            });
        })
}
