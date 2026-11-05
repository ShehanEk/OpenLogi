//! Click-to-paint colors for individual keys, on the keyboard's own render.
//!
//! The depot's metadata places each key's rectangle on the render and names it
//! by HID usage, the id `0x8080` addresses it by. The painted colors live in
//! [`Lighting::key_colors`] and reach the keyboard through
//! [`AppState::commit_lighting`].

use std::path::PathBuf;

use gpui::prelude::FluentBuilder as _;
use gpui::{
    App, InteractiveElement, IntoElement, ParentElement, Role, StatefulInteractiveElement as _,
    Styled, Toggled, div, img, px, relative, rgb, rgba,
};
use gpui_kit::base::Button as BaseButton;
use gpui_kit::component::{
    Disableable as _, IconName, Selectable as _, Sizable as _,
    button::{Button, ButtonVariants as _},
    h_flex, v_flex,
};
use openlogi_assets::ZoneRect;
use openlogi_core::color::Rgb;
use openlogi_core::config::Lighting;

use super::keyboard_rgb::{LightingPanel, PALETTE, SWATCH};
use crate::services::assets::ResolvedAsset;
use crate::state::AppState;
use crate::ui::theme::{self, FocusRingStyle as _, Palette, Typography as _};

const KEY_ZONE: &str = "PERKEY_KEYBOARD";
/// Render pixels kept above and below the keys; the rest is cable and palm rest.
const CROP_MARGIN: f32 = 40.;
/// Fill alpha of a key with its own color, and of one on the main color.
const PAINTED_ALPHA: u32 = 0xc0;
const MAIN_ALPHA: u32 = 0x40;

/// Where each key sits on a keyboard's render.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct KeyMap {
    image: PathBuf,
    keys: Vec<(u8, ZoneRect)>,
    width: f32,
    height: f32,
    top: f32,
    visible_height: f32,
}

impl KeyMap {
    /// The map of `asset`'s render, or `None` when its depot places no keys.
    /// The rectangles are in the metadata's origin pixels, which only locate
    /// them on a render of that size.
    pub(super) fn for_asset(asset: &ResolvedAsset) -> Option<Self> {
        let image = asset.metadata.image("device_image")?;
        if (image.origin.width, image.origin.height) != (asset.png_width, asset.png_height) {
            return None;
        }
        let keys: Vec<_> = image
            .zones
            .iter()
            .find(|zone| zone.id == KEY_ZONE)?
            .keys()
            .collect();
        let top = keys.iter().map(|(_, r)| r.y).reduce(f32::min)? - CROP_MARGIN;
        let bottom = keys.iter().map(|(_, r)| r.y + r.height).reduce(f32::max)? + CROP_MARGIN;
        #[expect(
            clippy::cast_precision_loss,
            reason = "render dimensions are a few thousand pixels"
        )]
        let (width, height) = (image.origin.width as f32, image.origin.height as f32);
        (width > 0.).then(|| Self {
            image: asset.image_path.clone(),
            keys,
            width,
            height,
            top: top.max(0.),
            visible_height: bottom - top.max(0.),
        })
    }

    /// The fractions of the visible area `rect` covers: left, top, width, height.
    fn place(&self, rect: ZoneRect) -> (f32, f32, f32, f32) {
        (
            rect.x / self.width,
            (rect.y - self.top) / self.visible_height,
            rect.width / self.width,
            rect.height / self.visible_height,
        )
    }
}

/// The brush row, the keyboard, and the reset. `brush` is the color a click
/// paints; `None` puts a key back on the main color.
pub(super) fn key_painter(
    map: &KeyMap,
    brush: Option<Rgb>,
    lighting: &Lighting,
    pal: Palette,
    cx: &mut gpui::Context<LightingPanel>,
) -> impl IntoElement + use<> {
    let brushes = PALETTE
        .iter()
        .map(|&color| {
            brush_swatch(color, brush == Some(color), pal).on_click(cx.listener(
                move |this, _, _, cx| {
                    this.set_brush(Some(color));
                    cx.notify();
                },
            ))
        })
        .collect::<Vec<_>>();
    let main_label = tr!("lighting.use_main_color");
    let eraser = Button::new("key-brush-main")
        .small()
        .ghost()
        .icon(IconName::Close)
        .selected(brush.is_none())
        .accessibility_label(main_label.clone())
        .tooltip(main_label)
        .on_click(cx.listener(|this, _, _, cx| {
            this.set_brush(None);
            cx.notify();
        }));
    let reset = Button::new("key-colors-reset")
        .small()
        .outline()
        .label(tr!("lighting.reset_keys"))
        .disabled(lighting.key_colors.is_empty())
        .on_click(|_, _, cx: &mut App| {
            AppState::apply(cx, |state| {
                let mut next = state.lighting();
                next.key_colors.clear();
                state.commit_lighting(next)
            });
        });
    let keys = map
        .keys
        .iter()
        .map(|&(usage, rect)| key_button(map, usage, rect, brush, lighting, pal))
        .collect::<Vec<_>>();

    v_flex()
        .gap_2()
        .w_full()
        .child(div().text_body().child(tr!("lighting.individual_keys")))
        .child(
            div()
                .text_caption()
                .text_color(pal.text_muted)
                .child(tr!("lighting.individual_keys_description")),
        )
        .child(
            h_flex()
                .gap_2()
                .flex_wrap()
                .items_center()
                .children(brushes)
                .child(eraser)
                .child(div().flex_1())
                .child(reset),
        )
        .child(
            div()
                .relative()
                .w_full()
                .aspect_ratio(map.width / map.visible_height)
                .overflow_hidden()
                .child(
                    img(map.image.clone())
                        .absolute()
                        .left_0()
                        .w_full()
                        .top(relative(-map.top / map.visible_height))
                        .h(relative(map.height / map.visible_height)),
                )
                .children(keys),
        )
}

