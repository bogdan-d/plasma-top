//! Typed applet requests and the publication sink, independent of the bus transport.
//!
//! The bus adapter validates raw method arguments into these types, so the daemon only ever sees well-formed requests.

use std::fmt::{self, Display, Formatter};
use std::num::NonZeroU64;

/// Largest accepted usable panel width, in pixels.
pub const MAX_USABLE_PX: f64 = 16_384.0;
/// Largest accepted glyph advance, in pixels.
pub const MAX_GLYPH_ADVANCE: f64 = 512.0;

/// Bus connection that sent a request: its D-Bus unique name.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ClientId(String);

impl ClientId {
    /// Wraps a sender's unique bus name.
    #[must_use]
    pub fn new(unique_name: impl Into<String>) -> Self {
        Self(unique_name.into())
    }

    /// Returns the unique bus name.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Display for ClientId {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// Positive Plasma applet instance identifier, unique within one client.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct InstanceId(NonZeroU64);

impl InstanceId {
    /// The smallest identifier.
    pub const MIN: Self = Self(NonZeroU64::MIN);

    /// Accepts only positive identifiers.
    #[must_use]
    pub fn new(raw: u64) -> Option<Self> {
        NonZeroU64::new(raw).map(Self)
    }

    /// Returns the raw identifier.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0.get()
    }
}

/// One wheel notch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PageStep {
    /// Advance one page, wrapping to the first.
    Next,
    /// Go back one page, wrapping to the last.
    Prev,
}

impl PageStep {
    /// Accepts only `+1` and `-1`.
    #[must_use]
    pub const fn from_delta(delta: i32) -> Option<Self> {
        match delta {
            1 => Some(Self::Next),
            -1 => Some(Self::Prev),
            _ => None,
        }
    }

    /// Applies the step to `page`, wrapping against `count`. Counts of at most one never move.
    #[must_use]
    pub const fn apply(self, page: u32, count: u32) -> u32 {
        if count <= 1 {
            return 0;
        }
        match self {
            Self::Next => (page % count + 1) % count,
            Self::Prev => (page % count + count - 1) % count,
        }
    }
}

/// Applet-measured panel and tooltip metrics, validated against fixed bounds.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AppletGeometry {
    usable_px: f64,
    glyph_advance: f64,
    vertical: bool,
    tooltip_advance: f64,
}

impl AppletGeometry {
    /// Accepts `usable_px` in `(0, 16384]` and both advances in `(0, 512]`; NaN and infinity fail the range checks.
    #[must_use]
    pub fn new(
        usable_px: f64,
        glyph_advance: f64,
        vertical: bool,
        tooltip_advance: f64,
    ) -> Option<Self> {
        let in_range = |value: f64, max: f64| value > 0.0 && value <= max;
        (in_range(usable_px, MAX_USABLE_PX)
            && in_range(glyph_advance, MAX_GLYPH_ADVANCE)
            && in_range(tooltip_advance, MAX_GLYPH_ADVANCE))
        .then_some(Self {
            usable_px,
            glyph_advance,
            vertical,
            tooltip_advance,
        })
    }

    /// Usable px of the panel's text area.
    #[must_use]
    pub const fn usable_px(&self) -> f64 {
        self.usable_px
    }

    /// On-screen advance of one main-font monospace glyph.
    #[must_use]
    pub const fn glyph_advance(&self) -> f64 {
        self.glyph_advance
    }

    /// Whether the applet sits on a vertical panel.
    #[must_use]
    pub const fn vertical(&self) -> bool {
        self.vertical
    }

    /// On-screen advance of one tooltip-font monospace glyph.
    #[must_use]
    pub const fn tooltip_advance(&self) -> f64 {
        self.tooltip_advance
    }
}

/// Validated request from an applet client, delivered to the daemon loop in receive order.
#[derive(Debug, Clone, PartialEq)]
pub enum AppletRequest {
    /// Turn the tooltip one page.
    StepPage(PageStep),
    /// Create or refresh one presentation lease.
    Present {
        /// Sending connection.
        client: ClientId,
        /// Applet instance within that connection.
        instance: InstanceId,
    },
    /// Release one presentation lease.
    Dismiss {
        /// Sending connection.
        client: ClientId,
        /// Applet instance within that connection.
        instance: InstanceId,
    },
    /// A client connection left the bus; all its leases end.
    ClientGone {
        /// Departed connection.
        client: ClientId,
    },
    /// Replace the measured panel geometry.
    Geometry(AppletGeometry),
}

/// Input from the applet service, in receive order.
#[derive(Debug, Clone, PartialEq)]
pub enum AppletEvent {
    /// A validated client request.
    Request(AppletRequest),
    /// The daemon's own bus connection dropped; every lease ends.
    ConnectionLost,
    /// The service cannot continue, for example because another daemon took the name.
    ServiceFailed(String),
}

/// Destination for published display state.
///
/// Callers announce only real changes: HTML when its bytes differ and the selection when either value differs.
pub trait PublicationSink: Send {
    /// Publishes new panel HTML.
    fn panel(&mut self, html: &str);
    /// Publishes new tooltip HTML.
    fn tooltip(&mut self, html: &str);
    /// Publishes the selected page and page count together, with `page < count`.
    fn selection(&mut self, page: u32, count: u32);
}

/// Sink that drops every publication, for runs with no applet.
#[derive(Debug, Default, Clone, Copy)]
pub struct DiscardSink;

impl PublicationSink for DiscardSink {
    fn panel(&mut self, _html: &str) {}
    fn tooltip(&mut self, _html: &str) {}
    fn selection(&mut self, _page: u32, _count: u32) {}
}

#[cfg(test)]
mod tests;
