"""Reject any native discrepancy before the first measured compiler starts."""
import ast, gzip, hashlib, json, pathlib, sys
r=pathlib.Path(sys.argv[1])
def get(p):return json.loads((r/p).read_bytes())
def digest(b):return hashlib.sha256(b).hexdigest()
gold={}; arms={}
for arm in 'AB':
    d=r/('native-'+arm); c=r/('clean-'+arm)
    cmd=get('native-'+arm+'/rustc-command.json')['args']
    assert '--extern' not in cmd and any('incremental=' in a for a in cmd)
    n=get('native-'+arm+'/native.roster.json')
    assert n['wild']==0 and len(n['rustc'])==1 and n['rustc'][0]==['ruff_text_size',cmd]
    assert get('native-'+arm+'/repeat.roster.json')=={'rustc':[],'wild':0}
    for place,l,expect in [(d,'native',False),(d,'repeat',True),(c,'clean',False)]:
        rec=json.loads((place/(l+'.json')).read_bytes());assert len(rec['typed'])==1 and rec['typed'][0][0]=='ruff_text_size' and rec['typed'][0][2]==expect and rec['rc']==0
        data=gzip.decompress((place/(l+'.root.rmeta.gz')).read_bytes())
        assert set(rec['outputs'].values())=={digest(data)}
        if not gold:gold={'bytes':len(data),'sha256':digest(data)}
        assert {'bytes':len(data),'sha256':digest(data)}==gold
    tests={}
    for state in ['current','previous']:
        for kind in ['legacy','external']:
            t=json.loads((c/(f'consumer-{state}-{kind}.json')).read_bytes());key=state+'-'+kind
            if key=='previous-external':
                assert t['compiler_exit']==1 and t['codes']==['E0599'] and 'to_std_range' in t['diagnostics'][0]
            else:
                assert t['compiler_exit']==t['exit']==0 and t['codes']==[] and not t['stderr'] and t['stdout']=='1..3:é\n'
            tests[key]={k:t[k] for k in ['compiler_exit','exit','source','caller','binary','stdout','codes','diagnostics'] if k in t}
    direct=get('direct-'+arm+'/native-replay.json');assert direct['exit']==0 and set(direct['outputs'].values())=={gold['sha256']}
    arms[arm]=tests
assert arms['A']==arms['B'] and not list(r.glob('slot-*'))
print(json.dumps({'verified_whole_rmeta':gold,'same_external_verdicts':arms['A'],'both_direct':True,'profiles_already_present':0},sort_keys=True))
