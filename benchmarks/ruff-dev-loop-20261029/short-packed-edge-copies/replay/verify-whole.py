"""Reopen all genuine Cargo, native-replay and whole-rustc profile receipts for one arm pair."""
import argparse,gzip,hashlib,itertools,json,math,os,re,statistics,subprocess as sp
from pathlib import Path

p=argparse.ArgumentParser();p.add_argument('evidence',type=Path);p.add_argument('--out',type=Path,required=True);a=p.parse_args();r=a.evidence
assert (r/'launcher-exit').read_text().strip()=='0'

def read(path):return json.loads((r/path).read_bytes())
def shas(b):return hashlib.sha256(b).hexdigest()
def load(p):return gzip.decompress(p.read_bytes())

def counts(raw):
    out={};fd=os.memfd_create('anonymous-rustc-CG')
    with os.fdopen(fd,'w+b') as f:
        f.write(raw);f.flush()
        for typ,inc in (('self','no'),('inclusive','yes')):
            f.seek(0)
            cmd=['callgrind_annotate','--auto=no','--threshold=100','--show=Ir','--inclusive='+inc,f'/proc/self/fd/{fd}']
            s=sp.check_output(cmd,pass_fds=[fd]).decode(); rows=[line for line in s.splitlines() if '<rustc_middle::dep_graph::serialized::SerializedDepGraph>::decode ' in line or '<rustc_middle::dep_graph::serialized::SerializedDepGraph>::decode() ' in line]
            if not rows:
                rows=[line for line in s.splitlines() if ('SerializedDepGraph' in line and '>::decode ' in line)]
            assert len(rows)==1,rows
            out[typ]=int(rows[0].split()[0].replace(',',''))
    return out

def native(arm):
    bd=read(f'native-{arm}/binding.json');b=r/f'native-{arm}';
    obs={q:read(f'native-{arm}/{q}.json') for q in ['native','repeat']}; rosters={q:read(f'native-{arm}/{q}.roster.json') for q in obs}
    assert rosters['repeat']==dict(rustc=[],wild=0) and [n for n,_ in rosters['native']['rustc']]==[bd['root']] and rosters['native']['wild']==0
    assert [x[0] for x in obs['native']['typed'] if not x[2]]==[bd['root']] and all(x[2] for x in obs['repeat']['typed'])
    for q in obs:
        raw=load(b/(q+'.jsonl.gz')); rows=[json.loads(line) for line in raw.splitlines()]; assert rows[-1]=={'reason':'build-finished','success':True}
        assert sorted([[x['target']['name'],x['target']['kind'],x['fresh'],x['features']] for x in rows if x['reason']=='compiler-artifact'])==obs[q]['typed']
    result=read(f'clean-{arm}/clean.json');assert all(not x[2] for x in result['typed'])
    assert obs['native']['outputs']==obs['repeat']['outputs']==result['outputs']
    assert read(f'direct-{arm}/native-replay.json')['outputs']==read('native-A/native.json')['outputs']
    keys=['source','caller','compiler_exit','codes','diagnostics','exit','stdout','stderr','binary','rlib']
    verdict={}
    for name in ['current','previous']:
        for kind in ['external','legacy']:
            v=read(f'clean-{arm}/consumer-{name}-{kind}.json')
            assert v['source']==bd['sources'][0 if name=='current' else 1] and v['caller']=={'external':EXTERNAL,'legacy':LEGACY}[kind] and len(v['rlib'])==64
            if kind=='external' and name=='previous':
                assert v['compiler_exit']==1 and v['codes']==['E0599'] and len(v['diagnostics'])==1 and 'to_std_range' in v['diagnostics'][0] and 'binary' not in v
            else:
                assert v['compiler_exit']==0 and v['codes']==[] and v['exit']==0 and v['stdout']=='1..3:é\n' and not v['stderr'] and len(v['binary'])==64
            verdict[name+'-'+kind]={k:v[k] for k in keys if k in v}
    for where,name in [(b,'native'),(b,'repeat'),(r/f'clean-{arm}','clean')]:
        root=load(where/(name+'.root.rmeta.gz'))
        assert set(result['outputs'].values())=={shas(root)}
    return dict(binding=bd,typed=obs['native']['typed'],native_outputs=result['outputs'],test_verdicts=verdict,rustc_exec_sha=shas(load(b/'native.exec.gz')))

