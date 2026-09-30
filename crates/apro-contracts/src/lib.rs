//! # apro-contracts
//!
//! Payload contracts shared between APRO product apps.
//!
//! The platform deliberately does **not** define a universal schema — the hub stores
//! opaque bytes. But two apps that hand data to each other still have to agree on what
//! those bytes mean, and the dangerous part of that agreement is units. This crate holds
//! the agreed shapes and, more importantly, the conversions, in one place that both the
//! producer and the consumer depend on.
//!
//! It depends on nothing but `serde`: no CAD kernel, no flight-dynamics model, no HTTP.
//! Each app maps its own types in and out.
//!
//! ## The unit boundary
//!
//! aproCAD computes mass properties in **document units**, which default to millimetres.
//! Its mass is already kilograms (density is `kg/mm³`), but its centre of gravity is in
//! millimetres and its inertia is in `kg·mm²`. HexaDOF is SI internally.
//!
//! | quantity | factor |
//! | --- | --- |
//! | mass | 1 (already kg) |
//! | centre of gravity | `metres_per_unit` (1000× for mm) |
//! | inertia | `metres_per_unit²` (**1e6×** for mm) |
//! | volume | `metres_per_unit³` |
//!
//! An inertia error is a factor of one million and still produces plausible-looking
//! flight dynamics, so [`MassPropertiesV1`] declares its units explicitly and
//! [`MassPropertiesV1::validate`] refuses a payload that does not declare SI rather than
//! guessing.
//!
//! ## The datum convention
//!
//! A CAD document's origin is wherever the author put it. A flight-dynamics body datum is
//! usually the nose tip, the base, or the centre of gravity. Nothing in either model can
//! tell you which, so the contract makes it **explicit** rather than inferred:
//! [`MassPropertiesV1::datum_offset_m`] states where the CAD origin sits in the body
//! datum frame.
//!
//! ```text
//! p_body = p_cad + datum_offset_m
//! ```
//!
//! Inertia is unaffected: [`MassPropertiesV1::inertia_kg_m2`] is about the centre of
//! gravity, and translating the coordinate origin does not change an inertia about the
//! centre of gravity. Only the centre of gravity moves, which is why
//! [`MassPropertiesV1::center_of_gravity_in_datum`] exists — so the shift is implemented
//! once, here, and tested, instead of being re-derived (and sign-flipped) by each consumer.

use serde::{Deserialize, Serialize};

/// Published artifact type for CAD mass properties.
///
/// The unit contract is in the type id, so it is immutable and self-documenting: a
/// consumer cannot receive this type without knowing the payload is SI.
///
/// Note the `-v1` suffix rather than `.v1`: type-id segments are kebab-case only
/// (`a-z`, `0-9`, `-`), so a dot is rejected as an invalid type id.
pub const MASS_PROPERTIES_TYPE: &str = "apro-cad/mass-properties-si-v1";

/// The marker every payload of [`MASS_PROPERTIES_TYPE`] must carry.
pub const SI_MARKER: &str = "SI";

/// Every payload of [`MASS_PROPERTIES_TYPE`] carries this in `schema`.
pub const MASS_PROPERTIES_SCHEMA_VERSION: u32 = 1;

// ---------------------------------------------------------------------------
// Units
// ---------------------------------------------------------------------------

/// The unit system a CAD document is authored in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceUnits {
    Millimeters,
    Centimeters,
    Meters,
    Inches,
    Feet,
}

impl SourceUnits {
    /// Metres per one unit of length.
    pub fn metres_per_unit(self) -> f64 {
        match self {
            SourceUnits::Millimeters => 0.001,
            SourceUnits::Centimeters => 0.01,
            SourceUnits::Meters => 1.0,
            SourceUnits::Inches => 0.0254,
            SourceUnits::Feet => 0.3048,
        }
    }

    /// `kg·m²` per one inertia unit (`kg·<length>²`).
    pub fn inertia_scale(self) -> f64 {
        let s = self.metres_per_unit();
        s * s
    }

    /// `m³` per one volume unit.
    pub fn volume_scale(self) -> f64 {
        let s = self.metres_per_unit();
        s * s * s
    }

    pub fn as_str(self) -> &'static str {
        match self {
            SourceUnits::Millimeters => "mm",
            SourceUnits::Centimeters => "cm",
            SourceUnits::Meters => "m",
            SourceUnits::Inches => "in",
            SourceUnits::Feet => "ft",
        }
    }
}

