# PlasmaTop

PlasmaTop gathers system metrics and presents them through a Plasma panel and tooltip. This glossary distinguishes metric sampling from display publication so freshness and coherence have precise meanings.

## Language

**Metric sample**:
A reading for one metric at a specific capture time. Samples for different metrics need not share a capture time.
_Avoid_: Poll result, collection-cycle value

**Sample age**:
The elapsed monotonic time since a metric sample was captured.
_Avoid_: Cache age

**Freshness budget**:
The maximum intended sample age before a new sampling attempt becomes due. Exceeding the budget does not by itself remove the latest sample from a display snapshot.
_Avoid_: Cache TTL, poll interval

**Demand set**:
The metrics currently required by the panel, enabled notifications, configured graph history, or a presented tooltip and its selected page. Metrics outside the demand set do not need fresh samples.
_Avoid_: Enabled metrics, configured metrics

**Hardware inventory**:
The latest known set of hardware that can provide PlasmaTop metrics. The inventory may change as devices and services appear or disappear.
_Avoid_: Hardware snapshot

**Display snapshot**:
The latest completed metric samples selected together for one display publication. A display snapshot may carry an older sample when no newer sample completed before its publication deadline.
_Avoid_: Collection snapshot, coherent sample

**Selected page**:
The tooltip page chosen for presentation, whether or not the tooltip is currently visible.
_Avoid_: Active page

**Presented tooltip**:
A tooltip surface currently visible because it is hovered, pinned, or shown as a planar/full representation. Page-owned metric samples are demanded only while the tooltip is presented.
_Avoid_: Active tooltip, open tooltip
