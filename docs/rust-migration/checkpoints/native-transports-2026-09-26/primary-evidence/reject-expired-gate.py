"""Negative verifier check: a delayed forwarded delta cannot pass an expired gate."""
import importlib.util
import json
from pathlib import Path
import sys

root = Path(__file__).parent
spec = importlib.util.spec_from_file_location("standalone", root / "verify-standalone.py")
proof = importlib.util.module_from_spec(spec)
spec.loader.exec_module(proof)
original = proof.next_sse_frame
proof.GATE_TIMEOUT = 0.05


def delayed_delta(*args):
    result = original(*args)
    frame = result[0]
    if frame is not None and frame[0] == "delta":
        assert proof.GATE_EXPIRED.wait(5), "fault injection did not expire the gate"
    return result


proof.next_sse_frame = delayed_delta
sys.argv = [str(root / "verify-standalone.py"), "--binary", str(root / "llmbase-cli-bin"),
            "--run-root", str(root / "expired-gate"), "--chroot"]
try:
    proof.main()
except AssertionError as error:
    assert str(error) == "delta did not arrive while provider gate was held", str(error)
    assert proof.GATE_EXPIRED.is_set()
    assert not (root / "expired-gate/receipt.json").exists()
    cleanup = json.loads((root / "expired-gate/http-transport.json").read_text())
    assert cleanup["exit"] == 0 and not cleanup["shutdown"]["forced"]
    print("PASS: delayed delta after expired provider gate rejected; no success receipt; HTTP clean exit 0")
else:
    raise AssertionError("verifier accepted a delta after provider gate expiry")
