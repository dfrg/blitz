//! `calc()` values that are not linear in their basis, which winkin resolves through the host's
//! function as layout learns the basis.
//!
//! A nonlinear [`css::Calc`] is kept in the layout's [`CalcValues`] and passed on as a
//! [`CalcHandle`]: the address of its boxed entry, which [`HostCalc`] resolves. A layout keeps
//! its entries until it is built again, so no handle it holds outlives its value.

use std::sync::Mutex;

use blitz_text::style as css;
use winkin::style::{CalcHandle, CalcResolver, LengthPercentage, LengthPercentageOrCalc};

/// A `calc()` and the device pixels per CSS pixel its layout is built at.
struct ScaledCalc {
    calc: css::Calc,
    scale: f32,
}

/// The nonlinear `calc()` values a layout's styles hold, each boxed so that its address, the
/// handle winkin keeps, stays put.
#[derive(Default)]
pub(crate) struct CalcValues(Mutex<Vec<Box<ScaledCalc>>>);

impl CalcValues {
    /// Drops every value, for a new build. No handle to them is read after.
    pub(crate) fn clear(&mut self) {
        if let Ok(values) = self.0.get_mut() {
            values.clear();
        }
    }

    /// Returns `value` in device pixels: a length and a fraction where it is linear in its
    /// basis, and a handle to it otherwise.
    pub(crate) fn length_percentage(
        &self,
        value: &css::LengthPercentage,
        scale: f32,
    ) -> LengthPercentageOrCalc {
        let css::LengthPercentage::Nonlinear(calc) = value else {
            return linear(value, scale).into();
        };
        let boxed = Box::new(ScaledCalc {
            calc: calc.clone(),
            scale,
        });
        let handle = CalcHandle::new(&*boxed as *const ScaledCalc as usize as u64);
        match self.0.lock() {
            Ok(mut values) => {
                values.push(boxed);
                LengthPercentageOrCalc::Calc(handle)
            }
            // A poisoned store keeps nothing: take the value at a basis of zero rather than hand
            // out a handle nothing owns.
            Err(_) => LengthPercentageOrCalc::from_px(calc.resolve(0.0) * scale),
        }
    }
}

/// Resolves the handles [`CalcValues`] hands out.
pub(crate) struct HostCalc;

impl CalcResolver for HostCalc {
    fn resolve(&self, calc: CalcHandle, basis: f32) -> f32 {
        // SAFETY: winkin resolves only the handles a layout's styles hold, and the layout keeps
        // their boxed values in its `CalcValues` until it is built again, which replaces every
        // handle.
        let value = unsafe { &*(calc.get() as usize as *const ScaledCalc) };
        let scale = value.scale;
        value.calc.resolve(basis / scale) * scale
    }
}

/// Returns `value` in device pixels as a length and a fraction of whatever layout resolves it
/// against. A nonlinear `calc()` is split by resolving it against two bases, which holds only
/// near them; [`CalcValues::length_percentage`] keeps it whole instead.
pub(crate) fn linear(value: &css::LengthPercentage, scale: f32) -> LengthPercentage {
    if let css::LengthPercentage::Calc { px, fraction } = *value {
        return LengthPercentage {
            px: px * scale,
            fraction,
        };
    }
    let at_zero = value.resolve(0.0);
    let at_hundred = value.resolve(100.0);
    LengthPercentage {
        px: at_zero * scale,
        fraction: (at_hundred - at_zero) / 100.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A nonlinear value becomes a handle that resolves through the host's function in device
    /// pixels: `max(20px, 10%)` at a scale of 2 is 40 device pixels against 200 and 80 against
    /// 800.
    #[test]
    fn a_handle_resolves_through_the_host_in_device_pixels() {
        let calcs = CalcValues::default();
        let max =
            css::LengthPercentage::Nonlinear(css::Calc::new(|basis| f32::max(20.0, 0.1 * basis)));
        let LengthPercentageOrCalc::Calc(handle) = calcs.length_percentage(&max, 2.0) else {
            panic!("a nonlinear calc() is kept whole");
        };
        assert_eq!(HostCalc.resolve(handle, 200.0), 40.0);
        assert_eq!(HostCalc.resolve(handle, 800.0), 80.0);
        let linear = css::LengthPercentage::Calc {
            px: 10.0,
            fraction: 0.05,
        };
        let LengthPercentageOrCalc::LengthPercentage(split) = calcs.length_percentage(&linear, 2.0)
        else {
            panic!("a linear calc() is split");
        };
        assert_eq!((split.px, split.fraction), (20.0, 0.05));
    }
}
