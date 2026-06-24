from __future__ import annotations

import sys
from pathlib import Path


SFT_DIR = Path(__file__).resolve().parents[1]
if str(SFT_DIR) not in sys.path:
    sys.path.insert(0, str(SFT_DIR))
