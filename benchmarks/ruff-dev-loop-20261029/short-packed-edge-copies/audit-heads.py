"""Read every advertised branch and pull-request head and authenticate its serialized-edge decoder."""
import argparse, hashlib, json, re, subprocess
from pathlib import Path
p=argparse.ArgumentParser();p.add_argument('--github',default='gh-auto');p.add_argument('--out',type=Path,required=True);p.add_argument('--source-bodies',type=Path);a=p.parse_args()
def request(args):
    return json.loads(subprocess.check_output([a.github,*args]))
prs=request(['pr','list','-R','astral-sh/ohm-rust','--state','all','--limit','1000','--json','number,title,state,headRefOid'])
branches=request(['api','--paginate','--slurp','repos/astral-sh/ohm-rust/branches?per_page=100'])
subjects={f"pr:{r['number']}":r['headRefOid'] for r in prs}
subjects.update({f"branch:{r['name']}":r['commit']['sha'] for page in branches for r in page})
assert prs and len(branches[-1])<100 and len(prs)<1000
heads=sorted(set(subjects.values())); files={}; bodies={}; missing=[]; other_trees={}
path='compiler/rustc_middle/src/dep_graph/serialized.rs'
for base in range(0,len(heads),25):
    part=heads[base:base+25]
    fields=' '.join(f's{j}:object(expression:"{k}:{path}"){{... on Blob{{oid text}}}}' for j,k in enumerate(part))
    result=request(['api','graphql','-f','query=query {repository(owner:"astral-sh",name:"ohm-rust"){'+fields+'}}'])['data']['repository']
    for j,k in enumerate(part):
        blob=result[f's{j}']
        if not blob:
            missing.append(k);continue
        assert blob['text'],(j,k)
        data=blob['text'].encode(); assert hashlib.sha1(b'blob '+str(len(data)).encode()+b'\0'+data).hexdigest()==blob['oid']
        files[k]=blob['oid'];bodies[blob['oid']]=data.decode()
for base in range(0,len(missing),25):
    part=missing[base:base+25]
    fields=' '.join(f's{j}:object(expression:"{k}:"){{... on Tree{{oid entries{{name type}}}}}}' for j,k in enumerate(part))
    result=request(['api','graphql','-f','query=query {repository(owner:"astral-sh",name:"ohm-rust"){'+fields+'}}'])['data']['repository']
    for j,k in enumerate(part):
        tree=result[f's{j}']; names=sorted(x['name'] for x in tree['entries'])
        assert 'compiler' not in names and 'src' not in names,(k,names)
        other_trees[k]={'root_tree':tree['oid'],'entries':names}
if a.source_bodies:a.source_bodies.mkdir(exist_ok=True)
variants={}
for oid,s in sorted(bodies.items()):
    if a.source_bodies:(a.source_bodies/(oid+'.rs')).write_text(s)
    lines=s.splitlines()
    variants[oid]={'bytes':len(s.encode()),'refs':sum(files.get(sha)==oid for sha in subjects.values()),'writes':[line.strip() for line in lines if 'edge_list_data.extend' in line or 'edge_list_data.push' in line or ('read_raw_bytes(' in line and 'edge' in line) or 'append_packed_edges' in line],'contains_exact_proposed_helper':'fn append_packed_edges(' in s,'locations':[i for i,line in enumerate(lines,1) if ('edge_list_data.extend' in line or 'edge_list_data.push' in line or 'append_packed_edges(' in line)]}
out={'repository':'astral-sh/ohm-rust','path':path,'pull_request_count':len(prs),'branch_count':sum(map(len,branches)),'distinct_head_count':len(heads),'non_compiler_head_count':len(missing),'non_compiler_trees':other_trees,'distinct_blobs':variants,'heads_to_blob':files,'refs_to_commit':subjects,'related_prs':[{k:r[k] for k in ['number','title','state','headRefOid']} for r in prs if re.search('edge|dependenc.y graph|serialized|compact dep',r['title'],re.I)]}
a.out.write_text(json.dumps(out,sort_keys=True,indent=2)+'\n')
print(out['pull_request_count'],out['branch_count'],out['distinct_head_count'],len(variants))
for k,v in variants.items():print(k,v['refs'],v['writes'])
