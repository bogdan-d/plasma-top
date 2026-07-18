//! Validated `metric[:form]` item tokens.

use std::fmt::{self, Display, Formatter};
use std::str::FromStr;

use crate::domain::form::{Form, FormParseError, Shape, SurfaceSet};
use crate::domain::metric::{Metric, MetricParseError};

/// A validated item token that cannot contain an impossible metric/form pairing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ItemToken {
    metric: Metric,
    rendering: ItemRendering,
}

/// The resolved rendering mode for a validated item token.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ItemRendering {
    /// A generic form selected from the config menu.
    Generic(Form),
    /// An intrinsic shape owned by the metric itself.
    Intrinsic(Shape),
}

/// Errors returned while parsing an item token.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ItemParseError {
    /// The token was empty.
    Empty,
    /// The metric part was unknown.
    UnknownMetric(MetricParseError),
    /// The form part was unknown.
    UnknownForm(FormParseError),
    /// A generic form was supplied to a metric with an intrinsic shape.
    IntrinsicMetricWithForm {
        /// The intrinsic metric.
        metric: Metric,
        /// The rejected form token.
        form: String,
    },
    /// A known form was rejected by the chosen metric.
    UnsupportedForm {
        /// The metric that rejected the form.
        metric: Metric,
        /// The rejected form.
        form: Form,
    },
}

impl ItemToken {
    /// Creates a validated token from its parts.
    ///
    /// # Errors
    ///
    /// Returns [`ItemParseError::IntrinsicMetricWithForm`] or
    /// [`ItemParseError::UnsupportedForm`] when the metric/form pairing is not
    /// part of the frozen contract.
    pub fn new(metric: Metric, form: Option<Form>) -> Result<Self, ItemParseError> {
        if let Some(shape) = metric.intrinsic_shape() {
            if let Some(form) = form {
                return Err(ItemParseError::IntrinsicMetricWithForm {
                    metric,
                    form: form.to_string(),
                });
            }

            return Ok(Self {
                metric,
                rendering: ItemRendering::Intrinsic(shape),
            });
        }

        let form = form.unwrap_or_default();
        if !metric.supports_form(form) {
            return Err(ItemParseError::UnsupportedForm { metric, form });
        }

        Ok(Self {
            metric,
            rendering: ItemRendering::Generic(form),
        })
    }

    /// Returns the token's metric.
    #[must_use]
    pub const fn metric(self) -> Metric {
        self.metric
    }

    /// Returns the token's resolved rendering mode.
    #[must_use]
    pub const fn rendering(self) -> ItemRendering {
        self.rendering
    }

    /// Returns the explicit generic form when the metric uses one.
    #[must_use]
    pub const fn form(self) -> Option<Form> {
        match self.rendering {
            ItemRendering::Generic(form) => Some(form),
            ItemRendering::Intrinsic(_) => None,
        }
    }

    /// Returns the effective admitted surfaces for this exact token.
    #[must_use]
    pub fn effective_surfaces(self) -> SurfaceSet {
        match self.rendering {
            ItemRendering::Generic(form) => {
                self.metric.surfaces().intersection(form.allowed_surfaces())
            }
            ItemRendering::Intrinsic(_) => self.metric.surfaces(),
        }
    }
}

impl Display for ItemToken {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self.rendering {
            ItemRendering::Generic(Form::Value) | ItemRendering::Intrinsic(_) => {
                write!(formatter, "{}", self.metric)
            }
            ItemRendering::Generic(form) => write!(formatter, "{}:{form}", self.metric),
        }
    }
}

impl Display for ItemParseError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => formatter.write_str("item token is empty"),
            Self::UnknownMetric(error) => write!(formatter, "{error}"),
            Self::UnknownForm(error) => write!(formatter, "{error}"),
            Self::IntrinsicMetricWithForm { metric, form } => {
                write!(formatter, "metric `{metric}` does not accept form `{form}`")
            }
            Self::UnsupportedForm { metric, form } => {
                write!(
                    formatter,
                    "metric `{metric}` does not support form `{form}`"
                )
            }
        }
    }
}

impl std::error::Error for ItemParseError {}

impl FromStr for ItemToken {
    type Err = ItemParseError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        if value.is_empty() {
            return Err(ItemParseError::Empty);
        }

        let (metric_text, form_text) = value
            .split_once(':')
            .map_or((value, None), |(metric, form)| (metric, Some(form)));
        let metric = metric_text
            .parse::<Metric>()
            .map_err(ItemParseError::UnknownMetric)?;
        let form = match form_text {
            Some(text) => Some(text.parse::<Form>().map_err(ItemParseError::UnknownForm)?),
            None => None,
        };

        Self::new(metric, form)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::form::Surface;

    #[test]
    fn bare_metric_defaults_to_value_form() {
        let token = "cpu_usage".parse::<ItemToken>();

        assert_eq!(
            token,
            Ok(ItemToken {
                metric: Metric::CpuUsage,
                rendering: ItemRendering::Generic(Form::Value),
            })
        );
        assert_eq!(
            token.map(|item| item.to_string()),
            Ok(String::from("cpu_usage"))
        );
    }

    #[test]
    fn explicit_generic_form_round_trips() {
        let token = "cpu_usage:spark_value".parse::<ItemToken>();

        assert_eq!(
            token.map(|item| item.to_string()),
            Ok(String::from("cpu_usage:spark_value"))
        );
    }

    #[test]
    fn intrinsic_metric_rejects_forms() {
        let token = "net_speed:value".parse::<ItemToken>();

        assert_eq!(
            token,
            Err(ItemParseError::IntrinsicMetricWithForm {
                metric: Metric::NetSpeed,
                form: String::from("value"),
            })
        );
    }

    #[test]
    fn unsupported_form_is_rejected() {
        let token = "cpu_temp:bar".parse::<ItemToken>();

        assert_eq!(
            token,
            Err(ItemParseError::UnsupportedForm {
                metric: Metric::CpuTemp,
                form: Form::Bar,
            })
        );
    }

    #[test]
    fn effective_surfaces_follow_metric_and_form_rules() {
        let panel_token = match "cpu_usage:bar".parse::<ItemToken>() {
            Ok(token) => token,
            Err(error) => panic!("unexpected parse error: {error}"),
        };
        let tooltip_token = match "uptime".parse::<ItemToken>() {
            Ok(token) => token,
            Err(error) => panic!("unexpected parse error: {error}"),
        };

        assert!(
            panel_token
                .effective_surfaces()
                .contains(Surface::PanelHorizontal)
        );
        assert!(!panel_token.effective_surfaces().contains(Surface::Tooltip));
        assert!(
            tooltip_token
                .effective_surfaces()
                .contains(Surface::Tooltip)
        );
    }
}
