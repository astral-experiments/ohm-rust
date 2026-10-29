"""Check the independently rerun Rust compiler comparison and exercise the evidence rejection paths."""
import argparse,copy,hashlib,importlib.util,json,re
from pathlib import Path
ROOT=Path(__file__).resolve().parent
spec=importlib.util.spec_from_file_location('compare',ROOT/'verify.py');v=importlib.util.module_from_spec(spec);spec.loader.exec_module(v)
EXPECTED={'A':'1512fb90725102e1290790b2202a88bd968ea60a4c09336803a0939a636cc0ac','B':'6d088c8d9c7297733e2d29d96e43dd7617f6e2e2f5c777bac2ec3ec05bb3016a'}
COHORT={'compiler':'fa2ed3ec44adb4455d5323c4ee582becd5307cb4aae6b5cfc3cdc289b3140512','cargo':'789c2f116732ed642565d0f92c500838f8cd53664fbe60f205a5621c43cbecf1','wild':'0d6335808b647d499f3d74993b602e64ff0625206c809bf4913c487eea380cd5'}
def hashb(b):return hashlib.sha256(b).hexdigest()
def validate(d,reg,raw=None):
 assert d['registration_sha256']==hashb(reg) and d['matched_rustc']==COHORT['compiler'] and d['common_cargo_wild']==[COHORT['cargo'],COHORT['wild']]
 assert d['shared_std_count']==len(d['std_name_sha256_table'])==53 and all(re.fullmatch('[0-9a-f]{64}',s) for s in d['std_name_sha256_table'].values())
 assert all(d['native']['arms'][a]['driver']==h for a,h in EXPECTED.items());assert d['affinity']==[8,9,10,11]
 g=d['resource_guard'];assert g['exit']==0 and g['violation'] is None and len(g['minimum'])==len(g['floors'])==4 and all(i>j>0 for i,j in zip(g['minimum'],g['floors']))
 for a in 'AB':
  commands=d['native']['arms'][a]['commands'];assert commands['native']['actual_rustc']==['ruff_text_size'] and not commands['repeat']['actual_rustc'] and all(commands[n]['actual_wild']==0 for n in ['native','repeat']);assert all(re.fullmatch('[0-9a-f]{64}',x['stdout_sha256']) for x in commands.values()); assert commands['native']['whole_rmeta']==d['native']['whole_rmeta'] and d['native']['whole_rmeta']['bytes']>0
 return v.verify(d,raw)
def wrong(d,group):
 if group=='stored count':d['slots']['1-A']['ir']+=123456
 if group=='lost ELF':d['native']['arms']['B']['external']['current-external']['binary']='0'*64
 if group=='candidate image':d['native']['arms']['B']['driver']=EXPECTED['A']
 if group=='partial schedule':del d['slots']['4-B']
 if group=='changed metadata':d['slots']['1-B']['whole_metadata_sha256']='0'*64
 if group=='extra root':d['native']['arms']['B']['commands']['repeat']['actual_rustc']=['ruff_text_size']
 if group=='crossed reserve':d['resource_guard']['minimum'][0]=d['resource_guard']['floors'][0]
 if group=='upstream compile error':d['native']['arms']['A']['external']['previous-external']['codes']=['E0460']
if __name__=='__main__':
 p=argparse.ArgumentParser(description=__doc__);p.add_argument('--report',type=Path,default=ROOT/'result.json');p.add_argument('--raw',type=Path);args=p.parse_args();d=json.loads(args.report.read_bytes());b=(ROOT/'registration.json').read_bytes();proven=validate(d,b,args.raw)
 for label in ['stored count','lost ELF','candidate image','partial schedule','changed metadata','extra root','crossed reserve','upstream compile error']:
  q=copy.deepcopy(d);wrong(q,label)
  try:validate(q,b,None)
  except (AssertionError,KeyError,ValueError):continue
  raise ValueError('accepted corrupt case: '+label)
 print(json.dumps({'independent':proven,'rejected_corruptions':8,'reopened_raw_profiles':24 if args.raw else 0},sort_keys=True))
