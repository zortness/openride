import hashlib
import importlib.util
import json
from pathlib import Path
import struct
import tempfile
import unittest
import urllib.error
from unittest.mock import patch, MagicMock

spec=importlib.util.spec_from_file_location('imagery',Path(__file__).parents[1]/'acquire-imagery.py')
imagery=importlib.util.module_from_spec(spec);spec.loader.exec_module(imagery)

class Imagery(unittest.TestCase):
    def test_timeout_retry_discards_partial_response_and_uses_configured_timeout(self):
        first=MagicMock();first.__enter__.return_value.read.side_effect=TimeoutError('read timed out')
        second=MagicMock();second.__enter__.return_value.read.return_value=b'complete image'
        with patch.object(imagery.urllib.request,'urlopen',side_effect=[first,second]) as opened, patch.object(imagery.time,'sleep') as sleep:
            self.assertEqual(imagery.fetch('https://imagery.nationalmap.gov/test',120,3),b'complete image')
            self.assertEqual(opened.call_count,2)
            self.assertEqual(opened.call_args.kwargs['timeout'],120)
            sleep.assert_called_once_with(2)

    def test_retry_exhaustion_and_permanent_http_failure(self):
        with patch.object(imagery.urllib.request,'urlopen',side_effect=TimeoutError('slow')) as opened, patch.object(imagery.time,'sleep'):
            with self.assertRaises(TimeoutError):imagery.fetch('https://imagery.nationalmap.gov/test',120,3)
            self.assertEqual(opened.call_count,3)
        failure=urllib.error.HTTPError('https://imagery.nationalmap.gov/test',404,'not found',{},None)
        with patch.object(imagery.urllib.request,'urlopen',side_effect=failure) as opened:
            with self.assertRaises(urllib.error.HTTPError):imagery.fetch('https://imagery.nationalmap.gov/test')
            self.assertEqual(opened.call_count,1)

    def test_failed_generation_records_stage_without_completion_marker(self):
        p=dict(terrain={'extent':dict(xmin=-118.01,xmax=-118.,ymin=33.,ymax=33.01)},origin_lat_lon_elevation=[33.,-118.,100.])
        with tempfile.TemporaryDirectory() as directory:
            root=Path(directory);source=root/'source.json';source.write_text(json.dumps(p))
            with patch.object(imagery,'fetch',side_effect=[b'{}',TimeoutError('slow')]):
                with self.assertRaisesRegex(RuntimeError,'Generate 1024'):imagery.acquire(source,root/'out',120,3,1024)
            error=json.loads((root/'out/failure.json').read_bytes())
            self.assertIn('1024',error['stage'])
            self.assertTrue((root/'out/service.json').exists())
            self.assertFalse((root/'out/imagery.json').exists())

    def test_extent_and_provenance_and_rejects_foreign_download_host(self):
        extent=dict(xmin=-118.01,xmax=-118.,ymin=33.,ymax=33.01,spatialReference={'wkid':4326})
        p=dict(terrain={'extent':extent},origin_lat_lon_elevation=[33.,-118.,100.])
        header=b'\x89PNG\r\n\x1a\n'+b'\0'*8+struct.pack('>II',2048,2048)
        response=dict(extent=extent,href='https://imagery.nationalmap.gov/test.png')
        with tempfile.TemporaryDirectory() as directory:
            root=Path(directory);source=root/'source.json';source.write_text(json.dumps(p))
            with patch.object(imagery,'fetch',side_effect=[b'{}',json.dumps(response).encode(),header]):
                imagery.acquire(source,root/'good')
            result=json.loads((root/'good/imagery.json').read_bytes())
            self.assertEqual(result['sha256'],hashlib.sha256(header).hexdigest())
            self.assertLess(result['local_bounds'][0],0);self.assertLess(result['local_bounds'][1],0)
            self.assertGreater(result['local_bounds'][2],0);self.assertGreater(result['local_bounds'][3],0)
            self.assertIn('not photography date',result['acquisition_date'])
            response['href']='https://example.com/test.png'
            with patch.object(imagery,'fetch',side_effect=[b'{}',json.dumps(response).encode()]):
                with self.assertRaisesRegex(ValueError,'host'):imagery.acquire(source,root/'bad')
            self.assertFalse((root/'bad/imagery.json').exists())

if __name__=='__main__':unittest.main()
