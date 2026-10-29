from collections import defaultdict
from pathlib import Path
import gzip,re,sys
def read(file):
 b=(gzip.open(file,'rt') if str(file).endswith('.gz') else open(file)).read().splitlines()
 names={}; objs={}; funobj={}; cost=defaultdict(int);calls=defaultdict(lambda:[0,0])
 ob=fn=cfn=n=None
 for l in b:
  if l.startswith(('fn=','cfn=','ob=','cob=')):
   k,s=l.split('=',1);m=re.fullmatch(r'\((\d+)\)(?: (.*))?',s);assert m,l
   i=int(m[1]);v=m[2]
   tab=objs if k in ('ob','cob') else names
   if v is not None:assert tab.setdefault(i,v)==v,(i,v)
   if k=='ob':ob=i
   if k=='fn':fn=i;funobj[i]=ob
   if k=='cfn':cfn=i
  elif l.startswith('calls='):
   n=int(l.split('=')[1].split()[0]);assert n>0
  elif l and (l[0].isdigit() or l[0] in '+-*'):
   t=l.split()
   if len(t)!=2:raise ValueError(l)
   v=int(t[1])
   if n is None:cost[fn]+=v
   else:
    q=calls[fn,cfn];q[0]+=n;q[1]+=v;n=None
 total=sum(cost.values());assert total==int(next(l for l in b if l.startswith('totals:')).split()[1]),(total,b[-2])
 return names,objs,funobj,cost,calls,total
def report(p):
 ns,os,fo,co,ct,t=read(p);q=sys.argv[2] if len(sys.argv)>2 else None
 for i,n in sorted(co.items(),key=lambda x:-x[1])[:28]:
  print('self %5.2f %10d %s'%(100*n/t,n,ns[i][:200]))
 if q:
  for i in ns:
   if q not in ns[i]:continue
   print('\nMATCH',i,repr(ns[i])[:210],'self',co[i])
   for (a,b),(n,v) in sorted(ct.items(),key=lambda x:-x[1][1]):
    if a==i or b==i:
     print(' ', 'caller' if b==i else 'callee',n,v,ns[a if b==i else b][:190])
if __name__=='__main__':report(sys.argv[1])