impl std::str::FromStr for SourceUnits {
    type Err = ContractError;

    /// Accepts both the short form (`mm`) and the full name (`millimeters`), because the
    /// unit arrives from a hand-edited document in one case and a UI in the other.
    fn from_str(raw: &str) -> Result<Self, Self::Err> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "mm" | "millimeter" | "millimeters" => Ok(SourceUnits::Millimeters),
            "cm" | "centimeter" | "centimeters" => Ok(SourceUnits::Centimeters),
            "m" | "meter" | "meters" | "metre" | "metres" => Ok(SourceUnits::Meters),
            "in" | "inch" | "inches" => Ok(SourceUnits::Inches),
            "ft" | "foot" | "feet" => Ok(SourceUnits::Feet),
            other => Err(ContractError::UnknownUnits(other.to_string())),
        }
    }
}

// ---------------------------------------------------------------------------
// Producer-side input
// ---------------------------------------------------------------------------

/// Mass properties in **document units**, as a CAD application computes them.
///
/// This is the app-agnostic input to [`MassPropertiesV1::from_document`]. Keeping it a
/// plain struct of numbers — rather than taking `apro_massprops::MassProperties` — is what
/// lets the conversion live here, where the consumer can also see and test it, without
/// this crate depending on any application.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DocumentMassProperties {
    /// Volume in cubic document units.
    pub volume: f64,
    /// Mass in kilograms. aproCAD's density is `kg/mm³`, so this needs no conversion.
    pub mass_kg: f64,
    /// Centre of gravity in document units, in the CAD document frame.
    pub center_of_mass: [f64; 3],
    /// Inertia about the centre of gravity, in `kg·<length unit>²`.
    pub inertia: [[f64; 3]; 3],
}

// ---------------------------------------------------------------------------
// The published payload
// ---------------------------------------------------------------------------

/// SI mass properties, as published to the platform.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MassPropertiesV1 {
    /// Always [`MASS_PROPERTIES_TYPE`].
    pub schema: String,
    /// Always [`SI_MARKER`]. Consumers must never have to guess.
    pub units: String,
    pub mass_kg: f64,
    /// Centre of gravity in metres, in the **CAD document frame**.
    ///
    /// Use [`Self::center_of_gravity_in_datum`] to get it in the consumer's body frame.
    pub center_of_gravity_m: [f64; 3],
    /// Inertia about the centre of gravity, in `kg·m²`, as a symmetric 3x3 matrix.
    ///
    /// Unaffected by [`Self::datum_offset_m`]: an inertia about the centre of gravity is
    /// invariant under a translation of the coordinate origin.
    pub inertia_kg_m2: [[f64; 3]; 3],
    pub volume_m3: f64,
    /// Where the **CAD document origin** sits in the consumer's body datum frame, in
    /// metres. `p_body = p_cad + datum_offset_m`.
    ///
    /// `[0, 0, 0]` means the CAD origin *is* the body datum — declared, not assumed.
    #[serde(default)]
    pub datum_offset_m: [f64; 3],
    /// Original document units, kept for provenance.
    pub source_units: SourceUnits,
    /// Reference geometry, when the producer could derive it.
    ///
    /// Optional because not every producer knows its body's aerodynamic conventions, and
    /// inventing a number would be worse than admitting the gap. A consumer that finds
    /// `None` must ask the user rather than falling back to a default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reference_geometry: Option<ReferenceGeometrySi>,
    /// A human-readable note about how the numbers were produced and converted.
    pub source_note: String,
}

impl MassPropertiesV1 {
    /// Convert native CAD mass properties into the published SI form.
    ///
    /// This is the only place unit conversion happens on the producer side, for any
    /// producer.
    pub fn from_document(
        native: DocumentMassProperties,
        source: SourceUnits,
        datum_offset_m: [f64; 3],
    ) -> Self {
        let length = source.metres_per_unit();
        let inertia = source.inertia_scale();
        let volume = source.volume_scale();

        Self {
            schema: MASS_PROPERTIES_TYPE.to_string(),
            units: SI_MARKER.to_string(),
            // Already kilograms: CAD density is kg per cubic document unit scaled to mm.
            mass_kg: native.mass_kg,
            center_of_gravity_m: [
                native.center_of_mass[0] * length,
                native.center_of_mass[1] * length,
                native.center_of_mass[2] * length,
            ],
            inertia_kg_m2: native.inertia.map(|row| row.map(|cell| cell * inertia)),
            volume_m3: native.volume * volume,
            datum_offset_m,
            source_units: source,
            reference_geometry: None,
            source_note: format!(
                "converted from {} at the publish boundary \
                 (cg ÷{:.0}, inertia ÷{:.0}); mass needed no conversion because CAD \
                 density is kg per cubic {}",
                source.as_str(),
                1.0 / length,
                1.0 / inertia,
                source.as_str()
            ),
        }
    }

