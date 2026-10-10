"""The diagnostic must not certify declared IDs without matching observations."""
import importlib.util
import os
from pathlib import Path
import unittest
from unittest.mock import patch

_path = Path(__file__).resolve().parents[2] / 'scripts/rust-migration/prove_dual_site_candidate.py'
_spec = importlib.util.spec_from_file_location('dual_site_proof', _path)
proof = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(proof)


class IdentityTests(unittest.TestCase):
    def test_observation_is_required(self):
        for observed in ('default', 'taixu', None, ''):
            with self.subTest(observed=observed), self.assertRaises(AssertionError):
                proof.require_identity(observed, 'siwen')

    def test_default_cannot_be_certified_as_a_site(self):
        for expected in ('default', '', None):
            with self.subTest(expected=expected), self.assertRaises(AssertionError):
                proof.require_identity(expected, expected)

    def test_matching_named_sites(self):
        for site in ('siwen', 'taixu'):
            proof.require_identity(site, site)

    def test_absent_environment_restored_after_exception(self):
        with patch.dict(os.environ, clear=True):
            with self.assertRaisesRegex(RuntimeError, 'probe failed'):
                with proof.library_scope('siwen'):
                    self.assertEqual(os.environ['LLMBASE_LIBRARY_ID'], 'siwen')
                    raise RuntimeError('probe failed')
            self.assertNotIn('LLMBASE_LIBRARY_ID', os.environ)

    def test_nested_environment_restores_present_value(self):
        with patch.dict(os.environ, {'LLMBASE_LIBRARY_ID':'original'}):
            with proof.library_scope('siwen'):
                with self.assertRaises(ValueError):
                    with proof.library_scope('taixu'):
                        self.assertEqual(os.environ['LLMBASE_LIBRARY_ID'], 'taixu')
                        raise ValueError()
                self.assertEqual(os.environ['LLMBASE_LIBRARY_ID'], 'siwen')
            self.assertEqual(os.environ['LLMBASE_LIBRARY_ID'], 'original')

    def test_absent_environment_restored_on_success(self):
        with patch.dict(os.environ, clear=True):
            with proof.library_scope('taixu'):
                self.assertEqual(os.environ['LLMBASE_LIBRARY_ID'], 'taixu')
            self.assertNotIn('LLMBASE_LIBRARY_ID', os.environ)


if __name__ == '__main__':
    unittest.main()
