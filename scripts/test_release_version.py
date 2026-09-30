"""Read-only tests for release-version preflight."""
from pathlib import Path
import tempfile
import unittest

from release_version import verify_release_version


class ReleaseVersionTests(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.manifest = Path(self.directory.name) / "Cargo.toml"
        self.manifest.write_text(
            '[package]\nname = "fixture"\nversion = "13.0.0"\n'
            '[dependencies]\nother = { version = "12.0.2" }\n',
            encoding="utf-8",
        )

    def test_matching_root_version_is_accepted(self):
        verify_release_version("13.0.0", self.manifest)

    def test_dependency_version_is_not_the_release_version(self):
        with self.assertRaisesRegex(ValueError, "differs from package"):
            verify_release_version("12.0.2", self.manifest)

    def test_malformed_versions_are_rejected(self):
        for version in ["13x0x0", "013.0.0", "13.0.0\n", "13.0.0; echo x", "$(echo x)", "13.0.0-beta"]:
            with self.subTest(version=version), self.assertRaises(ValueError):
                verify_release_version(version, self.manifest)


if __name__ == "__main__":
    unittest.main()