    /// Attach the reference geometry this body should be flown with.
    ///
    /// A builder rather than a constructor argument: a producer that cannot state its
    /// aerodynamic convention should leave it unset, and the type makes that the default
    /// instead of something you have to remember to pass `None` for.
    pub fn with_reference_geometry(mut self, reference: ReferenceGeometrySi) -> Self {
        self.reference_geometry = Some(reference);
        self
    }

    /// The centre of gravity expressed in the consumer's body datum frame.
    pub fn center_of_gravity_in_datum(&self) -> [f64; 3] {
        [
            self.center_of_gravity_m[0] + self.datum_offset_m[0],
            self.center_of_gravity_m[1] + self.datum_offset_m[1],
            self.center_of_gravity_m[2] + self.datum_offset_m[2],
        ]
    }

    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }

    pub fn from_json(text: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(text)
    }

    /// Largest relative asymmetry in the inertia matrix, for a consumer's sanity check.
    pub fn inertia_asymmetry(&self) -> f64 {
        let m = &self.inertia_kg_m2;
        let pairs = [(m[0][1], m[1][0]), (m[0][2], m[2][0]), (m[1][2], m[2][1])];
        pairs
            .iter()
            .map(|(a, b)| {
                let scale = a.abs().max(b.abs()).max(1e-12);
                (a - b).abs() / scale
            })
            .fold(0.0_f64, f64::max)
    }

    /// Reject a payload that cannot be trusted.
    ///
    /// The unit check is the important one. A consumer that silently accepted document
    /// units would be wrong by a factor of a million in inertia — physically absurd, but
    /// numerically smooth enough to produce a simulation that merely looks sluggish.
    pub fn validate(&self) -> Result<(), ContractError> {
        if self.units != SI_MARKER {
            return Err(ContractError::NotSi {
                units: self.units.clone(),
            });
        }
        if !self.mass_kg.is_finite() {
            return Err(ContractError::NotFinite { field: "mass_kg" });
        }
        if self.mass_kg < 0.0 {
            return Err(ContractError::NegativeMass {
                mass_kg: self.mass_kg,
            });
        }
        if !self.center_of_gravity_m.iter().all(|v| v.is_finite()) {
            return Err(ContractError::NotFinite {
                field: "center_of_gravity_m",
            });
        }
        if !self.datum_offset_m.iter().all(|v| v.is_finite()) {
            return Err(ContractError::NotFinite {
                field: "datum_offset_m",
            });
        }
        if !self.volume_m3.is_finite() {
            return Err(ContractError::NotFinite { field: "volume_m3" });
        }
        if !self.inertia_kg_m2.iter().flatten().all(|v| v.is_finite()) {
            return Err(ContractError::NotFinite {
                field: "inertia_kg_m2",
            });
        }
        // A wildly asymmetric tensor means the producer assembled it by hand and got a
        // transpose wrong. Compare against the tensor's own scale so a small part with
        // tiny inertias is not judged by an absolute threshold.
        let scale = self
            .inertia_kg_m2
            .iter()
            .flatten()
            .fold(0.0_f64, |acc, v| acc.max(v.abs()));
        if scale > 0.0 && self.inertia_asymmetry() > 1e-6 {
            return Err(ContractError::AsymmetricInertia {
                asymmetry: self.inertia_asymmetry(),
            });
        }
        Ok(())
    }

    /// Parse, then validate. The order matters: a payload from the wire is untrusted.
    pub fn from_json_checked(text: &str) -> Result<Self, ContractError> {
        let payload: Self = serde_json::from_str(text).map_err(ContractError::Json)?;
        payload.validate()?;
        Ok(payload)
    }
}

