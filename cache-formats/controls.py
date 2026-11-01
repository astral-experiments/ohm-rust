"""Run semantic rejection controls on actual affirmative rows, without reusing their original content hashes."""
import copy,importlib.util,json,sys,zipfile
from pathlib import Path
D=Path(__file__).resolve().parent
p=D
s=importlib.util.spec_from_file_location('review_decoder',p/'verify.py');v=importlib.util.module_from_spec(s);s.loader.exec_module(v)
def run(path):
 verdict=v.verify(path)
 with zipfile.ZipFile(path) as z:
  raw={n:z.read(n)for n in z.namelist()}
 get=lambda n:json.loads(raw[n]);m=get('manifest.json');scope=m['scope'];sdk=v.images(get,m,scope)
 fx=raw['probe.rs'];hs={'A':v.sha(fx),'B':v.sha(fx.replace(b'        11\n',b'        17\n')),'E':v.sha(fx.replace(b'impl Score for Left {',b'impl Score for Right {'))}
 before='base8' if scope=='mandatory' else 'integrated10';k='unforced/'+before+'-to-new11.json'
 controls={'wrong-causal-image':('positive.selected_driver_identity.1',123),'different-seed':('no_edit.seed_sha256','a'*64),
           'bad-negative-program':('negative.compile.exit',0),'generic-crash-is-not-a-load-state':('assert_loaded.compile.stderr','internal compiler error'),
           'missing-native-program-output':('positive.run.stdout','11\n'),'wrong-actual-compiler-flag':('positive.compile.argv.1','other.rs'),
           'wrong-version-environment':('assert_not_loaded.forced_rustc_version','rustc-perf')}
 for name,(chain,value)in controls.items():
  j=get(k);q=j;bits=chain.split('.')
  for n in bits[:-1]:q=q[int(n)if n.isdigit()else n]
  last=bits[-1];q[int(last)if last.isdigit()else last]=value
  def changed(n):return j if n==k else get(n)
  try:v.matrix(changed,scope,'unforced',sdk,hs)
  except (ValueError,KeyError):pass
  else:raise ValueError('control accepted: '+name)
 return {'scope':scope,'genuine_packet':v.sha(path.read_bytes()),'positive':True,'scientific_controls_rejected':sorted(controls)}
if __name__=='__main__':
 if sys.flags.optimize or len(sys.argv)<2:raise SystemExit("python3 -ISB controls.py mandatory.zip [integrated-v10.zip]")
 print(json.dumps([run(Path(p))for p in sys.argv[1:]],sort_keys=True,indent=2))