EXTERNAL='899aee3abc6a94857f04f6c3a3770a2784abc50e3cd034d071dd81c25ee49247'
LEGACY='e3541e4a4d4a51377883e5661afa745da9a5f310d3ef44c618f2ac2f8446ec8f'
arms={x:native(x) for x in 'AB'}; assert arms['A']['typed']==arms['B']['typed']
for k in ['ruff','edit','sources','root','cargo','wild','rustc','version']:assert arms['A']['binding'][k]==arms['B']['binding'][k]
assert arms['A']['binding']['driver']!=arms['B']['binding']['driver']
for k,v in arms['A']['test_verdicts'].items():
    other=arms['B']['test_verdicts'][k]
    for u in v:
        if u not in ('rlib','binary'):assert v[u]==other[u],(u,v,other)
    if 'binary' in v:assert v['binary']==other['binary'],(v,other)
assert set(arms['A']['native_outputs'].values())==set(arms['B']['native_outputs'].values())
rows={};pairs=[]; sequence=''.join(row[-1] for row in (Path(__file__).parent/'schedule.txt').read_text().splitlines());key='native-A/native.json';gold=read(key)['outputs']
for j,arm in enumerate(sequence):
    label=f'slot-{j//2+1}-{arm}';m=read(label+'/measurement.json');raw=load(r/label/'rustc.callgrind.gz')
    assert shas(raw)==m['raw_sha256'] and int(re.search(rb'^totals: (\d+)',raw,re.M).group(1))==m['ir']
    assert m['outputs']==gold and len(raw)==m['bytes'] and b'\nevents: Ir\n' in raw
    assert read(label+'/binding.json')['driver']==arms[arm]['binding']['driver']
    rows[label]={**m,'decode':counts(raw)}
for i in range(1,13):
    l=rows[f'slot-{i}-A'];c=rows[f'slot-{i}-B'];pairs.append(dict(pair=i,a=l['ir'],b=c['ir'],ir=c['ir']-l['ir'],percent=(c['ir']/l['ir']-1)*100,decode_percent=(c['decode']['inclusive']/l['decode']['inclusive']-1)*100))
report=dict(arms=arms,roster=sequence,profiles=rows,pairs=pairs,median_percent=statistics.median(x['percent'] for x in pairs),median_ir=statistics.median(x['ir'] for x in pairs),median_decoder_inclusive_percent=statistics.median(x['decode_percent'] for x in pairs),working_set='common previous parent Cargo snapshot, literal parent full-rustc argv and externs, one real same-generation root per ordinary edit on each compiler')
orders=[sequence[j*2:j*2+2] for j in range(12)];chrono=[math.log(p['b']/p['a'])*(1 if o=='AB' else -1) for p,o in zip(pairs,orders)]
obs=statistics.median(math.log(p['b']/p['a']) for p in pairs)
perms=[statistics.median(v*(1 if j in choose else -1) for j,v in enumerate(chrono)) for choose in itertools.combinations(range(12),6)]
p_value=sum(abs(v)>=abs(obs)-1e-15 for v in perms)/len(perms)
report['whole_root_confirmed']=sum(p['ir']<0 for p in pairs)>=10 and obs<0 and p_value<.05
report['two_sided_balanced_P']=p_value;report['order_assignments']=len(perms)
report['startup_ranges']={arm:{'min':min(p[arm] for p in pairs),'max':max(p[arm] for p in pairs)} for arm in ['a','b']}
import random
rng=random.Random('ruff_text_size-decoder-holdout-20261029'); values=[math.log(p['b']/p['a']) for p in pairs]
bootstrap=sorted(statistics.median(rng.choices(values,k=12)) for _ in range(30000))
report['paired_bootstrap_95_percent']=[math.expm1(bootstrap[i])*100 for i in (749,29249)]
report['median_by_chronological_order_percent']={o:statistics.median(p['percent'] for order,p in zip(orders,pairs) if order==o) for o in ('AB','BA')}
assert len(perms)==924 and sum(o=='AB' for o in orders)==6
guards=[read(f'slot-{i}-{arm}/terminal.json')['resource'] for i in range(1,13) for arm in 'AB']
report['resource_minima']=[min(s[i] for s in guards) for i in range(2)]
a.out.write_text(json.dumps(report,sort_keys=True,indent=2)+'\n');print(arms['A']['binding']['root'],report['median_percent'],report['median_ir'],report['median_decoder_inclusive_percent'],[(x['pair'],round(x['percent'],4)) for x in pairs],flush=True)
