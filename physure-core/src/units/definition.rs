/// Native unit definition for physure-core.
///
/// `UnitDefinition` is an immutable, interned record describing a single
/// named unit: its symbol, physical dimension, conversion strategy, and
/// optional metadata (human-readable name, kind, allow-prefixes flag).
use crate::units::converters::UnitConverter;
use crate::units::dimension::DimVector;

/// Distinguishes absolute units (e.g., Kelvin, Celsius) from delta units.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum UnitKind {
    Delta,
    Point, // absolute origin (e.g. °C, °F)
}

impl UnitKind {
    pub fn from_str(s: &str) -> Self {
        match s {
            "point" | "absolute" => UnitKind::Point,
            _ => UnitKind::Delta,
        }
    }
}

/// A complete, immutable description of a single physical unit.
#[derive(Debug, Clone)]
pub struct UnitDefinition {
    pub symbol: String,
    pub dimension: DimVector,
    pub converter: UnitConverter,
    pub name: Option<String>,
    pub kind: UnitKind,
    pub allow_prefixes: bool,
}

impl UnitDefinition {
    pub fn new(
        symbol: impl Into<String>,
        dimension: DimVector,
        converter: UnitConverter,
    ) -> Self {
        UnitDefinition {
            symbol: symbol.into(),
            dimension,
            converter,
            name: None,
            kind: UnitKind::Delta,
            allow_prefixes: true,
        }
    }

    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    pub fn with_kind(mut self, kind: UnitKind) -> Self {
        self.kind = kind;
        self
    }

    pub fn with_allow_prefixes(mut self, allow: bool) -> Self {
        self.allow_prefixes = allow;
        self
    }

    /// Returns the linear scale if this is a LinearConverter, else None.
    pub fn scale(&self) -> Option<f64> {
        match &self.converter {
            UnitConverter::Linear { scale } => Some(*scale),
            _ => None,
        }
    }

    /// Returns the offset (b in y = ax + b) for OffsetConverter, else 0.
    pub fn offset(&self) -> f64 {
        match &self.converter {
            UnitConverter::Offset { offset, .. } => *offset,
            _ => 0.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::units::dimension::DimVector;

    #[test]
    fn test_unit_definition_linear() {
        let dim = DimVector::from_pairs([("L", 1)]).unwrap();
        let def = UnitDefinition::new("m", dim, UnitConverter::linear(1.0))
            .with_name("meter")
            .with_allow_prefixes(true);

        assert_eq!(def.symbol, "m");
        assert_eq!(def.scale(), Some(1.0));
        assert_eq!(def.name.as_deref(), Some("meter"));
    }

    #[test]
    fn test_unit_definition_offset() {
        let dim = DimVector::from_pairs([("O", 1)]).unwrap();
        let def = UnitDefinition::new("degC", dim, UnitConverter::offset(1.0, 273.15))
            .with_kind(UnitKind::Point)
            .with_allow_prefixes(false);

        assert_eq!(def.kind, UnitKind::Point);
        assert!(!def.allow_prefixes);
        assert_eq!(def.offset(), 273.15);
    }
}
