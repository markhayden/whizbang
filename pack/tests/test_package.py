import sys
import unittest


class PackageTest(unittest.TestCase):
    def test_the_package_imports_from_the_pack_and_runs_on_the_pinned_python(self):
        import whizbang_pack

        self.assertIn("pack", whizbang_pack.__file__.replace("\\", "/").split("/"))
        self.assertEqual(sys.version_info[:2], (3, 12))


if __name__ == "__main__":
    unittest.main()
