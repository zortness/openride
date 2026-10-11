import importlib.util
from pathlib import Path
import tempfile
import unittest
import zipfile

spec = importlib.util.spec_from_file_location("acquire_osm2world",Path(__file__).parents[1]/"acquire-osm2world.py")
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)


class PortableToolsTests(unittest.TestCase):
    def test_extracts_nested_files_without_overwriting_existing_directory(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            archive = root/"source.zip"
            with zipfile.ZipFile(archive,"w") as zipped:
                zipped.writestr("tool/",b"")
                zipped.writestr("tool/license.txt",b"fixture")
            output = root/"output"
            module.extract(archive,output)
            self.assertEqual((output/"tool/license.txt").read_bytes(),b"fixture")
            with self.assertRaises(FileExistsError):
                module.extract(archive,output)

    def test_rejects_escaping_and_ambiguous_windows_paths_before_extracting(self):
        for name in ("../outside", "C:/outside", "/outside", "tool/file:stream", "tool/NUL", "tool/file. "):
            with self.subTest(name=name), tempfile.TemporaryDirectory() as folder:
                root = Path(folder)
                with zipfile.ZipFile(root/"source.zip","w") as zipped:
                    zipped.writestr(name,b"fixture")
                with self.assertRaises(ValueError):
                    module.extract(root/"source.zip",root/"output")
                self.assertFalse((root/"output").exists())

    def test_rejects_duplicate_case_paths_and_symlinks(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            with zipfile.ZipFile(root/"duplicate.zip","w") as zipped:
                zipped.writestr("Tool.txt",b"a")
                zipped.writestr("tool.txt",b"b")
            with self.assertRaises(ValueError):
                module.extract(root/"duplicate.zip",root/"output")
            with zipfile.ZipFile(root/"symlink.zip","w") as zipped:
                info = zipfile.ZipInfo("link")
                info.external_attr = 0o120777 << 16
                zipped.writestr(info,b"../outside")
            with self.assertRaises(ValueError):
                module.extract(root/"symlink.zip",root/"output")


if __name__ == "__main__":
    unittest.main()