/// Reference geometry a consumer needs but a CAD document does not determine.
///
/// Aerodynamic reference area is a **convention**, not a derivation: a rocket's is usually
/// the body cross-section, but a winged vehicle's may be the planform, and a multi-stage
/// stack's may be the first-stage diameter. Nothing in the CAD document says which, so the
/// producer states the convention it used in [`Self::convention`] rather than leaving the
/// consumer to guess — and a consumer that does not trust the stated convention can ask
/// the user instead of silently accepting a number.
///
/// It travels inside [`MassPropertiesV1`] rather than as a second artifact on purpose: the
/// two describe one body, and separate artifacts could be read at mismatched revisions.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReferenceGeometrySi {
    /// Aerodynamic reference area, `m²`.
    pub reference_area_m2: f64,
    /// Reference length, `m`. Conventionally the body length.
    pub reference_length_m: f64,
    /// Maximum body diameter, `m`.
    pub body_diameter_m: f64,
    /// How these numbers were chosen, in the producer's words. Free text, but required:
    /// a bare number with no stated convention is exactly what this field exists to stop.
    #[serde(default)]
    pub convention: String,
}

impl ReferenceGeometrySi {
    /// Reference geometry for a circular body of the given diameter, the common case.
    pub fn from_body_diameter(diameter_m: f64, length_m: f64) -> Self {
        let radius = diameter_m / 2.0;
        Self {
            reference_area_m2: std::f64::consts::PI * radius * radius,
            reference_length_m: length_m,
            body_diameter_m: diameter_m,
            convention: "reference area is the area of a circle of the body diameter".to_string(),
        }
    }

    /// Derive reference geometry from an axis-aligned bounding box, in metres.
    ///
    /// `longitudinal` is the index (0 = x, 1 = y, 2 = z) of the body's long axis. The
    /// convention this applies — and records — is:
    ///
    /// * reference **length** is the bounding-box extent along the long axis;
    /// * body **diameter** is the *smaller* of the two transverse extents, so an
    ///   asymmetric body is not credited with a wider reference area than it has;
    /// * reference **area** is a circle of that diameter.
    ///
    /// This is a stated choice, not a law. A vehicle whose real reference area is a
    /// planform must override it, which is why the convention travels with the numbers.
    pub fn from_bounding_box(extent_m: [f64; 3], longitudinal: usize) -> Self {
        let transverse: Vec<f64> = (0..3)
            .filter(|axis| *axis != longitudinal)
            .map(|axis| extent_m[axis].abs())
            .collect();
        let diameter = transverse.iter().copied().fold(f64::INFINITY, f64::min);
        let length = extent_m[longitudinal].abs();
        let radius = diameter / 2.0;

        Self {
            reference_area_m2: std::f64::consts::PI * radius * radius,
            reference_length_m: length,
            body_diameter_m: diameter,
            convention: format!(
                "derived from the assembly bounding box: reference length is the extent \
                 along axis {longitudinal}, body diameter is the smaller of the two \
                 transverse extents, and reference area is a circle of that diameter"
            ),
        }
    }

    pub fn validate(&self) -> Result<(), ContractError> {
        for (field, value) in [
            ("reference_area_m2", self.reference_area_m2),
            ("reference_length_m", self.reference_length_m),
            ("body_diameter_m", self.body_diameter_m),
        ] {
            if !value.is_finite() {
                return Err(ContractError::NotFinite { field });
            }
        }
        if self.reference_area_m2 <= 0.0 {
            return Err(ContractError::InvalidReferenceGeometry {
                reason: "reference_area_m2 must be positive".into(),
            });
        }
        if self.reference_length_m <= 0.0 {
            return Err(ContractError::InvalidReferenceGeometry {
                reason: "reference_length_m must be positive".into(),
            });
        }
        if self.body_diameter_m <= 0.0 {
            return Err(ContractError::InvalidReferenceGeometry {
                reason: "body_diameter_m must be positive".into(),
            });
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub enum ContractError {
    /// The payload did not declare SI. Refusing beats guessing by a factor of 1e6.
    NotSi {
        units: String,
    },
    NotFinite {
        field: &'static str,
    },
    NegativeMass {
        mass_kg: f64,
    },
    AsymmetricInertia {
        asymmetry: f64,
    },
    InvalidReferenceGeometry {
        reason: String,
    },
    UnknownUnits(String),
    Json(serde_json::Error),
}

impl std::fmt::Display for ContractError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ContractError::NotSi { units } => write!(
                f,
                "payload declares units {units:?}; this consumer requires {SI_MARKER:?} \
                 and will not guess"
            ),
            ContractError::NotFinite { field } => write!(f, "field {field} is not finite"),
            ContractError::NegativeMass { mass_kg } => {
                write!(f, "mass {mass_kg} kg is negative")
            }
            ContractError::AsymmetricInertia { asymmetry } => write!(
                f,
                "inertia tensor is asymmetric by a relative {asymmetry:.3e}; \
                 the matrix was probably assembled with a transpose error"
            ),
            ContractError::InvalidReferenceGeometry { reason } => {
                write!(f, "invalid reference geometry: {reason}")
            }
            ContractError::UnknownUnits(raw) => write!(
                f,
                "unknown unit system {raw:?}; expected one of mm, cm, m, in, ft"
            ),
            ContractError::Json(err) => write!(f, "payload is not valid JSON: {err}"),
        }
    }
}

