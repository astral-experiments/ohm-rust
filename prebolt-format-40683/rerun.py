"""Run the published ordinary 36-target design on the exact SDK identities in this packet."""
import importlib.util
import json
from pathlib import Path
import resource
import sys
from zipfile import ZipFile
D=Path(__file__).resolve().parent
def load(n,p):
    spec=importlib.util.spec_from_file_location(n,p);m=importlib.util.module_from_spec(spec);spec.loader.exec_module(m);return m
v=load('issued_high_40683',D/'verify.py')
STATS={'user_us':'ru_utime','system_us':'ru_stime','minor_faults':'ru_minflt','major_faults':'ru_majflt','voluntary_context_switches':'ru_nvcsw','involuntary_context_switches':'ru_nivcsw'}
def usage():
    u=resource.getrusage(resource.RUSAGE_CHILDREN);return {n:round(1e6*getattr(u,k))if n.endswith('_us')else getattr(u,k)for n,k in STATS.items()}
def program(branch,roles,seeds):
    for n in ('finite-prebolt/rerun.py','finite-prebolt/base007.py','ohm-uv-resolver-repro.py'):
        v.need(v.sha((branch/n).read_bytes())==v.d942.SOURCES[n],'the published d942 runner and shared functions must be available')
    p=load('original_d942_program',branch/'finite-prebolt/rerun.py')
    p.SEEDS=tuple(seeds);p.DRIVERS={k:x['driver_sha256']for k,x in roles.items()};p.LAUNCHERS={k:x['launcher_sha256']for k,x in roles.items()};original=p.load
    def watched(path,name):
        m=original(path,name)
        if name=='shared_cargo_runner':
            cargo=m.command
            def invoke(log,label,*args):
                before=usage()
                try:return cargo(log,label,*args)
                finally:
                    after=usage()
                    with (log/(label+'.usage.json')).open('x')as f:json.dump({k:after[k]-before[k]for k in before},f)
            m.command=invoke
        return m
    p.load=watched
    return p
if __name__=='__main__':
    v.need('--acknowledge-rebuilt' not in sys.argv and '--base-runner' not in sys.argv,'the strict external design requires the three measured SDK identities')
    with ZipFile(D/'records.zip')as z:
        raw=z.read('qualification.json');v.need(v.sha(raw)==json.loads(z.read('MANIFEST.json'))['qualification.json'],'issued subjects')
    q=json.loads(raw);v.qualify(q)
    program(D.parent,{'stock':q['stock'],**q['subjects']},v.FRESH_SEEDS).main()
