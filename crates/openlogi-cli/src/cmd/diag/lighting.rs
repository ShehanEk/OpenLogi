//! `openlogi diag lighting <RRGGBB>` — set an online RGB device to a solid
//! colour via the same HID++ lighting write path the GUI uses.

use anyhow::{Context as _, Result};
use clap::{Args, ValueEnum};
use openlogi_core::color::Rgb;
use openlogi_hid::LightingMethod;

use super::select_device;

const LIGHTING_FEATURES: &[u16] = &[0x8070, 0x8071, 0x8080, 0x8081];

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum Method {
    /// Prefer 0x8070 then 0x8071 effects, falling back to 0x8081 then 0x8080.
    Auto,
    /// Force 0x8070 ColorLedEffects (the fixed-effect onboard override).
    Effects,
    /// Force 0x8080 PerKeyLighting (the raw per-key stream).
    Perkey,
    /// Force 0x8081 PerKeyLighting2 (the zone-addressed successor).
    Perkeyv2,
}

impl From<Method> for LightingMethod {
    fn from(m: Method) -> Self {
        match m {
            Method::Auto => Self::Auto,
            Method::Effects => Self::Effects,
            Method::Perkey => Self::PerKey,
            Method::Perkeyv2 => Self::PerKeyV2,
        }
    }
}

#[derive(Debug, Args)]
pub struct LightingArgs {
    /// Colour as `RRGGBB` hex (e.g. `ff0000` for red).
    pub color: String,

    /// Run against the online device whose name contains this string
    /// (case-insensitive). Useful when several devices are connected.
    #[arg(long, value_name = "NAME")]
    pub device: Option<String>,

    /// Which HID++ lighting path to drive.
    #[arg(long, value_enum, default_value_t = Method::Auto)]
    pub method: Method,
}

pub async fn run(args: LightingArgs) -> Result<()> {
    let color: Rgb = args.color.trim_start_matches('#').parse()?;
    let (r, g, b) = color.components();

    let (route, name) = select_device(args.device.as_deref(), LIGHTING_FEATURES).await?;

    let method: LightingMethod = args.method.into();
    println!("setting {name} ({route}) to #{r:02x}{g:02x}{b:02x} via {method:?}");
    openlogi_hid::set_keyboard_color_with(&route, method, r, g, b).await?;
    println!("done — {name} should now be solid #{r:02x}{g:02x}{b:02x}");
    Ok(())
}

#[derive(Debug, Args)]
pub struct LightingZonesArgs {
    /// Run against the online device whose name contains this string
    /// (case-insensitive). Useful when several devices are connected.
    #[arg(long, value_name = "NAME")]
    pub device: Option<String>,
}

/// Print each `0x8070` zone the device reports. Writes nothing.
pub async fn run_zones(args: LightingZonesArgs) -> Result<()> {
    let (route, name) = select_device(args.device.as_deref(), &[0x8070]).await?;
    println!("{name} ({route})");
    for zone in openlogi_hid::read_lighting_zones(&route).await? {
        match zone.location {
            Some(location) => println!("  zone {}: {location:?}", zone.index),
            None => println!("  zone {}: unknown location", zone.index),
        }
    }
    Ok(())
}

#[derive(Debug, Args)]
pub struct LightingKeysArgs {
    /// Keys as `USAGE=RRGGBB`, with the HID usage in decimal (Esc is `41`).
    #[arg(required = true)]
    pub keys: Vec<String>,

    /// Colour of every other key, as `RRGGBB`.
    #[arg(long, default_value = "000000")]
    pub base: String,

    /// Run against the online device whose name contains this string
    /// (case-insensitive). Useful when several devices are connected.
    #[arg(long, value_name = "NAME")]
    pub device: Option<String>,
}

/// Paint the listed keys through `0x8080`.
pub async fn run_keys(args: LightingKeysArgs) -> Result<()> {
    let base: Rgb = args.base.parse()?;
    let keys = args
        .keys
        .iter()
        .map(|key| {
            let (usage, color) = key
                .split_once('=')
                .with_context(|| format!("{key:?} is not USAGE=RRGGBB"))?;
            Ok((usage.parse()?, color.parse()?))
        })
        .collect::<Result<Vec<(u8, Rgb)>>>()?;
    let (route, name) = select_device(args.device.as_deref(), &[0x8080]).await?;
    println!("setting {} keys on {name} ({route})", keys.len());
    openlogi_hid::set_key_colors(&route, base, keys).await?;
    println!("done");
    Ok(())
}

#[cfg(test)]
mod color_validation_tests {
    use openlogi_core::color::RgbParseError;

    use super::{LightingArgs, Method, run};

    fn args(color: &str) -> LightingArgs {
        LightingArgs {
            color: color.to_string(),
            device: None,
            method: Method::Auto,
        }
    }

    /// Invalid colours are rejected before any device I/O, so `run` is safe to
    /// call in-process here. Valid colours proceed to hardware enumeration and
    /// are deliberately not exercised.
    #[tokio::test]
    async fn rejects_malformed_colors_before_touching_hardware() {
        for bad in ["zzz", "ff000", "ff00001", "gg0000", ""] {
            let err = run(args(bad)).await.unwrap_err();
            assert!(
                err.downcast_ref::<RgbParseError>().is_some(),
                "{bad:?} should fail Rgb parsing, got: {err}"
            );
        }
    }

    #[tokio::test]
    async fn hash_prefix_is_stripped_before_validation() {
        // `#zzzzzz` still fails, and the rejected input the error reports is
        // `zzzzzz` — proving the `#` is stripped rather than counted toward
        // the 6-digit length.
        let err = run(args("#zzzzzz")).await.unwrap_err();
        let parse = err
            .downcast_ref::<RgbParseError>()
            .expect("Rgb parse error");
        assert_eq!(
            parse.to_string(),
            r#"invalid RGB color "zzzzzz": expected 6 hex digits ("RRGGBB", no '#')"#
        );
    }
}
