"""Shared clock validation and registered contrast arithmetic from the published 007 verifier."""
import argparse

import csv

import io

from collections import Counter

from hashlib import sha256

import itertools

import json

import math

from pathlib import Path

import random

import re

from statistics import median

from zipfile import ZipFile

PH=('discovery','confirm-1','confirm-2')

ARMS=('stock','parent','candidate')

ARGV=['cargo','check','-p','uv-resolver','--locked','--offline','-j8']

DIRTY={'uv_workspace','uv_types','uv_distribution','uv_resolver'}

PATCH='24852ea0d29fe8eb355f2a0d4eb682c54e53fa56af3d048635f4de9049925ecb'

CARGO='02a86ebd2d9e4f237b33f212102b81ed145eb77fae090d67e379cb6f1d70a573'

SDK={
 'stock':('a4fa11bcd38280639cb94daacc125472657db6c3','a7648053cbad025c1bb0bebb7009ce9e1515bdb02e8c30bef29b52a881e5b8de'),
 'base':('556a8364f1485a03be31b1a50c250d63198a943d','c502c3d3454f20b07806de770170d27b6d2043b6d8d8abbab829d5c99cc88ad0'),
 'T1':('8833a44ae11c06f328141f5b4254729c2d7b27cc','cdb9a3ad169b7af296562f96f9cfa4f10fb8d40d8830d2511bfced8b8f87f560'),
 'T3':('a34f2fffdf6f68cb07e89e0a239d30d0e2af38b5','62e6dcf4febdcf50d4a214ab140a339a82be47461a7c7d56c734510b371bf30c')}

WORK={'externs':9,'ctfe-stress-5':9,'cargo-0.87.1':12,'token-stream-stress':9,'match-stress':9,'tuple-stress':12,'diesel-2.2.10':12,'bitmaps-3.2.1':12,'serde-1.0.219-new-solver':3}

SUITES={'tests/incremental/overlapping-impls-in-new-solver-issue-135514.rs':[0,1],
        'tests/incremental/ich_method_call_trait_scope.rs':[0,0], 'tests/incremental/change_implementation_cross_crate':[0,0],
        'tests/ui/traits/ambiguous-trait-matching-distinct-lifetimes.rs':[1],'tests/run-make/trait-impl-bucket-projection':[0]*14}

def need(condition,msg):
    if not condition:raise ValueError(msg)

def digest(b):return sha256(b).hexdigest()

def ishash(s):return bool(re.fullmatch('[0-9a-f]{64}',s))

PROFILE={
 'base':('f2f1991cef35ff8706a8f4e619082b36dc5867f00d08da7f619050ee49055e43',43479456),
 'T1':('bad0f4c8d291ddd5f27d3ecdc022837523de4ff7513a7dc4df4f4f2d25f03614',43534456),
 'T3':('2fc5ae8ff82814328637a40961bfbba33d9f69664e15a380a977f7a449e59c37',43542120)}

def clock(c,expected):
    d=c['cargo'];names=[n.replace('-','_') for n in re.findall(r'^\s*(?:Checking|Compiling) ([\w-]+) v',c['stderr'],re.M)]
    need(d['cmd']==ARGV and d['dirty_units']==names and c['stdout']=='' and 'Finished 'in c['stderr'],'ordinary Cargo log and command')
    need(math.isfinite(d['wall_s']) and d['wall_s']>0 and c['usage']['sys_us']>=0 and c['usage']['user_us']>0,'elapsed and CPU')
    need((names and DIRTY<=set(names)) if expected=='cold' else (len(names)==4 and set(names)==DIRTY) if expected=='edit' else names==[],'observed dirty crates/no-op')
    return d['wall_s']

def cohort(phase,seed,v,q):
    roles=list(itertools.permutations(sorted(ARMS)));random.Random(seed).shuffle(roles)
    need(v['phase']==phase and v['seed']==seed and v['closed']['all_workers_returned_zero'] and v['closed']['admitted']==q['admission_sha256'],'actual closed cohort')
    rows=[json.loads(s) for s in v['pairs_jsonl'].splitlines()];orders=[list(x)for x in roles]
    need(v['order']==orders==[list(x)for x in rows] and set(v['arms'])==set(ARMS),'six counterbalanced triples')
    cold={}
    for role in ARMS:
        a=v['arms'][role];identity=a['loaded'];tag=q['timed'][role];sdk=q['stock'] if tag=='stock' else q['optimized'][tag]
        need(identity['cargo']==CARGO and identity['driver']==sdk['driver_sha256'] and identity['llvm']==q['shared_llvm'] and identity['launcher']==sdk['launcher_sha256'],'timed process identity')
        need(a['protocol'][0]=={'patch':PATCH,'state':'ready'} and a['protocol'][1:]==[r[role]for r in rows],'worker dispatch')
        cold[role]=clock(a['setup']['warm'],'cold');clock(a['setup']['settled'],'noop')
        need(len(a['trials'])==6,'six real restores')
        for i,(x,row) in enumerate(zip(a['trials'],rows,strict=True),1):
            raw=row[role];need(raw['state']=='measured' and raw['trial']==str(i) and raw['dirty']==x['edit-D']['cargo']['dirty_units'],'row identity')
            for label,dirt,field in [('settled-A','noop','parent_noop'),('edit-D','edit','edit'),('noop-D','noop','noop')]:
                need(clock(x[label],dirt)==raw[field],'raw Cargo time differs')
    return rows,cold

def contrast(blocks,role,alpha,threshold):
    a,b=role;raw=blocks[0]+blocks[1]
    def value(row):
        # Reproduce the registered secondary stock/base arithmetic, including binary64 rounding.
        return (row['stock']['edit']/row['candidate']['edit'])/(row['parent']['edit']/row['candidate']['edit']) if role==('stock','parent') else row[a]['edit']/row[b]['edit']
    vals=list(map(value,raw));n=len(vals);need(n==12,'exactly twelve held-out pairs')
    k=max(k for k in range(n//2)if 2*sum(math.comb(n,i)for i in range(k+1))/2**n<=alpha)
    interval=[sorted(vals)[k],sorted(vals)[-k-1]]
    strata=[[value(r)for r in raw if (list(r).index(a)<list(r).index(b))==side]for side in (False,True)]
    need(list(map(len,strata))==[6,6],'relative order strata')
    seeds=[median(map(value,c))for c in blocks];orders=list(map(median,strata))
    untied=[v for v in vals if v!=threshold];m=len(untied);p=sum(math.comb(m,j)for j in range(sum(v>threshold for v in untied),m+1))/2**m
    return dict(median=median(vals),paired=vals,interval=interval,coverage=1-2*sum(math.comb(n,i)for i in range(k+1))/2**n,one_sided_sign_p=p,threshold=threshold,alpha=alpha,order_medians=orders,seed_medians=seeds,passed=interval[0]>threshold and p<alpha and min(seeds+orders)>threshold)
