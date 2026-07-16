"""The unit letters readings are shown with.

A leaf module (imports nothing) because the three consumers sit at different
levels and must agree: the cell factories (items.py), the formatter's own rows,
and the notifications. sensors.py is where the unit is actually decided, but it
can't host this — it imports registry, which imports items, so items importing
sensors back would close a cycle.
"""
from __future__ import annotations

# Temperature. Not a scale knob: every source is Celsius (hwmon reports
# millidegrees, sensors._read_path_millideg divides by 1000) and so are the
# [thresholds], so this only ever names what the readings already are.
# Fahrenheit would mean converting at the sensor AND in the thresholds — a real
# feature, not a suffix.
TEMP_SCALE = "C"
