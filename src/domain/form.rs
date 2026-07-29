//! Form, shape, and surface contracts.

use std::fmt::{self, Display, Formatter};
use std::str::FromStr;

/// Intrinsic row shapes owned by specific metrics.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub enum Shape {
    /// Label + left-aligned auxiliary column + value.
    TripleL,
    /// Two adjacent key/value pairs on one row.
    Duo,
}

/// A concrete render surface.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub enum Surface {
    /// Tooltip HTML.
    Tooltip,
    /// Horizontal panel HTML.
    PanelHorizontal,
    /// Vertical panel HTML.
    PanelVertical,
}

/// A small bitset describing one or more admitted surfaces.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct SurfaceSet(u8);

/// Generic user-selectable rendering forms.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Ord, PartialOrd, Default)]
pub enum Form {
    /// Plain value rendering.
    #[default]
    Value,
    /// Orientation-adaptive bar/column rendering.
    Bar,
    /// Block sparkline rendering.
    Spark,
    /// Braille sparkline rendering.
    Braille,
    /// Sparkline plus numeric value.
    SparkValue,
    /// Braille sparkline plus numeric value.
    BrailleValue,
    /// Current bar plus history sparkline.
    BarSpark,
    /// Current bar plus history braille sparkline.
    BarBraille,
    /// Multi-instance pair rendering.
    Pair,
}

impl Shape {
    /// Returns the stable snake_case token used by the migration plan.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::TripleL => "triple_l",
            Self::Duo => "duo",
        }
    }
}

impl Surface {
    /// Returns the stable snake_case token used by the migration plan.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Tooltip => "tooltip",
            Self::PanelHorizontal => "panel_horizontal",
            Self::PanelVertical => "panel_vertical",
        }
    }
}

impl SurfaceSet {
    /// Empty surface set.
    pub const NONE: Self = Self(0);
    /// Tooltip-only surface set.
    pub const TOOLTIP: Self = Self(1 << 0);
    /// Horizontal-panel-only surface set.
    pub const PANEL_HORIZONTAL: Self = Self(1 << 1);
    /// Vertical-panel-only surface set.
    pub const PANEL_VERTICAL: Self = Self(1 << 2);
    /// Both panel orientations.
    pub const PANEL: Self = Self(Self::PANEL_HORIZONTAL.0 | Self::PANEL_VERTICAL.0);
    /// All supported surfaces.
    pub const ALL: Self = Self(Self::TOOLTIP.0 | Self::PANEL.0);

    /// Creates a singleton surface set.
    #[must_use]
    pub const fn from_surface(surface: Surface) -> Self {
        match surface {
            Surface::Tooltip => Self::TOOLTIP,
            Surface::PanelHorizontal => Self::PANEL_HORIZONTAL,
            Surface::PanelVertical => Self::PANEL_VERTICAL,
        }
    }

    /// Returns `true` when the set includes the given surface.
    #[must_use]
    pub const fn contains(self, surface: Surface) -> bool {
        (self.0 & Self::from_surface(surface).0) != 0
    }

    /// Returns the intersection of two surface sets.
    #[must_use]
    pub const fn intersection(self, other: Self) -> Self {
        Self(self.0 & other.0)
    }

    /// Returns `true` when the set is empty.
    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }
}

impl Form {
    /// Returns the stable snake_case config token.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Value => "value",
            Self::Bar => "bar",
            Self::Spark => "spark",
            Self::Braille => "braille",
            Self::SparkValue => "spark_value",
            Self::BrailleValue => "braille_value",
            Self::BarSpark => "bar_spark",
            Self::BarBraille => "bar_braille",
            Self::Pair => "pair",
        }
    }

    /// Returns the surfaces admitted by the generic form.
    #[must_use]
    pub const fn allowed_surfaces(self) -> SurfaceSet {
        match self {
            Self::Value => SurfaceSet::ALL,
            Self::Bar | Self::Spark | Self::Braille => SurfaceSet::PANEL,
            Self::SparkValue
            | Self::BrailleValue
            | Self::BarSpark
            | Self::BarBraille
            | Self::Pair => SurfaceSet::TOOLTIP,
        }
    }
}

impl Display for Shape {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl Display for Surface {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl Display for Form {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl FromStr for Form {
    type Err = FormParseError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "" | "value" => Ok(Self::Value),
            "bar" => Ok(Self::Bar),
            "spark" => Ok(Self::Spark),
            "braille" => Ok(Self::Braille),
            "spark_value" => Ok(Self::SparkValue),
            "braille_value" => Ok(Self::BrailleValue),
            "bar_spark" => Ok(Self::BarSpark),
            "bar_braille" => Ok(Self::BarBraille),
            "pair" => Ok(Self::Pair),
            _ => Err(FormParseError {
                value: value.to_owned(),
            }),
        }
    }
}

/// An invalid generic form token.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FormParseError {
    /// The rejected form token.
    pub value: String,
}

impl Display for FormParseError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        write!(formatter, "unknown form: {}", self.value)
    }
}

impl std::error::Error for FormParseError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn form_defaults_to_value() {
        assert_eq!(Form::default(), Form::Value);
        assert_eq!("".parse::<Form>(), Ok(Form::Value));
    }

    #[test]
    fn panel_forms_stay_out_of_tooltip() {
        assert!(!Form::Bar.allowed_surfaces().contains(Surface::Tooltip));
        assert!(
            Form::Bar
                .allowed_surfaces()
                .contains(Surface::PanelHorizontal)
        );
        assert!(
            Form::Spark
                .allowed_surfaces()
                .contains(Surface::PanelVertical)
        );
    }

    #[test]
    fn tooltip_only_forms_stay_out_of_panel() {
        let surfaces = Form::SparkValue.allowed_surfaces();

        assert!(surfaces.contains(Surface::Tooltip));
        assert!(!surfaces.contains(Surface::PanelHorizontal));
        assert!(!surfaces.contains(Surface::PanelVertical));
    }
}
