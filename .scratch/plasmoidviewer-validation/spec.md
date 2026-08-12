# Plasmoidviewer validation

Status: needs-info

## Objective

Repair PlasmaTop's live validation scripts so they use KDE's `plasmoidviewer` reliably for horizontal, vertical, and planar applet representations.

The current integration is disabled until its launch, environment, interaction, and cleanup behavior can be reproduced and fixed. Routine automated gates remain independent of this effort, and panel behavior is not inferred from `plasmawindowed`.

## Implementation sequence

1. [Fix plasmoidviewer validation scripts](issues/01-fix-plasmoidviewer-validation.md).
