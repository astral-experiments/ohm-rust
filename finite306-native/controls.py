"""Semantic refusals use elements from the real packet, independently of its whole-file hashes."""
from pathlib import Path
import copy,io,json,runpy,sys,zipfile
D=Path(__file__).resolve().parent
v=runpy.run_path(str(D/'verify.py'))
def reject(fn,*args):
    try:fn(*args)
    except ValueError:return
    raise RuntimeError('corruption was accepted: '+fn.__name__)
def main(path):
    v['verify'](path)
    with zipfile.ZipFile(path)as z:
        j=lambda n:json.loads(z.read(n));base='build/discovery/'
        identity=j(base+'stock/identity.json');identity['cargo']='0'*64;reject(v['signed_identity'],'stock',identity)
        q=j('qualification.json');q['T4']['header_format']=10;reject(v['qualify'],q)
        row=json.loads(z.read(base+'pairs.jsonl').splitlines()[0]);wrong=copy.deepcopy(row);wrong['candidate']['edit']+=.02
        obj=v['archive'](z.read(base+'candidate/pair-1.zip'),'build',('settled-A','edit-D','noop-D'))
        reject(v['paired'],wrong,list(row),'candidate',1,obj)
        reject(v['paired'],dict(reversed(list(row.items()))),list(row),'candidate',1,obj)
        a=obj['settled-A'][1];b=copy.deepcopy(obj['edit-D'][1]);b['units']['uv_resolver'][1]=a['units']['uv_resolver'][1];reject(v['change'],a,b)
        with zipfile.ZipFile(io.BytesIO(z.read(base+'stock/pair-1.zip')))as r:
            c=json.loads(r.read('edit-D.json'));raw=r.read('edit-D.stderr');reject(v['prog'],c,raw.replace(b'Compiling uv-resolver',b'Fresh uv-resolver'),'build','edit-D')
            c['cmd'].append('--timings');reject(v['prog'],c,raw,'build','edit-D')
        with zipfile.ZipFile(io.BytesIO(z.read('test/discovery/candidate/pair-1.zip')))as r:
            roster=r.read('edit-D.listing');reject(v['listing'],roster.replace(b': test',b': bench',1))
        rdir='test/discovery/candidate/offclock/';transcript=z.read(rdir+'all-resolver-tests.out');names=v['listing'](z.read(rdir+'exact-list.out'))
        reject(v['passed'],transcript.replace(b' ... ok',b' ... FAILED',1),names)
        ratios=[[1.5,k]for k in ['stock','candidate']*3];reject(v['result'],{'discovery':ratios,'confirm-1':ratios},'build',{})
    print('Replayed authentic rows; refused wrong Cargo or format, changed/swapped clock, stale stamp, missing crate, traced argv, changed list, failed test and below-gate confirmation.')
if __name__=='__main__':main(Path(sys.argv[1])if len(sys.argv)>1 else D/'records.zip')
