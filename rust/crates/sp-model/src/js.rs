//! Numbers the way JavaScript writes them.
//!
//! TypeScript has one number type, so `JSON.stringify` writes a whole
//! number as `20` where serde_json writes an `f64` as `20.0`. Every crate
//! that emits JSON the TypeScript also emits (the CLI's `--json`, the
//! `--debug` elements dump, settings files) needs the JavaScript spelling,
//! so fields typed `f64` here carry `#[serde(with = "crate::js::number")]`.
//!
//! What it covers: integral values print without a fraction, and `-0`
//! prints as `0` (as `JSON.stringify(-0)` does). Non-integral values go
//! through serde_json's shortest round-trip form, which agrees with
//! JavaScript's `Number.prototype.toString` for every value a screenplay
//! produces; the two differ only in exponent spelling at magnitudes of
//! 1e21 and above or below 1e-6 (`1e+21` in JavaScript). Non-finite values
//! cannot occur in JSON on either side.

/// The largest integer an `f64` holds exactly (`Number.MAX_SAFE_INTEGER`).
const MAX_SAFE: f64 = 9_007_199_254_740_991.0;

/// `f64` written as JavaScript would write it.
pub mod number {
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(v: &f64, s: S) -> Result<S::Ok, S::Error> {
        super::write(*v, s)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<f64, D::Error> {
        f64::deserialize(d)
    }
}

/// `Option<f64>` written as JavaScript would write it. Pair with
/// `skip_serializing_if = "Option::is_none"` and `default`.
pub mod opt_number {
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(v: &Option<f64>, s: S) -> Result<S::Ok, S::Error> {
        match v {
            Some(v) => super::write(*v, s),
            None => s.serialize_none(),
        }
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Option<f64>, D::Error> {
        Option::<f64>::deserialize(d)
    }
}

/// `Vec<f64>` written as JavaScript would write it (a pdf.js transform).
pub mod numbers {
    use serde::ser::SerializeSeq;
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(v: &[f64], s: S) -> Result<S::Ok, S::Error> {
        let mut seq = s.serialize_seq(Some(v.len()))?;
        for x in v {
            seq.serialize_element(&super::Js(*x))?;
        }
        seq.end()
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<f64>, D::Error> {
        Vec::<f64>::deserialize(d)
    }
}

/// A bare `f64` that serializes the JavaScript way, for building
/// `serde_json::Value`s by hand.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Js(pub f64);

impl serde::Serialize for Js {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        write(self.0, s)
    }
}

fn write<S: serde::Serializer>(v: f64, s: S) -> Result<S::Ok, S::Error> {
    if v == 0.0 {
        // Catches -0 too, which JSON.stringify writes as 0.
        return s.serialize_i64(0);
    }
    if v.is_finite() && v.fract() == 0.0 && v.abs() <= MAX_SAFE {
        // In range by the check above, so the cast is exact.
        #[allow(clippy::cast_possible_truncation)]
        return s.serialize_i64(v as i64);
    }
    s.serialize_f64(v)
}

#[cfg(test)]
mod tests {
    use super::Js;

    fn js(v: f64) -> String {
        serde_json::to_string(&Js(v)).unwrap()
    }

    #[test]
    fn whole_numbers_have_no_fraction() {
        assert_eq!(js(20.0), "20");
        assert_eq!(js(-5.0), "-5");
        assert_eq!(js(792.0), "792");
    }

    #[test]
    fn negative_zero_is_zero() {
        assert_eq!(js(-0.0), "0");
    }

    #[test]
    fn fractions_round_trip_shortest() {
        assert_eq!(js(0.4), "0.4");
        assert_eq!(js(612.0001), "612.0001");
        assert_eq!(js(0.1 + 0.2), "0.30000000000000004");
    }
}
