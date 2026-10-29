"""Compare rustc rlib constituents, including an ordered GNU symbol-to-member map."""
import hashlib
import json
import mmap
from pathlib import Path
import struct
import sys

def digest(data):return hashlib.sha256(data).hexdigest()
def cstr(s,off):return s[off:s.index(b'\x00',off)]
def leb(buf,i):
    val=shift=0
    while True:
        c=buf[i];i+=1;val+=(c&127)<<shift
        if c<128:return val,i
        shift+=7;assert shift<70
def elf(data):
    assert data[:7]==b'\x7fELF\x02\x01\x01';assert int.from_bytes(data[16:18],'little')==1 and int.from_bytes(data[18:20],'little')==62
    shoff=struct.unpack_from('<Q',data,40)[0];es,num,names=struct.unpack_from('<HHH',data,58);assert es==64
    ss=[struct.unpack_from('<IIQQQQIIQQ',data,shoff+i*es) for i in range(num)];sn=ss[names];st=data[sn[4]:sn[4]+sn[5]];return {cstr(st,s[0]).decode():{'type':s[1],'flags':s[2],'link':s[6],'info':s[7],'align':s[8],'entry':s[9],'data':data[s[4]:s[4]+s[5]]} for s in ss}
def inspect(path):
    with path.open('rb') as f,mmap.mmap(f.fileno(),0,access=mmap.ACCESS_READ) as b:
        assert b[:8]==b'!<arch>\n';at=8;names=b'';members=[];offsets={};table=None;special={}
        while at<len(b):
            h=b[at:at+60];assert h.endswith(b'`\n');l=int(h[48:58]);name=h[:16].rstrip();s=at+60;e=s+l;assert e<=len(b)
            if name==b'/':assert table is None;table=b[s:e]
            elif name==b'//':names=b[s:e]
            else:
                if name.startswith(b'/') and name[1:].isdigit():name=names[int(name[1:]):].split(b'/\n',1)[0]
                else:name=name.rstrip(b'/')
                name=name.decode('ascii');offsets[at]=len(members);members.append((name,l,digest(memoryview(b)[s:e])))
                if name in ['lib.rmeta','lib.rmeta-link']:special[name]=elf(b[s:e])
            at=e+(e%2)
        assert at==len(b) and table;count=struct.unpack_from('>I',table)[0];cursor=4+4*count;symbols=table[cursor:].split(b'\0');assert 1<=len(symbols)-count<=3 and not any(symbols[count:]);symbols=symbols[:count];assert all(symbols)
        h=hashlib.sha256();indices=[]
        for i,symbol in enumerate(symbols):
            offset=struct.unpack_from('>I',table,4+4*i)[0];mi=offsets[offset];indices.append(mi);h.update(struct.pack('<II',mi,len(symbol)));h.update(symbol)
        wrappers={};objs=members[2:];assert [x[0] for x in members[:2]]==['lib.rmeta','lib.rmeta-link'] and all(x[0].endswith('.rcgu.o') for x in objs)
        for name,secs in special.items():
            meta='.rmeta' if name=='lib.rmeta' else '.rmeta-link';assert set(secs)=={'','.note.gnu.property',meta,'.symtab','.strtab','.shstrtab'} and secs[meta]['flags']==0x80000000 and secs['.symtab']['data']==bytes(24) and secs['.strtab']['data']==b'\0';assert not any(s['flags']&2 for s in secs.values())
            wrappers[name]={k:{**{a:v for a,v in val.items() if a!='data'},'sha256':digest(val['data']),'bytes':len(val['data'])} for k,val in secs.items()}
        encoded=special['lib.rmeta-link']['.rmeta-link']['data'];n,i=leb(encoded,0);declared=[]
        for _ in range(n):
            ln,i=leb(encoded,i);declared.append(encoded[i:i+ln].decode());i+=ln;assert encoded[i]==193;i+=1
        assert declared==[x[0] for x in objs];assert encoded[i:]==b'\x00rust-end-file';assert all(x>=2 for x in indices)
        result={'sha256':digest(b),'symbol_index_entries':count,'symbol_name_member_ordinal_sha256':h.hexdigest(),'referenced_object_member_ordinals':sorted(set(indices)),'objects':objs,'wrappers':wrappers,'rmeta_link':{'object_names':n,'rust_object_order_matches_archive':True,'native_lib_filenames':[]}}
        return result
