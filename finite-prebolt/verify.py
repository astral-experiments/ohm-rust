"""Verify the finite-receiver measurements from the corrected fourth-role producer."""
from pathlib import Path
import argparse
import hashlib
import importlib.util
import json

REG='251e04bd4cc16efc0418c48b745d882f554a6805a52fd29f366dc4b8c4b5acb2'
STAGE='b78b9b10b32a90d9488a2ebb3a456969e66bade3b92edb6cea291415f4b72567'
FAILED_ZIP='c17f1fa01ac912c8dead676ed55c7b0d201859708724984dbfbc0e4daa8bac58'
FAILED_RETURN='1d4003873a4dad24bf74351092537af218361b5a7f607597e2047990efe84134'
FAILED_WORKER='361f80bbbfcf5a1f3eb78697d6a11636bc39409047ab42d923a58b0f6a4b44a5'
HELPERS='120790182e9ebfac4a9adce4ce8e7afaa7162dde8b4d5e00f3161053b6cdbbe5'
ADMISSION='e417531add1277ac1b77005668c4b75d9387827325f7938830b125d807074d87'
def configure(previous):
    previous.REG,previous.STAGE=REG,STAGE
    return previous
def check_failure(f):
    if not (f['archive_sha256']==FAILED_ZIP and f['worker_log_sha256']==FAILED_WORKER and hashlib.sha256(f['return_raw'].encode()).hexdigest()==FAILED_RETURN and json.loads(f['return_raw'])['exit']==1 and f['scored_rows']==0 and f['failure']=='legacy selector rejected fourth role before stage2'):
        raise ValueError('replaced original failed build')
def check_fixture(q):
    raw=q['preceding_discovery_setup_raw'];h=json.loads(raw)
    if not (hashlib.sha256(raw.encode()).hexdigest()==HELPERS and q['admission_sha256']==h['signed_admission']==ADMISSION and h['native_rows_at_failure']==0 and set(h['helpers'])=={'screen.py','score-pairs.py'}):
        raise ValueError('displaced discovery or helper identity')
    fixture=q['common_t4_fixture'];t=q['optimized']['T4']
    encoded=(json.dumps(fixture,sort_keys=True,indent=2)+'\n').encode()
    if not (t['driver_sha256']=='f10dcce341a10cd09646bdd778952f6ad32631854d3197f917da8ae47d06d642' and t['profile_sha256']=='70892579bb848a9eb268d1d74538e53b849f3de4c0df6a05a504f071bdb869ea' and t['profile_bytes']==43532768 and t['fixture_receipt']==hashlib.sha256(encoded).hexdigest()=='fbd1a2d527e16d79b383e7a3960a325bde53a3013b523311484de696d916275f' and t['finite_receipt']=='ab86d928740d59d16609fa969cb251aabd8962e9d680cb9fde52d136a6462d59' and fixture['regression_source']==q['optimized']['T3']['commit'] and fixture['source_before']!=fixture['executed_files'] and t['dispatch']==fixture['dispatch']):
        raise ValueError('common 007 oracle, separate finite suite, or actual subject changed')
def replay(common,doc,negative=False):
    check_fixture(doc['qualification'])
    check_failure(doc['qualification']['displaced_generation'])
    result=common.compute(doc)
    if negative:
        failures=common.negatives(doc)
        try:check_failure(doc['qualification']['displaced_generation']|{'scored_rows':1})
        except ValueError:failures.append('failed generation credited')
        else:raise ValueError('failed generation credited')
        result['rejected_controls']=failures
    return result
if __name__=='__main__':
    d=Path(__file__).resolve().parent;s=importlib.util.spec_from_file_location('finite_evidence',d/'finite.py');f=importlib.util.module_from_spec(s);s.loader.exec_module(f)
    p=argparse.ArgumentParser();p.add_argument('packet',nargs='?',type=Path,default=d/'records.zip');p.add_argument('--negative',action='store_true');a=p.parse_args()
    print(json.dumps(replay(configure(f),f.load(a.packet),a.negative),sort_keys=True,indent=2))
