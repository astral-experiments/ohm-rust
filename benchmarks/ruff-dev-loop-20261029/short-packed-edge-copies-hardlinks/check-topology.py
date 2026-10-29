"""Verify the ordinary Cargo prior aliases and the distinct private inodes in every measured restoration."""
import argparse,collections,gzip,hashlib,json,re,subprocess as sp,sys,tempfile
from pathlib import Path

def sha(b):return hashlib.sha256(b).hexdigest()
def rd(p):return json.loads(p.read_bytes())
def label(fs):
 if any('/out/libruff_text_size' in s for s in fs):return 'root-metadata'
 for term in ['dep-graph','query-cache','metadata-prefix']:
  if any('/'+term in s for s in fs):return term
 raise ValueError(fs)
def shape(data):return {label(g):sorted(Path(s).name for s in g) for g in data['classes']}
KEYS=('mode','size','mtime_ns','sha256')
def same_files(x,y,shared):
 assert x['classes']==y['classes'] and set(x['paths'])==set(y['paths']) and len(x['paths'])==36 and not x['outside_names'] and not y['outside_names']
 for k,prior in x['paths'].items():
  new=y['paths'][k];assert all(prior[attr]==new[attr] for attr in KEYS)
  assert shared==((prior['dev'],prior['ino'])==(new['dev'],new['ino']))
 for g in x['classes']:
  assert len({(x['paths'][n]['dev'],x['paths'][n]['ino']) for n in g})==len({(y['paths'][n]['dev'],y['paths'][n]['ino']) for n in g})==1
  assert all(x['paths'][n]['nlink']==y['paths'][n]['nlink']==len(g) for n in g)
 assert len(x['classes'])==4 and len({n for g in x['classes'] for n in g})==9

def measure(raw):
 reads=rd(raw/'train-A/original-live-topology.json');donor=rd(raw/'train-A/independent-preserved-topology.json');same_files(reads,donor,False); rows={}
 for pair in range(1,13):
  for arm in 'AB':
   phase=f'slot-{pair}-{arm}';r=rd(raw/phase/'pre-dirty-topology.json');same_files(donor,r,False)
   files=['pre-dirty-topology.json','measurement.json','terminal.json']; rows[phase]={n:sha((raw/phase/n).read_bytes()) for n in files}
 with tempfile.TemporaryDirectory() as d:
  path=Path(d)/'native.json';sp.run([sys.executable,'-IB',Path(__file__).with_name('audit-topology.py'),raw,'--out',path],check=True,stdout=sp.DEVNULL);n=rd(path)
 assert len(n)==6 and all(x['root_and_threads']==3 for x in n.values()); assert len({x['root_rmeta_sha256'] for x in n.values()})==1
 return {'copied_input_names':36,'hardlink_groups':shape(donor),'per_slot_raw_digests':rows,'native_phase':{k:{f:v for f,v in x.items() if f!='raw_trace_shas'} for k,x in n.items()},'strace_native_manifest_sha256':{k:sha(json.dumps(x['raw_trace_shas'],sort_keys=True).encode()) for k,x in n.items()},'training_reports':{n:sha((raw/'train-A'/n).read_bytes()) for n in ['original-live-topology.json','public-copy-topology.json','independent-preserved-topology.json']}}
def verify(comp):
 assert set(comp['hardlink_groups'])=={'root-metadata','dep-graph','metadata-prefix','query-cache'} and sorted(len(x) for x in comp['hardlink_groups'].values())==[2,2,2,3]
 assert comp['copied_input_names']==36 and set(comp['per_slot_raw_digests'])=={f'slot-{i}-{a}' for i in range(1,13) for a in 'AB'}
 for row in comp['per_slot_raw_digests'].values():assert set(row)=={'pre-dirty-topology.json','measurement.json','terminal.json'} and all(re.fullmatch('[0-9a-f]{64}',v) for v in row.values())
 for arm in 'AB':
  rows=[comp['native_phase'][arm+'/'+m] for m in ['in-place','public-copy','joint']];assert [r['actual_groups'] for r in rows]==[4,0,4] and [r['same_inode_as_live_paths'] for r in rows]==[36,0,0]
  for r in rows:
   assert r['root_rustc']==1 and r['repeat_rustc']==r['stock_wild']==0 and r['links_outside_study']=={} and r['root_rmeta_sha256']==comp['native_phase']['A/in-place']['root_rmeta_sha256']
   for k in ['dep-graph','query-cache']:
    d=r['mapped_and_read'][k];assert d['mmap_bytes']>0 and d['working_links']>=1 and d['successful_read_opens']>=1
if __name__=='__main__':
 p=argparse.ArgumentParser(description=__doc__);p.add_argument('--raw',type=Path);p.add_argument('--report',type=Path,default=Path(__file__).with_name('topology.json'));p.add_argument('--create',action='store_true');a=p.parse_args()
 if a.create:
  assert a.raw and not a.report.exists();data=measure(a.raw);verify(data);a.report.write_text(json.dumps(data,sort_keys=True,indent=2)+'\n')
 data=rd(a.report);verify(data)
 if a.raw:assert data==measure(a.raw)
 print('4 link groups, 9 alias names, 6 genuine Cargo dirty controls, 24 independent-inode valid restorations')
