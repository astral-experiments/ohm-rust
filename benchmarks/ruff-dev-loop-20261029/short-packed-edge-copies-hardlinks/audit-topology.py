"""Audit inode partitions, actual root rustc, linkat to working objects, and mapped previous data."""
import argparse,ast,collections,gzip,hashlib,json,re
from pathlib import Path
p=argparse.ArgumentParser();p.add_argument('records',type=Path);p.add_argument('--out',type=Path,required=True);a=p.parse_args();r=a.records

def gh(b):return hashlib.sha256(b).hexdigest()
def load(f):return json.loads(f.read_bytes())
def tag(names):
 if any('/out/libruff_text_size' in n for n in names):return 'root-metadata'
 if any(n.endswith('/dep-graph.bin') for n in names):return 'dep-graph'
 if any(n.endswith('/query-cache.bin') for n in names):return 'query-cache'
 if any(n.endswith('/metadata-prefix-v3.ohm-prefix-v3') for n in names):return 'metadata-prefix'
 raise AssertionError(names)
def inode(row):return (row['dev'],row['ino'])
def packed(f):return gzip.decompress(f.read_bytes())
reports={}
for arm in 'AB':
 train=r/f'train-{arm}'; gold=load(train/'original-live-topology.json');mapping={name:tag(names) for names in gold['classes'] for name in names};assert len(mapping)==9 and len(gold['classes'])==4 and not gold['outside_names']
 for mode,prefix in [('in-place','native'),('public-copy','native-public'),('joint','native-joint')]:
  here=r/f'{prefix}-{arm}';st=load(here/'pre-dirty-topology.json');partition=load(train/({'in-place':'original-live','public-copy':'public-copy','joint':'independent-preserved'}[mode]+'-topology.json'))
  assert st['classes']==partition['classes'] and not st['outside_names'] and set(st['paths'])==set(gold['paths']);assert len(st['paths'])==36
  for name,row in gold['paths'].items():
   got=st['paths'][name]; assert all(row[k]==got[k] for k in ['sha256','mode','mtime_ns','size']);assert (mode=='in-place')==(inode(got)==inode(row))
  assert len(st['classes'])==(0 if mode=='public-copy' else 4)
  cmd=load(here/'rustc-command.json')['args'];pre=cmd[cmd.index('--out-dir')+1].split('/products/',1)[0]+'/';assert pre.startswith('/')
  roster=load(here/'native.roster.json');assert roster=={'rustc':[['ruff_text_size',cmd]],'wild':0};assert load(here/'repeat.roster.json')=={'rustc':[],'wild':0}
  records={};threads=collections.defaultdict(set);root=[];raw_hashes={}
  for f in sorted(here.glob('fs-task.*.gz')):
   data=packed(f);raw_hashes[f.name]=gh(data);pid=int(f.name.split('.')[-2]);records[pid]=[]
   for line in data.decode().splitlines():
    when,sep,call=line.partition(' ')
    if not sep or not re.fullmatch(r'\d+\.\d+',when):continue
    if call.startswith('execve(') and call.endswith(') = 0'):
     _,argv,_=ast.literal_eval('('+call[7:-len(') = 0')]+')');
     if argv==cmd:root.append(pid)
    if 'CLONE_THREAD' in call and call.startswith(('clone3(','clone(')):
     m=re.search(r' = ([1-9]\d*)$',call)
     if m:threads[pid].add(int(m[1]))
    records[pid].append((when,pid,call))
  assert len(root)==1;group=set(root);todo=list(root)
  while todo:
   for t in threads[todo.pop()]-group:group.add(t);todo.append(t)
  assert group.issubset(records);events=sorted(e for t in group for e in records[t]);files={pre+n:label for n,label in mapping.items()};fdmap={};obs={key:collections.Counter() for key in set(mapping.values())};names={key:set() for key in obs}
  for _,pid,call in events:
   if call.startswith(('linkat(','link(','renameat2(','renameat(','rename(')) and call.endswith(' = 0'):
    ops=re.findall(r'"([^"\x00]+)"',call);assert len(ops)==2,call[:90];old,new=ops
    if old in files:
     t=files[old];files[new]=t;obs[t]['working_links' if call.startswith('link') else 'renames']+=1
    else:files.pop(new,None)
    if call.startswith('rename'):files.pop(old,None)
   elif call.startswith('openat('):
    m=re.search(r' = (\d+)<([^>]+)>$',call)
    if not m:continue
    n=int(m[1]);target=m[2];fdmap.pop(n,None)
    if target in files:
     if 'O_TRUNC' in call:files.pop(target,None);continue
     if 'O_RDONLY' in call:fdmap[n]=(files[target],target);obs[files[target]]['successful_read_opens']+=1
   elif call.startswith(('read(','pread64(')):
    m=re.match(r'\w+\((0x[0-9a-f]+|\d+),.* = (0x[0-9a-f]+|\d+)$',call)
    if not m:continue
    pair=fdmap.get(int(m[1],0));n=int(m[2],0)
    if pair and n>0:obs[pair[0]]['read_bytes']+=n;names[pair[0]].add(Path(pair[1]).name)
   elif call.startswith('mmap(') and 'PROT_READ' in call and not re.search(r' = -1\b',call):
    m=re.search(r'^mmap\([^,]+, (\d+), .*?, \d+<([^>]+)>, (?:0x[0-9a-f]+|\d+)\) = 0x[0-9a-f]+$',call)
    if m and m[2] in files:t=files[m[2]];obs[t]['mmap_bytes']+=int(m[1]);names[t].add(Path(m[2]).name)
   elif call.startswith('close('):
    m=re.match(r'close\((\d+)(?:<[^>]+>)?\) = 0$',call)
    if m:fdmap.pop(int(m[1]),None)
  for t in ['dep-graph','query-cache']:assert obs[t]['mmap_bytes']>0 and obs[t]['working_links']>0,(prefix,arm,t,obs)
  out=load(here/'native.json')['outputs'];assert list(out.values())==list(load(here/'repeat.json')['outputs'].values())==list(load(r/f'clean-{arm}/clean.json')['outputs'].values());same=next(iter(out.values()));assert len(out)==1
  reports[f'{arm}/{mode}']={'paths':len(st['paths']),'prior_named_groups':len(gold['classes']),'actual_groups':len(st['classes']),'same_inode_as_live_paths':sum(inode(st['paths'][n])==inode(gold['paths'][n]) for n in st['paths']),'links_outside_study':st['outside_names'],'alias_classes':{tag(names):[Path(n).name for n in names] for names in gold['classes']},'mapped_and_read':{t:dict(v) for t,v in sorted(obs.items())},'names_observed_when_read':{t:sorted(v) for t,v in sorted(names.items())},'root_and_threads':len(group),'root_rustc':1,'stock_wild':0,'repeat_rustc':0,'root_rmeta_sha256':same,'raw_trace_shas':raw_hashes}
assert len({v['root_rmeta_sha256'] for v in reports.values()})==1
if a.out.exists():raise FileExistsError(a.out)
a.out.write_text(json.dumps(reports,sort_keys=True,indent=2)+'\n');print([(k,v['actual_groups'],v['mapped_and_read']) for k,v in reports.items()][:3]);
