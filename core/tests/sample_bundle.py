"""Locate the real GF2 sample bundle used by the contract tests.

The old development cache path is kept for machines that still have it, but
the installed game is the normal source now. `EXMT_GF2_BUNDLE` wins so CI can
point at a copy.
"""

from __future__ import annotations

import os

_INSTALLED = (
    r"D:\Program Files\GF2Exilium\GF2 Game\GF2_Exilium_Data\LocalCache\Data"
    r"\AssetBundles_Windows\872f1c4f93f07d6ba65b103ae5d1a337.bundle"
)
_LEGACY = r"C:\DevCache\gf2\backup\872f1c4f93f07d6ba65b103ae5d1a337.bundle.orig"


def find() -> str:
    for candidate in (os.environ.get("EXMT_GF2_BUNDLE"), _INSTALLED, _LEGACY):
        if candidate and os.path.isfile(candidate):
            return candidate
    return ""


SAMPLE = find()
