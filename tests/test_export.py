"""Die exportierten Daten für die Rust-Engine müssen zur Python-Sim passen."""
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent


def test_sim_export_is_up_to_date():
    r = subprocess.run([sys.executable, str(ROOT / "tools" / "export_sim_data.py"), "--check"],
                       capture_output=True, text=True)
    assert r.returncode == 0, r.stdout + r.stderr