fn brush_swatch(color: Rgb, selected: bool, pal: Palette) -> BaseButton {
    BaseButton::new(("key-brush", color.packed()))
        .role(Role::RadioButton)
        .selected(selected)
        .accessibility_label(format!(
            "{} #{:06X}",
            tr!("lighting.individual_keys"),
            color.packed()
        ))
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
}

/// One key, tinted with the color it shows. Clicking applies the brush.
fn key_button(
    map: &KeyMap,
    usage: u8,
    rect: ZoneRect,
    brush: Option<Rgb>,
    lighting: &Lighting,
    pal: Palette,
) -> BaseButton {
    let (left, top, width, height) = map.place(rect);
    let painted = lighting.key_colors.iter().any(|key| key.usage == usage);
    let alpha = if painted { PAINTED_ALPHA } else { MAIN_ALPHA };
    let color = lighting.key_color_or_main(usage);
    BaseButton::new(("paint-key", u32::from(usage)))
        .absolute()
        .left(relative(left))
        .top(relative(top))
        .w(relative(width))
        .h(relative(height))
        .rounded(px(3.))
        .border_1()
        .border_color(gpui::transparent_black())
        .when(lighting.enabled, |key| {
            key.bg(rgba((color.packed() << 8) | alpha))
        })
        .cursor_pointer()
        .accessibility_label(format!("{} {usage}", tr!("lighting.key")))
        .hover(|key| key.border_color(theme::accent()))
        .focus_visible(move |style| style.focus_ring(pal))
        .on_click(move |_, _, cx: &mut App| {
            AppState::apply(cx, |state| {
                let mut next = state.lighting();
                next.enabled = true;
                next.set_key_color(usage, brush);
                state.commit_lighting(next)
            });
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn asset(png_width: u32) -> ResolvedAsset {
        let metadata = serde_json::from_str(
            r#"{"images":[{"key":"device_image","origin":{"width":2000,"height":1000},"zones":[
            {"id":"PERKEY_INDICATOR","components":[{"id":2,"renderings":[{"rect":
                {"x":1,"y":1,"width":1,"height":1}}]}]},
            {"id":"PERKEY_KEYBOARD","components":[
                {"id":41,"renderings":[{"type":"rect","rect":
                    {"x":100,"y":200,"width":100,"height":100}}]},
                {"id":44,"renderings":[{"type":"rect","rect":
                    {"x":500,"y":500,"width":400,"height":100}}]}
            ]}]}]}"#,
        )
        .expect("valid metadata");
        ResolvedAsset {
            depot: "g513".into(),
            display_name: "G512".into(),
            kind: None,
            image_path: "front.png".into(),
            hero_image_path: Some("front.png".into()),
            glow: None,
            metadata,
            png_width,
            png_height: 1000,
            side_view: None,
        }
    }

    #[test]
    fn a_key_is_placed_inside_the_cropped_view_of_the_render() {
        let map = KeyMap::for_asset(&asset(2000)).expect("a depot with keys");

        assert_eq!(
            map.keys.iter().map(|(usage, _)| *usage).collect::<Vec<_>>(),
            [41, 44]
        );
        // The keys span y 200..600, so 40 px either side leaves 160..640.
        assert_eq!((map.top, map.visible_height), (160., 480.));
        let (left, top, width, height) = map.place(map.keys[0].1);
        assert_eq!((left, width), (0.05, 0.05));
        assert_eq!(top, 40. / 480.);
        assert_eq!(height, 100. / 480.);
    }

    #[test]
    fn keys_are_not_placed_on_a_render_of_another_size() {
        assert!(KeyMap::for_asset(&asset(1000)).is_none());
    }
}