impl std::error::Error for ContractError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            ContractError::Json(err) => Some(err),
            _ => None,
        }
    }
}

impl From<serde_json::Error> for ContractError {
    fn from(err: serde_json::Error) -> Self {
        ContractError::Json(err)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// A rectangular block of Al-6061-T6, 100 x 100 x `length` mm, with its corner at the
    /// CAD origin. Closed form, so a unit error shows up as a wrong number rather than a
    /// self-consistent one.
    const EDGE_MM: f64 = 100.0;
    const DENSITY_KG_PER_MM3: f64 = 2700.0 / 1.0e9;

    fn block(length_mm: f64) -> (DocumentMassProperties, [f64; 4]) {
        let volume = EDGE_MM * EDGE_MM * length_mm;
        let mass = DENSITY_KG_PER_MM3 * volume;
        let ixx = mass / 12.0 * (EDGE_MM * EDGE_MM + length_mm * length_mm);
        let izz = mass / 12.0 * (EDGE_MM * EDGE_MM + EDGE_MM * EDGE_MM);

        let native = DocumentMassProperties {
            volume,
            mass_kg: mass,
            center_of_mass: [EDGE_MM / 2.0, EDGE_MM / 2.0, length_mm / 2.0],
            inertia: [
                [ixx, 0.0, 0.0],
                [
                    0.0,
                    mass / 12.0 * (EDGE_MM * EDGE_MM + length_mm * length_mm),
                    0.0,
                ],
                [0.0, 0.0, izz],
            ],
        };
        (native, [mass, volume, ixx, izz])
    }

    #[test]
    fn inertia_is_scaled_by_one_million_for_millimetres() {
        let (native, [_, _, ixx_mm, _]) = block(600.0);
        let payload = MassPropertiesV1::from_document(native, SourceUnits::Millimeters, [0.0; 3]);

        // The whole point of the crate: kg·mm² -> kg·m² is 1e6, not 1e3.
        let expected_ixx = ixx_mm / 1.0e6;
        let drift = (payload.inertia_kg_m2[0][0] - expected_ixx).abs() / expected_ixx;
        assert!(drift < 1e-12, "Ixx drift {drift}");

        let ratio = payload.inertia_kg_m2[0][0] / ixx_mm;
        assert!((ratio - 1.0e-6).abs() < 1e-18, "ratio {ratio}");
    }

    #[test]
    fn centre_of_gravity_is_scaled_by_one_thousand_for_millimetres() {
        let (native, _) = block(600.0);
        let payload = MassPropertiesV1::from_document(native, SourceUnits::Millimeters, [0.0; 3]);
        assert!((payload.center_of_gravity_m[2] - 0.3).abs() < 1e-15);
        assert!((payload.center_of_gravity_m[0] - 0.05).abs() < 1e-15);
    }

    #[test]
    fn mass_is_not_converted_because_density_is_already_kg_per_mm3() {
        let (native, [mass_kg, _, _, _]) = block(600.0);
        let payload = MassPropertiesV1::from_document(native, SourceUnits::Millimeters, [0.0; 3]);
        assert!((payload.mass_kg - mass_kg).abs() < 1e-12);
    }

    #[test]
    fn volume_scales_cubically() {
        let (native, [_, volume_mm3, _, _]) = block(600.0);
        let payload = MassPropertiesV1::from_document(native, SourceUnits::Millimeters, [0.0; 3]);
        assert!((payload.volume_m3 - volume_mm3 / 1.0e9).abs() / (volume_mm3 / 1.0e9) < 1e-12);
    }

    #[test]
    fn inches_and_feet_use_their_real_factors() {
        assert!((SourceUnits::Inches.metres_per_unit() - 0.0254).abs() < 1e-15);
        assert!((SourceUnits::Feet.metres_per_unit() - 0.3048).abs() < 1e-15);

        let native = DocumentMassProperties {
            volume: 1.0,
            mass_kg: 1.0,
            center_of_mass: [1.0, 0.0, 0.0],
            inertia: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
        };
        let inches = MassPropertiesV1::from_document(native, SourceUnits::Inches, [0.0; 3]);
        assert!((inches.center_of_gravity_m[0] - 0.0254).abs() < 1e-15);
        // 0.0254^2 = 6.4516e-4, not 1e-4 as a "divide by 100 twice" slip would give.
        assert!((inches.inertia_kg_m2[0][0] - 0.000_645_16).abs() < 1e-18);
    }

    #[test]
    fn datum_offset_shifts_the_centre_of_gravity_only() {
        let (native, _) = block(600.0);
        let inertia_before = native.inertia;
        let payload =
            MassPropertiesV1::from_document(native, SourceUnits::Millimeters, [0.0, 0.0, -0.3]);

        // CG moves by exactly the declared offset...
        let cg = payload.center_of_gravity_in_datum();
        assert!((cg[2] - 0.0).abs() < 1e-15, "cg z was {}", cg[2]);
        assert!((payload.center_of_gravity_m[2] - 0.3).abs() < 1e-15);

        // ...and the inertia is untouched, because it is about the CG.
        for (row, before_row) in inertia_before.iter().enumerate() {
            for (col, before) in before_row.iter().enumerate() {
                let expected = before / 1.0e6;
                assert!((payload.inertia_kg_m2[row][col] - expected).abs() < 1e-18);
            }
        }
    }

    #[test]
    fn a_zero_offset_means_the_origins_coincide() {
        let (native, _) = block(600.0);
        let payload = MassPropertiesV1::from_document(native, SourceUnits::Millimeters, [0.0; 3]);
        assert_eq!(
            payload.center_of_gravity_in_datum(),
            payload.center_of_gravity_m
        );
    }

    #[test]
    fn json_round_trips_losslessly() {
        let (native, _) = block(600.0);
        let payload =
            MassPropertiesV1::from_document(native, SourceUnits::Millimeters, [0.0, 0.0, -0.3]);
        let text = payload.to_json().unwrap();
        let back = MassPropertiesV1::from_json(&text).unwrap();
        assert_eq!(payload, back);
    }

    #[test]
    fn datum_offset_defaults_when_absent_from_json() {
        // A payload written before the field existed must still parse, and must mean
        // "origins coincide" rather than failing.
        let json = r#"{
            "schema": "apro-cad/mass-properties-si-v1",
            "units": "SI",
            "mass_kg": 1.0,
            "center_of_gravity_m": [0.0, 0.0, 0.3],
            "inertia_kg_m2": [[1.0,0.0,0.0],[0.0,1.0,0.0],[0.0,0.0,1.0]],
            "volume_m3": 0.001,
            "source_units": "millimeters",
            "source_note": "test"
        }"#;
        let payload = MassPropertiesV1::from_json(json).unwrap();
        assert_eq!(payload.datum_offset_m, [0.0; 3]);
    }

