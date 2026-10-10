"""Negative controls for the C2 verifier, without a native build or Siwen import."""
import importlib.util
import json
from pathlib import Path
import sys
import unittest

_scripts = Path(__file__).resolve().parents[2] / "scripts/rust-migration"
sys.path.insert(0, str(_scripts))
try:
    spec = importlib.util.spec_from_file_location("privacy_proof", _scripts / "prove_site_privacy_evidence.py")
    proof = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(proof)
finally:
    sys.path.pop(0)


class Response:
    def __init__(self, status, data):
        self.status_code, self.data = status, data

    def get_json(self, silent=False):
        return self.data

    def get_data(self, as_text=False):
        return json.dumps(self.data)


class PrivacyProofTests(unittest.TestCase):
    def test_leaked_unicode_in_json_is_not_hidden_by_escaping(self):
        result = proof.check_response(Response(200, {"body": "公開 私藏𠀀"}),
                                      public_marker="公開", forbidden=["私藏𠀀"])
        self.assertEqual(result["status"], "failed")
        self.assertEqual(result["leaked_markers"], ["私藏𠀀"])

    def test_empty_or_error_response_cannot_pass_positive_control(self):
        for status in (200, 401, 403, 404, 500):
            with self.subTest(status=status):
                self.assertEqual(proof.check_response(Response(status, {}),
                    public_marker="visible")["status"], "failed")

    def test_only_explicit_denial_passes_private_detail(self):
        for status in (200, 302, 401, 403, 404, 500):
            with self.subTest(status=status):
                result = proof.check_response(Response(status, {}), detail=True)
                self.assertEqual(result["status"], "passed" if status in (401, 403, 404) else "failed")

    def test_denial_body_must_not_leak(self):
        self.assertEqual(proof.check_response(Response(403, {"error": "secret"}),
            forbidden=["secret"], detail=True)["status"], "failed")

    def test_wrong_failure_is_not_a_valid_evidence_denial(self):
        def unrelated():
            raise RuntimeError("database unavailable")
        with self.assertRaises(AssertionError):
            proof.require_rejection(unrelated, RuntimeError, "citation does not match")
        with self.assertRaises(AssertionError):
            proof.require_rejection(lambda: {}, PermissionError, "scope denied")

    def test_correct_public_content_passes(self):
        self.assertEqual(proof.check_response(Response(200, {"body": "visible"}),
            public_marker="visible", forbidden=["secret"])["status"], "passed")

    def test_bake_validator_denial_cannot_be_ignored(self):
        result = proof.check_bake({"public.json": "visible"}, {"public.json": "visible"},
                                  ["secret"], {"accepted": False, "exit": 2})
        self.assertEqual(result["status"], "failed")

    def test_json_success_cannot_mask_missing_public_html(self):
        result = proof.check_bake({"public.json": "visible"},
                                  {"public.json": "visible", "public.html": "visible"},
                                  ["secret"], {"accepted": True})
        self.assertEqual(result["status"], "failed")
        self.assertFalse(result["public_output_controls"]["public.html"])

    def test_private_identifier_without_private_body_still_leaks(self):
        result = proof.check_response(Response(200, {"articles": ["public", "private-proof"]}),
                                      public_marker="public", forbidden=["private-proof"])
        self.assertEqual(result["status"], "failed")

    def test_private_artifact_filename_without_private_body_still_leaks(self):
        result = proof.check_bake({"public.json": "visible", "private-proof.json": "{}"},
                                  {"public.json": "visible"}, ["private-proof"], {"accepted": True})
        self.assertEqual(result["status"], "failed")


if __name__ == "__main__":
    unittest.main()