    #[test]
    fn validate_refuses_a_payload_that_is_not_si() {
        let (native, _) = block(600.0);
        let mut payload =
            MassPropertiesV1::from_document(native, SourceUnits::Millimeters, [0.0; 3]);
        payload.units = "mm".into();
        match payload.validate() {
            Err(ContractError::NotSi { units }) => assert_eq!(units, "mm"),
            other => panic!("expected NotSi, got {other:?}"),
        }
    }

    #[test]
    fn validate_refuses_a_transposed_inertia_block() {
        let (native, _) = block(600.0);
        let mut payload =
            MassPropertiesV1::from_document(native, SourceUnits::Millimeters, [0.0; 3]);
        // Give the tensor a genuinely asymmetric off-diagonal pair.
        payload.inertia_kg_m2[0][1] = 1.0;
        payload.inertia_kg_m2[1][0] = 0.0;
        assert!(matches!(
            payload.validate(),
            Err(ContractError::AsymmetricInertia { .. })
        ));
    }

    #[test]
    fn validate_accepts_a_well_formed_payload() {
        let (native, _) = block(600.0);
        let payload =
            MassPropertiesV1::from_document(native, SourceUnits::Millimeters, [0.0, 0.0, -0.3]);
        payload.validate().unwrap();
    }

    #[test]
    fn from_json_checked_rejects_bad_units_on_the_wire() {
        let json = r#"{
            "schema": "apro-cad/mass-properties-si-v1",
            "units": "mm",
            "mass_kg": 1.0,
            "center_of_gravity_m": [0.0, 0.0, 0.0],
            "inertia_kg_m2": [[1.0,0.0,0.0],[0.0,1.0,0.0],[0.0,0.0,1.0]],
            "volume_m3": 0.001,
            "source_units": "millimeters",
            "source_note": "test"
        }"#;
        assert!(matches!(
            MassPropertiesV1::from_json_checked(json),
            Err(ContractError::NotSi { .. })
        ));
    }

    #[test]
    fn source_units_parse_from_both_forms() {
        assert_eq!(
            "mm".parse::<SourceUnits>().unwrap(),
            SourceUnits::Millimeters
        );
        assert_eq!(
            "Millimeters".parse::<SourceUnits>().unwrap(),
            SourceUnits::Millimeters
        );
        assert_eq!("in".parse::<SourceUnits>().unwrap(), SourceUnits::Inches);
        assert_eq!("feet".parse::<SourceUnits>().unwrap(), SourceUnits::Feet);
        assert!("furlongs".parse::<SourceUnits>().is_err());
    }

    #[test]
    fn reference_geometry_from_diameter_is_the_circle() {
        let reference = ReferenceGeometrySi::from_body_diameter(0.1, 0.6);
        // pi * 0.05^2 = 0.007853981...
        assert!((reference.reference_area_m2 - 0.007_853_981_633_974_483).abs() < 1e-15);
        assert_eq!(reference.body_diameter_m, 0.1);
        assert!(!reference.convention.is_empty());
        reference.validate().unwrap();
    }

    #[test]
    fn reference_geometry_rejects_a_zero_area() {
        let reference = ReferenceGeometrySi {
            reference_area_m2: 0.0,
            reference_length_m: 0.6,
            body_diameter_m: 0.1,
            convention: "test".into(),
        };
        assert!(reference.validate().is_err());
    }

    /// The bounding-box derivation is a stated convention, so hold it to the one it
    /// states: length along the long axis, diameter from the *smaller* transverse extent.
    #[test]
    fn reference_geometry_from_a_bounding_box_uses_the_smaller_transverse_extent() {
        // 0.2 x 0.15 x 0.6 m, long axis Z.
        let reference = ReferenceGeometrySi::from_bounding_box([0.2, 0.15, 0.6], 2);
        assert!((reference.reference_length_m - 0.6).abs() < 1e-15);
        // The smaller transverse extent is 0.15, not 0.2. An asymmetric body must not be
        // credited with a wider reference area than it has.
        assert!((reference.body_diameter_m - 0.15).abs() < 1e-15);
        let expected_area = std::f64::consts::PI * 0.075 * 0.075;
        assert!((reference.reference_area_m2 - expected_area).abs() < 1e-15);
        assert!(reference.convention.contains("axis 2"));
        reference.validate().unwrap();
    }

    #[test]
    fn reference_geometry_travels_inside_the_payload() {
        let (native, _) = block(600.0);
        let reference = ReferenceGeometrySi::from_body_diameter(0.1, 0.6);
        let payload = MassPropertiesV1::from_document(native, SourceUnits::Millimeters, [0.0; 3])
            .with_reference_geometry(reference.clone());

        assert_eq!(payload.reference_geometry.as_ref(), Some(&reference));

        let text = payload.to_json().unwrap();
        let back = MassPropertiesV1::from_json_checked(&text).unwrap();
        assert_eq!(back.reference_geometry, Some(reference));
    }

    /// A payload from a producer that cannot state its aerodynamic convention must still
    /// parse, and must say `None` rather than inventing a number.
    #[test]
    fn reference_geometry_is_optional_and_absent_by_default() {
        let (native, _) = block(600.0);
        let payload = MassPropertiesV1::from_document(native, SourceUnits::Millimeters, [0.0; 3]);
        assert!(payload.reference_geometry.is_none());

        let text = payload.to_json().unwrap();
        assert!(
            !text.contains("reference_geometry"),
            "an unset field should not be serialised: {text}"
        );
        assert!(MassPropertiesV1::from_json_checked(&text)
            .unwrap()
            .reference_geometry
            .is_none());
    }
}
